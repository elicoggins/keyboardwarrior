// The audio engine: a cpal output stream with our own mixer.
//
// Why this exists: rhythm-game sync cannot be bolted onto a fire-and-forget
// sound API. Here the game clock IS the audio hardware's frame counter — the
// callback counts every frame delivered to the device, music stems begin at
// an exact frame index, and one-shots can be scheduled at exact timeline
// positions. Sync is guaranteed by construction, not queried from a library.
//
// Everything is mixed in one place: song stems (backing + duckable lead) and
// synthesized one-shots (drums, plucks, UI ticks) share the same callback.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

/// A decoded, device-rate, stereo sound. Cheap to clone.
pub type Buf = Arc<Vec<[f32; 2]>>;

enum Cmd {
    /// Play a one-shot now.
    Play { buf: Buf, vol: f32 },
    /// Play a one-shot at an exact timeline position (seconds).
    PlayAt { buf: Buf, vol: f32, time: f64 },
    /// Begin a timeline whose zero lands at global frame `start_frame`,
    /// with optional music stems that start exactly there.
    StartTimeline { start_frame: u64, backing: Option<Buf>, lead: Option<Buf> },
    /// Duck or restore the lead stem (smoothed in the callback).
    SetLeadGain(f32),
    StopTimeline,
}

struct Voice {
    buf: Buf,
    vol: f32,
    pos: usize,
    start_frame: u64,
}

struct Timeline {
    start_frame: u64,
    backing: Option<Buf>,
    lead: Option<Buf>,
    lead_gain: f32,
    lead_target: f32,
}

struct ClockSmooth {
    offset: f64, // audio-clock minus wall-clock, low-passed
    init: bool,
}

pub struct AudioEngine {
    pub sample_rate: u32,
    frames: Arc<AtomicU64>, // total frames submitted to the device
    tx: Sender<Cmd>,
    timeline_start: AtomicU64, // game-side copy, frames
    epoch: Instant,
    smooth: Mutex<ClockSmooth>,
    _stream: cpal::Stream,
}

impl AudioEngine {
    pub fn new() -> AudioEngine {
        let host = cpal::default_host();
        let device = host.default_output_device().expect("no audio output device");
        let config = device.default_output_config().expect("no default audio config");
        let sample_rate = config.sample_rate().0;
        let channels = config.channels() as usize;

        let frames = Arc::new(AtomicU64::new(0));
        let (tx, rx) = channel::<Cmd>();

        let stream = match config.sample_format() {
            cpal::SampleFormat::F32 => {
                build_stream::<f32>(&device, &config.into(), channels, frames.clone(), rx)
            }
            cpal::SampleFormat::I16 => {
                build_stream::<i16>(&device, &config.into(), channels, frames.clone(), rx)
            }
            other => panic!("unsupported sample format: {other:?}"),
        };
        stream.play().expect("failed to start audio stream");

        AudioEngine {
            sample_rate,
            frames,
            tx,
            timeline_start: AtomicU64::new(u64::MAX),
            epoch: Instant::now(),
            smooth: Mutex::new(ClockSmooth { offset: 0.0, init: false }),
            _stream: stream,
        }
    }

    /// Fire a one-shot immediately (UI ticks, judgement feedback).
    pub fn play(&self, buf: &Buf, vol: f32) {
        let _ = self.tx.send(Cmd::Play { buf: buf.clone(), vol });
    }

    /// Schedule a one-shot at an exact timeline position — used by the
    /// built-in song so its synth events land sample-accurately.
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
        let wall = self.epoch.elapsed().as_secs_f64();
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

fn build_stream<T: cpal::SizedSample + cpal::FromSample<f32>>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    channels: usize,
    frames: Arc<AtomicU64>,
    rx: Receiver<Cmd>,
) -> cpal::Stream {
    let mut voices: Vec<Voice> = Vec::new();
    let mut timeline: Option<Timeline> = None;
    let sample_rate = config.sample_rate.0 as f64;

    device
        .build_output_stream(
            config,
            move |out: &mut [T], _| {
                // Apply pending commands
                while let Ok(cmd) = rx.try_recv() {
                    let now = frames.load(Ordering::Relaxed);
                    match cmd {
                        Cmd::Play { buf, vol } => {
                            voices.push(Voice { buf, vol, pos: 0, start_frame: now });
                        }
                        Cmd::PlayAt { buf, vol, time } => {
                            let start_frame = match &timeline {
                                Some(t) => {
                                    let f = t.start_frame as f64 + time * sample_rate;
                                    f.max(now as f64) as u64
                                }
                                None => now,
                            };
                            voices.push(Voice { buf, vol, pos: 0, start_frame });
                        }
                        Cmd::StartTimeline { start_frame, backing, lead } => {
                            timeline = Some(Timeline {
                                start_frame,
                                backing,
                                lead,
                                lead_gain: 1.0,
                                lead_target: 1.0,
                            });
                        }
                        Cmd::SetLeadGain(g) => {
                            if let Some(t) = timeline.as_mut() {
                                t.lead_target = g;
                            }
                        }
                        Cmd::StopTimeline => timeline = None,
                    }
                }

                let start = frames.load(Ordering::Relaxed);
                let nframes = out.len() / channels;
                // ~8 ms exponential gain smoothing for the lead stem
                let gain_k = 1.0 - (-1.0 / (0.008 * sample_rate as f32)).exp();

                for i in 0..nframes {
                    let gf = start + i as u64;
                    let mut l = 0.0f32;
                    let mut r = 0.0f32;

                    if let Some(t) = timeline.as_mut() {
                        if gf >= t.start_frame {
                            let idx = (gf - t.start_frame) as usize;
                            if let Some(b) = &t.backing {
                                if let Some(s) = b.get(idx) {
                                    l += s[0];
                                    r += s[1];
                                }
                            }
                            t.lead_gain += (t.lead_target - t.lead_gain) * gain_k;
                            if let Some(ld) = &t.lead {
                                if let Some(s) = ld.get(idx) {
                                    l += s[0] * t.lead_gain;
                                    r += s[1] * t.lead_gain;
                                }
                            }
                        }
                    }

                    for v in voices.iter_mut() {
                        if gf >= v.start_frame {
                            if let Some(s) = v.buf.get(v.pos) {
                                l += s[0] * v.vol;
                                r += s[1] * v.vol;
                                v.pos += 1;
                            }
                        }
                    }

                    // Soft-clip to keep stem sums from cracking
                    let (l, r) = (soft_clip(l), soft_clip(r));
                    out[i * channels] = T::from_sample(l);
                    if channels > 1 {
                        out[i * channels + 1] = T::from_sample(r);
                    }
                    for c in 2..channels {
                        out[i * channels + c] = T::from_sample(0.0f32);
                    }
                }

                voices.retain(|v| v.pos < v.buf.len());
                frames.store(start + nframes as u64, Ordering::Relaxed);
            },
            |err| eprintln!("audio stream error: {err}"),
            None,
        )
        .expect("failed to build audio stream")
}

fn soft_clip(x: f32) -> f32 {
    if x.abs() <= 0.95 {
        x
    } else {
        x.signum() * (0.95 + (x.abs() - 0.95).tanh() * 0.05)
    }
}
