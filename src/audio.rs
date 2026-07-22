// The audio engine: an output stream with our own mixer.
//
// Why this exists: rhythm-game sync cannot be bolted onto a fire-and-forget
// sound API. Here the game clock IS the audio hardware's frame counter — the
// callback counts every frame delivered to the device, music stems begin at
// an exact frame index, and one-shots can be scheduled at exact timeline
// positions. Sync is guaranteed by construction, not queried from a library.
//
// Everything is mixed in one place: song stems (backing + duckable lead) and
// synthesized one-shots (drums, plucks, UI ticks) share the same callback.
//
// The mixer itself (`Mixer`) is backend-agnostic. Natively it runs inside a
// cpal callback on the real-time audio thread; in the browser demo the same
// mixer is pulled from JS by a ScriptProcessorNode (see web/kw_audio.js), so
// both versions play byte-identical mixes.
//
// Real-time hygiene: the callback never frees big buffers — retired stems are
// shipped back to the main thread over a channel and dropped there (reap()) —
// and the voice list is pre-reserved so ordinary play doesn't allocate.

use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};
#[cfg(not(target_arch = "wasm32"))]
use std::time::Instant;

#[cfg(not(target_arch = "wasm32"))]
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

/// A decoded, device-rate, stereo sound. Cheap to clone.
pub type Buf = Arc<Vec<[f32; 2]>>;

/// Whammy pump rate in Hz — how fast the bar bobs while held. The renderer
/// pulses the sustain tail at the same rate, so width and pitch breathe as one.
pub const WH_PUMP_HZ: f64 = 4.8;

enum Cmd {
    /// Play a one-shot now.
    Play {
        buf: Buf,
        vol: f32,
    },
    /// Play a one-shot at an exact timeline position (seconds).
    PlayAt {
        buf: Buf,
        vol: f32,
        time: f64,
    },
    /// Begin a timeline whose zero lands at global frame `start_frame`,
    /// with optional music stems that start exactly there.
    StartTimeline {
        start_frame: u64,
        backing: Option<Buf>,
        lead: Option<Buf>,
    },
    /// Duck or restore the lead stem (smoothed in the callback).
    SetLeadGain(f32),
    /// Whammy bar position 0..1: 1 holds the lead bent-down and doubled
    /// (fatter), 0 releases it back to the plain stem.
    SetWhammy(f32),
    /// Star power reverb send 0..1 — how much of the LEAD stem feeds the
    /// hall. Lead only: the backing (vocals, drums, …) is premixed and
    /// always plays dry. Smoothed in the callback; the tail rings out.
    SetSpFx(f32),
    /// Freeze or resume the timeline (and everything scheduled on it).
    SetPaused(bool),
    StopTimeline,
}

struct Voice {
    buf: Buf,
    vol: f32,
    pos: usize,
    start_frame: u64,
    scheduled: bool, // timeline-relative: shifts with the timeline while paused
}

struct Timeline {
    start_frame: u64,
    backing: Option<Buf>,
    lead: Option<Buf>,
    lead_gain: f32,
    lead_target: f32,
    whammy: f32, // bar position, smoothed toward whammy_target in the callback
    whammy_target: f32,
    wh_phase: f64, // grain phase of the pitch-down voice, 0..1
    wh_lfo: f64,   // pump phase 0..1 — the bar bobbing up and down while held
    paused: bool,
}

struct ClockSmooth {
    offset: f64, // audio-clock minus wall-clock, low-passed
    init: bool,
}

// ------------------------------------------------------- star power reverb
//
// A small Freeverb-shaped hall for the lead stem while star power burns:
// eight damped combs and three allpass diffusers per channel, the right
// channel's lines a few samples longer for stereo width. Deliberately
// quiet — a touch of air behind the guitar, not a room around the mix.

const VERB_FB: f32 = 0.82; // comb feedback: tail just over a second
const VERB_DAMP: f32 = 0.35; // highs roll off each repeat
const VERB_IN: f32 = 0.015; // input scaling ahead of the comb bank
const VERB_LEVEL: f32 = 1.0; // overall wet level — the subtlety knob

