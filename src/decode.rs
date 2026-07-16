// Audio decoding: turns compressed stem bytes into device-rate stereo f32.
// .opus goes through libopus (Chorus Encore transcodes everything to opus);
// ogg-vorbis, mp3, wav, and flac go through symphonia. No external tools,
// no pre-conversion — Clone Hero files play as downloaded.

use std::io::Cursor;
use std::sync::Arc;

use crate::audio::Buf;

/// Decode `bytes` (format inferred from `name`'s extension) and resample to
/// `out_rate`.
pub fn decode(bytes: &[u8], name: &str, out_rate: u32) -> Result<Buf, String> {
    let ext = name.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
    let (frames, rate) = match ext.as_str() {
        "opus" => decode_opus(bytes)?,
        _ => decode_symphonia(bytes, &ext)?,
    };
    Ok(Arc::new(resample(frames, rate, out_rate)))
}

// ---------------------------------------------------------------- opus

fn decode_opus(bytes: &[u8]) -> Result<(Vec<[f32; 2]>, u32), String> {
    let mut reader = ogg::PacketReader::new(Cursor::new(bytes));
    let mut decoder: Option<opus::Decoder> = None;
    let mut channels = 2usize;
    let mut pre_skip = 0usize;
    let mut out: Vec<[f32; 2]> = Vec::new();
    let mut pcm = vec![0f32; 5760 * 2]; // max opus frame at 48k, stereo

    while let Ok(Some(packet)) = reader.read_packet() {
        let data = &packet.data;
        if data.starts_with(b"OpusHead") {
            if data.len() < 19 {
                return Err("malformed OpusHead header".into());
            }
            channels = data[9] as usize;
            pre_skip = u16::from_le_bytes([data[10], data[11]]) as usize;
            let ch = if channels == 1 { opus::Channels::Mono } else { opus::Channels::Stereo };
            decoder = opus::Decoder::new(48000, ch).ok();
            continue;
        }
        if data.starts_with(b"OpusTags") {
            continue;
        }
        let Some(dec) = decoder.as_mut() else { continue };
        let Ok(n) = dec.decode_float(data, &mut pcm, false) else { continue };
        for i in 0..n {
            let (l, r) =
                if channels == 1 { (pcm[i], pcm[i]) } else { (pcm[i * 2], pcm[i * 2 + 1]) };
            out.push([l, r]);
        }
    }
    if out.is_empty() {
        return Err(if decoder.is_none() {
            "no OpusHead found — not an ogg-opus stream".into()
        } else {
            "opus stream decoded to no audio".into()
        });
    }
    // OpusHead declares encoder padding to drop from the start
    if pre_skip > 0 && pre_skip < out.len() {
        out.drain(..pre_skip);
    }
    Ok((out, 48000))
}

// ---------------------------------------------------------------- symphonia

fn decode_symphonia(bytes: &[u8], ext: &str) -> Result<(Vec<[f32; 2]>, u32), String> {
    use symphonia::core::audio::SampleBuffer;
    use symphonia::core::codecs::DecoderOptions;
    use symphonia::core::formats::FormatOptions;
    use symphonia::core::io::MediaSourceStream;
    use symphonia::core::meta::MetadataOptions;
    use symphonia::core::probe::Hint;

    let mss = MediaSourceStream::new(Box::new(Cursor::new(bytes.to_vec())), Default::default());
    let mut hint = Hint::new();
    hint.with_extension(ext);
    let probed = symphonia::default::get_probe()
        .format(&hint, mss, &FormatOptions::default(), &MetadataOptions::default())
        .map_err(|e| format!("unrecognized {ext} stream: {e}"))?;
    let mut format = probed.format;
    let track = format.default_track().ok_or("no audio track in stream")?.clone();
    let mut decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &DecoderOptions::default())
        .map_err(|e| format!("codec init failed: {e}"))?;

    let mut rate = track.codec_params.sample_rate.unwrap_or(44100);
    let mut out: Vec<[f32; 2]> = Vec::new();
    let mut sample_buf: Option<SampleBuffer<f32>> = None;

    while let Ok(packet) = format.next_packet() {
        if packet.track_id() != track.id {
            continue;
        }
        let Ok(decoded) = decoder.decode(&packet) else { continue };
        let spec = *decoded.spec();
        rate = spec.rate;
        let channels = spec.channels.count();
        let buf = sample_buf
            .get_or_insert_with(|| SampleBuffer::<f32>::new(decoded.capacity() as u64, spec));
        buf.copy_interleaved_ref(decoded);
        let samples = buf.samples();
        match channels {
            1 => out.extend(samples.iter().map(|&s| [s, s])),
            _ => out.extend(samples.chunks_exact(channels).map(|c| [c[0], c[1]])),
        }
    }
    if out.is_empty() {
        Err("stream decoded to no audio".into())
    } else {
        Ok((out, rate))
    }
}

// ---------------------------------------------------------------- resample

/// Linear-interpolation resampler. Runs once at load time.
fn resample(input: Vec<[f32; 2]>, from: u32, to: u32) -> Vec<[f32; 2]> {
    if from == to || input.is_empty() {
        return input;
    }
    let ratio = from as f64 / to as f64;
    let out_len = (input.len() as f64 / ratio) as usize;
    let mut out = Vec::with_capacity(out_len);
    for i in 0..out_len {
        let src = i as f64 * ratio;
        let i0 = src as usize;
        let i1 = (i0 + 1).min(input.len() - 1);
        let t = (src - i0 as f64) as f32;
        let a = input[i0];
        let b = input[i1];
        out.push([a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t]);
    }
    out
}

/// Sum one stem into the running backing mix, growing it as needed. Stems are
/// mixed one at a time as they decode, so peak memory is the mix plus a
/// single stem — never every decoded stem at once.
pub fn mix_into(mix: &mut Vec<[f32; 2]>, stem: &[[f32; 2]]) {
    if mix.len() < stem.len() {
        mix.resize(stem.len(), [0.0; 2]);
    }
    for (m, s) in mix.iter_mut().zip(stem.iter()) {
        m[0] += s[0];
        m[1] += s[1];
    }
}

/// Normalize the summed backing (and lead together) if the sum would clip.
pub fn finalize_mix(mut mixed: Vec<[f32; 2]>, lead: Option<Buf>) -> (Buf, Option<Buf>) {
    let peak = |buf: &[[f32; 2]]| buf.iter().fold(0f32, |m, s| m.max(s[0].abs()).max(s[1].abs()));
    let mut max = peak(&mixed);
    if let Some(l) = &lead {
        // The lead plays on top of the backing; scale for the combined peak
        max = max.max(peak(l) + max * 0.2);
    }
    if max > 1.0 {
        let k = 0.98 / max;
        for s in mixed.iter_mut() {
            s[0] *= k;
            s[1] *= k;
        }
        if let Some(l) = lead {
            let scaled: Vec<[f32; 2]> = l.iter().map(|s| [s[0] * k, s[1] * k]).collect();
            return (Arc::new(mixed), Some(Arc::new(scaled)));
        }
    }
    (Arc::new(mixed), lead)
}