struct Comb {
    buf: Vec<f32>,
    i: usize,
    store: f32, // one-pole lowpass state in the feedback path
}

impl Comb {
    fn new(len: usize) -> Comb {
        Comb { buf: vec![0.0; len], i: 0, store: 0.0 }
    }
    fn tick(&mut self, x: f32) -> f32 {
        let y = self.buf[self.i];
        self.store = y + (self.store - y) * VERB_DAMP;
        self.buf[self.i] = x + self.store * VERB_FB;
        self.i += 1;
        if self.i == self.buf.len() {
            self.i = 0;
        }
        y
    }
}

struct Allpass {
    buf: Vec<f32>,
    i: usize,
}

impl Allpass {
    fn new(len: usize) -> Allpass {
        Allpass { buf: vec![0.0; len], i: 0 }
    }
    fn tick(&mut self, x: f32) -> f32 {
        let b = self.buf[self.i];
        self.buf[self.i] = x + b * 0.5;
        self.i += 1;
        if self.i == self.buf.len() {
            self.i = 0;
        }
        b - x
    }
}

struct Reverb {
    combs_l: Vec<Comb>,
    combs_r: Vec<Comb>,
    aps_l: Vec<Allpass>,
    aps_r: Vec<Allpass>,
}

impl Reverb {
    fn new(rate: f64) -> Reverb {
        // Freeverb's classic tunings (in seconds of delay), scaled to the
        // device rate; the right channel runs 23 samples behind the left
        const COMBS: [f64; 8] =
            [0.02532, 0.02694, 0.02896, 0.03075, 0.03225, 0.03381, 0.03530, 0.03667];
        const APS: [f64; 3] = [0.01261, 0.01000, 0.00773];
        let len = |sec: f64, spread: usize| (sec * rate) as usize + spread + 1;
        Reverb {
            combs_l: COMBS.iter().map(|&s| Comb::new(len(s, 0))).collect(),
            combs_r: COMBS.iter().map(|&s| Comb::new(len(s, 23))).collect(),
            aps_l: APS.iter().map(|&s| Allpass::new(len(s, 0))).collect(),
            aps_r: APS.iter().map(|&s| Allpass::new(len(s, 23))).collect(),
        }
    }

    fn tick(&mut self, l: f32, r: f32) -> (f32, f32) {
        let (fl, fr) = (l * VERB_IN, r * VERB_IN);
        let mut ol = 0.0;
        for c in &mut self.combs_l {
            ol += c.tick(fl);
        }
        let mut or_ = 0.0;
        for c in &mut self.combs_r {
            or_ += c.tick(fr);
        }
        for a in &mut self.aps_l {
            ol = a.tick(ol);
        }
        for a in &mut self.aps_r {
            or_ = a.tick(or_);
        }
        (ol * VERB_LEVEL, or_ * VERB_LEVEL)
    }
}

/// The mixer state and per-buffer DSP, shared by every backend: commands
/// arrive over a channel, `process` fills interleaved f32 frames and
/// advances the global frame counter.
struct Mixer {
    channels: usize,
    sample_rate: f64,
    voices: Vec<Voice>,
    timeline: Option<Timeline>,
    verb: Reverb,
    sp_wet: f32,        // star power reverb send, smoothed in the callback
    sp_wet_target: f32, // where the send is headed
    cur_master: f32,
    frames: Arc<AtomicU64>,
    timeline_start: Arc<AtomicU64>,
    master: Arc<AtomicU32>,
    rx: Receiver<Cmd>,
    garbage: Sender<Buf>,
}

impl Mixer {
    #[allow(clippy::too_many_arguments)]
    fn new(
        channels: usize,
        sample_rate: u32,
        frames: Arc<AtomicU64>,
        timeline_start: Arc<AtomicU64>,
        master: Arc<AtomicU32>,
        rx: Receiver<Cmd>,
        garbage: Sender<Buf>,
    ) -> Mixer {
        Mixer {
            channels,
            sample_rate: sample_rate as f64,
            voices: Vec::with_capacity(64),
            timeline: None,
            verb: Reverb::new(sample_rate as f64),
            sp_wet: 0.0,
            sp_wet_target: 0.0,
            cur_master: f32::from_bits(master.load(Ordering::Relaxed)),
            frames,
            timeline_start,
            master,
            rx,
            garbage,
        }
    }

    /// Mix one buffer of interleaved f32 frames. Every sample of `out` is
    /// written (no accumulation), so the caller never needs to zero it.
    fn process(&mut self, out: &mut [f32]) {
        let channels = self.channels;
        let sample_rate = self.sample_rate;
        // Apply pending commands
        while let Ok(cmd) = self.rx.try_recv() {
            let now = self.frames.load(Ordering::Relaxed);
            match cmd {
                Cmd::Play { buf, vol } => {
                    self.voices.push(Voice {
                        buf,
                        vol,
                        pos: 0,
                        start_frame: now,
                        scheduled: false,
                    });
                }
                Cmd::PlayAt { buf, vol, time } => {
                    let start_frame = match &self.timeline {
                        Some(t) => {
                            let f = t.start_frame as f64 + time * sample_rate;
                            f.max(now as f64) as u64
                        }
                        None => now,
                    };
                    self.voices.push(Voice { buf, vol, pos: 0, start_frame, scheduled: true });
                }
                Cmd::StartTimeline { start_frame, backing, lead } => {
                    retire(&self.garbage, self.timeline.take());
                    // Unplayed one-shots from the old timeline die with it
                    self.voices.retain(|v| !v.scheduled || v.pos > 0);
                    // A fresh run never inherits the last one's reverb send
                    self.sp_wet_target = 0.0;
                    self.timeline = Some(Timeline {
                        start_frame,
                        backing,
                        lead,
                        lead_gain: 1.0,
                        lead_target: 1.0,
                        whammy: 0.0,
                        whammy_target: 0.0,
                        wh_phase: 0.0,
                        wh_lfo: 0.0,
                        paused: false,
                    });
                }
                Cmd::SetLeadGain(g) => {
                    if let Some(t) = self.timeline.as_mut() {
                        t.lead_target = g;
                    }
                }
                Cmd::SetWhammy(a) => {
                    if let Some(t) = self.timeline.as_mut() {
                        t.whammy_target = a.clamp(0.0, 1.0);
                    }
                }
                Cmd::SetSpFx(a) => {
                    self.sp_wet_target = a.clamp(0.0, 1.0);
                }
                Cmd::SetPaused(p) => {
                    if let Some(t) = self.timeline.as_mut() {
                        t.paused = p;
                    }
                }
                Cmd::StopTimeline => {
                    retire(&self.garbage, self.timeline.take());
                    self.voices.retain(|v| !v.scheduled || v.pos > 0);
                    self.sp_wet_target = 0.0;
                }
            }
        }

        let start = self.frames.load(Ordering::Relaxed);
        let nframes = out.len() / channels;
        // ~8 ms exponential gain smoothing for the lead stem
        let gain_k = 1.0 - (-1.0 / (0.008 * sample_rate as f32)).exp();
        // Whammy: while the bar is down a granular pitch-down voice
        // (dual taps on a drifting delay, triangle-crossfaded) is
        // layered with the dry stem — a fatter, doubled sound whose
        // bend depth rides a pump LFO, like a player bobbing the bar,
        // so the pitch dives and recovers GH-style. Bar travel is
        // eased over ~80 ms so press and release both glide.
        let whammy_k = 1.0 - (-1.0 / (0.08 * sample_rate as f32)).exp();
        // ~0.25 s ease on the star power reverb send
        let wet_k = 1.0 - (-1.0 / (0.25 * sample_rate as f32)).exp();
        let wh_win = 0.06 * sample_rate as f32; // grain window, samples
        const WH_BEND: f32 = 0.09; // ~1.6 semitones down at full dive
        let master_target = f32::from_bits(self.master.load(Ordering::Relaxed));
        let paused = self.timeline.as_ref().is_some_and(|t| t.paused);

        for i in 0..nframes {
            let gf = start + i as u64;
            let mut l = 0.0f32;
            let mut r = 0.0f32;
            // The lead stem's contribution this frame — the reverb's feed.
            // Lead only: the backing is a premixed bus (vocals, drums, …)
            // and a hall around the whole band reads as mud, not an effect.
            let mut fx_l = 0.0f32;
            let mut fx_r = 0.0f32;

            if let Some(t) = self.timeline.as_mut() {
                if !t.paused && gf >= t.start_frame {
                    let idx = (gf - t.start_frame) as usize;
                    // Whammy: ease the bar, pump the LFO, and advance
                    // the grain phase by the momentary dive depth —
                    // the pitch wobbles down and back while held
                    t.whammy += (t.whammy_target - t.whammy) * whammy_k;
                    let wh = t.whammy;
                    let bend = wh > 0.001;
                    if bend {
                        t.wh_lfo = (t.wh_lfo + WH_PUMP_HZ / sample_rate).fract();
                        let pump = 0.5 - 0.5 * (std::f64::consts::TAU * t.wh_lfo).cos() as f32;
                        let depth = wh * (0.30 + 0.70 * pump);
                        t.wh_phase = (t.wh_phase + (depth * WH_BEND / wh_win) as f64).fract();
                    } else {
                        // Each fresh press starts the pump from the top
                        t.wh_lfo = 0.0;
                    }
                    if let Some(b) = &t.backing {
                        // Backing (vocals, drums, bass, …) always plays dry:
                        // the whammy is a lead-guitar effect and must never
                        // touch the rest of the mix. A single-stem song (no
                        // isolated lead) therefore gets the bar's on-screen bow
                        // but no audio bend, instead of the whole mix — vocals
                        // and all — being run through the pitch-down voice.
                        let s = b.get(idx).copied().unwrap_or([0.0; 2]);
                        l += s[0];
                        r += s[1];
                    }
                    t.lead_gain += (t.lead_target - t.lead_gain) * gain_k;
                    if let Some(ld) = &t.lead {
                        let s = if bend {
                            whammy_mix(ld, idx, wh, t.wh_phase, wh_win)
                        } else {
                            ld.get(idx).copied().unwrap_or([0.0; 2])
                        };
                        l += s[0] * t.lead_gain;
                        r += s[1] * t.lead_gain;
                        fx_l = s[0] * t.lead_gain;
                        fx_r = s[1] * t.lead_gain;
                    }
                }
            }

            // Star power reverb: ticked every frame (a silent feed is a
            // handful of multiplies) so the tail rings out naturally when
            // the power ends instead of being cut off.
            self.sp_wet += (self.sp_wet_target - self.sp_wet) * wet_k;
            let (vl, vr) = self.verb.tick(fx_l * self.sp_wet, fx_r * self.sp_wet);
            l += vl;
            r += vr;

            for v in self.voices.iter_mut() {
                if gf >= v.start_frame && !(paused && v.scheduled) {
                    if let Some(s) = v.buf.get(v.pos) {
                        l += s[0] * v.vol;
                        r += s[1] * v.vol;
                        v.pos += 1;
                    }
                }
            }

            // Master volume, smoothed like the lead gain
            self.cur_master += (master_target - self.cur_master) * gain_k;
            let (l, r) = (l * self.cur_master, r * self.cur_master);

            // Soft-clip to keep stem sums from cracking
            let (l, r) = (soft_clip(l), soft_clip(r));
            out[i * channels] = l;
            if channels > 1 {
                out[i * channels + 1] = r;
            }
            for c in 2..channels {
                out[i * channels + c] = 0.0;
            }
        }

        // While paused the timeline start slides forward with the
        // device clock, so its position (and every not-yet-started
        // one-shot scheduled on it) holds still.
        if paused {
            if let Some(t) = self.timeline.as_mut() {
                t.start_frame += nframes as u64;
                self.timeline_start.store(t.start_frame, Ordering::Relaxed);
            }
            for v in self.voices.iter_mut() {
                if v.scheduled && v.pos == 0 {
                    v.start_frame += nframes as u64;
                }
            }
        }

        self.voices.retain(|v| v.pos < v.buf.len());
        self.frames.store(start + nframes as u64, Ordering::Relaxed);
    }
}

pub struct AudioEngine {
    pub sample_rate: u32,
    frames: Arc<AtomicU64>, // total frames submitted to the device
    tx: Sender<Cmd>,
    // Shared with the callback: while paused the callback slides this forward
    // in lockstep with the device clock so the position holds still.
    timeline_start: Arc<AtomicU64>,
    // Master volume 0..1 as f32 bits, applied to the whole mix in the
    // callback (smoothed there, so steps never zipper)
    master: Arc<AtomicU32>,
    #[cfg(not(target_arch = "wasm32"))]
    epoch: Instant,
    #[cfg(target_arch = "wasm32")]
    epoch: f64, // seconds, miniquad date clock
    smooth: Mutex<ClockSmooth>,
    garbage_rx: Receiver<Buf>, // buffers retired by the callback, dropped here
    #[cfg(not(target_arch = "wasm32"))]
    _stream: cpal::Stream,
}

impl AudioEngine {
    #[cfg(not(target_arch = "wasm32"))]
    pub fn new() -> AudioEngine {
        let host = cpal::default_host();
        let device = host.default_output_device().expect("no audio output device");
        let config = device.default_output_config().expect("no default audio config");
        let sample_rate = config.sample_rate().0;
        let channels = config.channels() as usize;

        let frames = Arc::new(AtomicU64::new(0));
        let timeline_start = Arc::new(AtomicU64::new(u64::MAX));
        let master = Arc::new(AtomicU32::new(1.0f32.to_bits()));
        let (tx, rx) = channel::<Cmd>();
        let (garbage_tx, garbage_rx) = channel::<Buf>();

        let stream = match config.sample_format() {
            cpal::SampleFormat::F32 => build_stream::<f32>(
                &device,
                &config.into(),
                channels,
                frames.clone(),
                timeline_start.clone(),
                master.clone(),
                rx,
                garbage_tx,
            ),
            cpal::SampleFormat::I16 => build_stream::<i16>(
                &device,
                &config.into(),
                channels,
                frames.clone(),
                timeline_start.clone(),
                master.clone(),
                rx,
                garbage_tx,
            ),
            other => panic!("unsupported sample format: {other:?}"),
        };
        stream.play().expect("failed to start audio stream");

        AudioEngine {
            sample_rate,
            frames,
            tx,
            timeline_start,
            master,
            epoch: Instant::now(),
            smooth: Mutex::new(ClockSmooth { offset: 0.0, init: false }),
            garbage_rx,
            _stream: stream,
        }
    }

    /// Browser demo: the same mixer, pulled from JS by a ScriptProcessorNode.
    /// `kw_audio_start` (web/kw_audio.js) creates the AudioContext and node,
    /// and the node's callback calls back into `kw_render` for every buffer.
    #[cfg(target_arch = "wasm32")]
    pub fn new() -> AudioEngine {
        let sample_rate = unsafe { web::kw_audio_start() };
        let frames = Arc::new(AtomicU64::new(0));
        let timeline_start = Arc::new(AtomicU64::new(u64::MAX));
        let master = Arc::new(AtomicU32::new(1.0f32.to_bits()));
        let (tx, rx) = channel::<Cmd>();
        let (garbage_tx, garbage_rx) = channel::<Buf>();
        web::install(Mixer::new(
            2,
            sample_rate,
            frames.clone(),
            timeline_start.clone(),
            master.clone(),
            rx,
            garbage_tx,
        ));
        AudioEngine {
            sample_rate,
            frames,
            tx,
            timeline_start,
            master,
            epoch: macroquad::miniquad::date::now(),
            smooth: Mutex::new(ClockSmooth { offset: 0.0, init: false }),
            garbage_rx,
        }
    }

    /// Seconds of wall clock since the engine was created.
    fn wall_elapsed(&self) -> f64 {
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.epoch.elapsed().as_secs_f64()
        }
        #[cfg(target_arch = "wasm32")]
        {
            macroquad::miniquad::date::now() - self.epoch
        }
    }

    /// Fire a one-shot immediately (UI ticks, judgement feedback).
    pub fn play(&self, buf: &Buf, vol: f32) {
        let _ = self.tx.send(Cmd::Play { buf: buf.clone(), vol });
    }

    /// Schedule a one-shot at an exact timeline position — used by count-in
    /// ticks and the calibration metronome so they land sample-accurately.
    pub fn play_at(&self, buf: &Buf, vol: f32, time: f64) {
        let _ = self.tx.send(Cmd::PlayAt { buf: buf.clone(), vol, time });
    }

    /// Start a new timeline. Its zero lands `lead_in` seconds from now, and
    /// the stems (if any) begin at exactly that frame.
    pub fn start_timeline(&self, lead_in: f64, backing: Option<Buf>, lead: Option<Buf>) {
        let now = self.frames.load(Ordering::Relaxed);
        // Small safety pad so the start frame is still in the future when the
        // callback processes the command.
        let start_frame = now + ((lead_in.max(0.05)) * self.sample_rate as f64) as u64;
        self.timeline_start.store(start_frame, Ordering::Relaxed);
        self.smooth.lock().unwrap().init = false;
        let _ = self.tx.send(Cmd::StartTimeline { start_frame, backing, lead });
    }

    pub fn stop_timeline(&self) {
        self.timeline_start.store(u64::MAX, Ordering::Relaxed);
        let _ = self.tx.send(Cmd::StopTimeline);
    }

    pub fn set_lead_gain(&self, gain: f32) {
        let _ = self.tx.send(Cmd::SetLeadGain(gain));
    }

    /// Whammy bar position (0..1). While pressed the lead stays bent down
    /// and doubled with the dry stem (fatter); releasing returns it to the
    /// plain stem. Both transitions glide.
    pub fn set_whammy(&self, amt: f32) {
        let _ = self.tx.send(Cmd::SetWhammy(amt));
    }

    /// Star power reverb send (0..1) on the lead stem: 1 while the power
    /// burns, 0 otherwise. Eased in the callback; the tail rings out.
    pub fn set_sp_fx(&self, amt: f32) {
        let _ = self.tx.send(Cmd::SetSpFx(amt));
    }

    /// Master volume, 0..1.
    pub fn master(&self) -> f32 {
        f32::from_bits(self.master.load(Ordering::Relaxed))
    }

    pub fn set_master(&self, vol: f32) {
        self.master.store(vol.clamp(0.0, 1.0).to_bits(), Ordering::Relaxed);
    }

    /// Freeze or resume the timeline. While paused the callback advances the
    /// timeline's start frame with the device clock, so the position — and
    /// every one-shot scheduled on it — holds still.
    pub fn set_paused(&self, paused: bool) {
        self.smooth.lock().unwrap().init = false;
        let _ = self.tx.send(Cmd::SetPaused(paused));
    }

    /// Drop buffers the callback has retired. Called from the main thread
    /// once per frame so multi-hundred-MB stem frees never happen on the
    /// real-time audio thread.
    pub fn reap(&self) {
        while self.garbage_rx.try_recv().is_ok() {}
    }

    /// Current timeline position in seconds (negative during the count-in).
    ///
    /// The raw value advances in device-buffer steps, so it's blended with
    /// wall clock: the wall clock provides smoothness, the audio clock
    /// provides truth, and the low-passed offset between them removes drift.
    pub fn timeline_pos(&self) -> f64 {
        let start = self.timeline_start.load(Ordering::Relaxed);
        if start == u64::MAX {
            return f64::NEG_INFINITY;
        }
        let frames = self.frames.load(Ordering::Relaxed);
        let raw = (frames as f64 - start as f64) / self.sample_rate as f64;
        let wall = self.wall_elapsed();
        let mut s = self.smooth.lock().unwrap();
        let target = raw - wall;
        if !s.init || (target - s.offset).abs() > 0.06 {
            s.offset = target;
            s.init = true;
        } else {
            s.offset += (target - s.offset) * 0.05;
        }
        wall + s.offset
    }
}

/// Ship a retired timeline's stems to the main thread for dropping. If the
/// engine is already gone (app exit) the send fails and they drop here, which
/// no longer matters.
fn retire(garbage: &Sender<Buf>, timeline: Option<Timeline>) {
    if let Some(t) = timeline {
        for buf in [t.backing, t.lead].into_iter().flatten() {
            let _ = garbage.send(buf);
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[allow(clippy::too_many_arguments)]
fn build_stream<T: cpal::SizedSample + cpal::FromSample<f32>>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    channels: usize,
    frames: Arc<AtomicU64>,
    timeline_start: Arc<AtomicU64>,
    master: Arc<AtomicU32>,
    rx: Receiver<Cmd>,
    garbage: Sender<Buf>,
) -> cpal::Stream {
    let mut mixer =
        Mixer::new(channels, config.sample_rate.0, frames, timeline_start, master, rx, garbage);
    let mut scratch: Vec<f32> = Vec::new();
    device
        .build_output_stream(
            config,
            move |out: &mut [T], _| {
                scratch.resize(out.len(), 0.0);
                mixer.process(&mut scratch);
                for (o, s) in out.iter_mut().zip(scratch.iter()) {
                    *o = T::from_sample(*s);
                }
            },
            |err| eprintln!("audio stream error: {err}"),
            None,
        )
        .expect("failed to build audio stream")
}

// ---------------------------------------------------------------- web backend

/// Browser backend: the mixer lives in a thread-local and is pulled from JS.
/// The page's ScriptProcessorNode runs on the same (only) thread as the game
/// loop, so no synchronization beyond the existing channels is needed.
#[cfg(target_arch = "wasm32")]
mod web {
    use super::Mixer;
    use std::cell::RefCell;

    extern "C" {
        /// Provided by web/kw_audio.js: creates the AudioContext and the
        /// ScriptProcessorNode that pulls `kw_render`, and returns the
        /// context's sample rate.
        pub fn kw_audio_start() -> u32;
    }

    thread_local! {
        static MIXER: RefCell<Option<Mixer>> = const { RefCell::new(None) };
        static SCRATCH: RefCell<Vec<f32>> = const { RefCell::new(Vec::new()) };
    }

    pub fn install(mixer: Mixer) {
        MIXER.with(|m| *m.borrow_mut() = Some(mixer));
    }

    /// Called from JS for every audio buffer: mix `nframes` stereo frames
    /// and return a pointer to the interleaved f32 samples in wasm memory.
    #[no_mangle]
    pub extern "C" fn kw_render(nframes: u32) -> *const f32 {
        SCRATCH.with(|s| {
            let mut s = s.borrow_mut();
            s.resize(nframes as usize * 2, 0.0);
            MIXER.with(|m| match m.borrow_mut().as_mut() {
                Some(mixer) => mixer.process(&mut s),
                None => s.fill(0.0),
            });
            s.as_ptr()
        })
    }
}

/// The whammied stem sample: the dry signal layered with a pitched-down
/// voice. The voice is two taps on a delay that drifts backward (a constant
/// pitch-down), each wrapping where its triangle weight is zero, so grains
/// never click. Weights sum past 1 on purpose — the held note gets fatter.
fn whammy_mix(buf: &[[f32; 2]], idx: usize, wh: f32, phase: f64, win: f32) -> [f32; 2] {
    let dry = buf.get(idx).copied().unwrap_or([0.0; 2]);
    let mut s = [dry[0] * (1.0 - 0.25 * wh), dry[1] * (1.0 - 0.25 * wh)];
    for off in [0.0, 0.5] {
        let q = ((phase + off) % 1.0) as f32;
        let w = (1.0 - (2.0 * q - 1.0).abs()) * 0.85 * wh;
        let tap = sample_at(buf, idx as f64 - (q * win) as f64);
        s[0] += tap[0] * w;
        s[1] += tap[1] * w;
    }
    s
}

/// Read a stereo buffer at a fractional frame position, linearly
/// interpolated — the whammy voice's drifting delay lands between samples.
fn sample_at(buf: &[[f32; 2]], pos: f64) -> [f32; 2] {
    if pos < 0.0 {
        return [0.0, 0.0];
    }
    let i = pos as usize;
    let frac = (pos - i as f64) as f32;
    let a = buf.get(i).copied().unwrap_or([0.0; 2]);
    let b = buf.get(i + 1).copied().unwrap_or(a);
    [a[0] + (b[0] - a[0]) * frac, a[1] + (b[1] - a[1]) * frac]
}

fn soft_clip(x: f32) -> f32 {
    if x.abs() <= 0.95 {
        x
    } else {
        x.signum() * (0.95 + (x.abs() - 0.95).tanh() * 0.05)
    }
}

pub struct Sounds {
    pub kick: Buf,
    pub hat: Buf,
    pub miss: Buf,
    pub sp_start: Buf, // star power ignition — a soft thump under a short air swell
}

// ---------------------------------------------------------------- audio synth

/// Synthesize a mono waveform into a device-rate stereo buffer.
fn synth(rate: u32, dur: f32, f: impl Fn(f32) -> f32) -> Buf {
    let n = (dur * rate as f32) as usize;
    Arc::new(
        (0..n)
            .map(|i| {
                let s = f(i as f32 / rate as f32).clamp(-1.0, 1.0);
                [s, s]
            })
            .collect(),
    )
}

pub fn make_sounds(rate: u32) -> Sounds {
    use std::f32::consts::TAU;
    let noise_cell = std::cell::Cell::new(0x1234_5678u32);
    let noise = || {
        let mut s = noise_cell.get();
        s ^= s << 13;
        s ^= s >> 17;
        s ^= s << 5;
        noise_cell.set(s);
        (s as f32 / u32::MAX as f32) * 2.0 - 1.0
    };

    // Kick: pitch sweep 160 -> 45 Hz with fast decay
    let kick = synth(rate, 0.20, |t| {
        let freq = 45.0 + 115.0 * (-t * 22.0).exp();
        (TAU * freq * t).sin() * (-t * 16.0).exp() * 0.9
    });
    // Hi-hat: short noise burst (crude high-pass via previous-sample subtract)
    let prev = std::cell::Cell::new(0.0f32);
    let hat = synth(rate, 0.05, |t| {
        let raw = noise();
        let hp = raw - prev.get() * 0.7;
        prev.set(raw);
        hp * (-t * 90.0).exp() * 0.30
    });
    // Miss: low buzzy thud
    let miss = synth(rate, 0.22, |t| {
        let square = if (TAU * 108.0 * t).sin() > 0.0 { 1.0 } else { -1.0 };
        square * (-t * 14.0).exp() * 0.30
    });

    // Star power ignition: a soft low thump under a short breath of air —
    // it marks the moment without stealing it from the music
    let lp = std::cell::Cell::new(0.0f32);
    let sp_start = synth(rate, 0.5, |t| {
        let n = noise();
        let mut l = lp.get();
        l += (n - l) * 0.06;
        lp.set(l);
        let air = l * 2.0 * (t * 9.0).min(1.0) * (-((t - 0.15).max(0.0)) * 8.0).exp();
        let thump = (TAU * 70.0 * t).sin() * (-t * 18.0).exp();
        air * 0.35 + thump * 0.4
    });

    Sounds { kick, hat, miss, sp_start }
}
