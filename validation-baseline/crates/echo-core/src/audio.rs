use crate::model::{AudioSummary, Config, Event, RidgePoint, Spectrogram};
use rustfft::{num_complex::Complex, FftPlanner};
use std::{io::Cursor, sync::Arc};
use symphonia::core::{
    audio::SampleBuffer, codecs::DecoderOptions, errors::Error, formats::FormatOptions,
    io::MediaSourceStream, meta::MetadataOptions, probe::Hint,
};

pub struct Decoded {
    pub channels: Vec<Vec<f32>>,
    pub rate: u32,
    pub full_duration_s: f64,
    pub truncated: bool,
}
pub fn decode(bytes: &[u8], max_s: f64) -> Result<Decoded, String> {
    if bytes.is_empty() || bytes.len() > 64 * 1024 * 1024 {
        return Err("Audio must be between 1 byte and 64 MiB".into());
    }
    let mss = MediaSourceStream::new(Box::new(Cursor::new(bytes.to_vec())), Default::default());
    let probed = symphonia::default::get_probe()
        .format(
            &Hint::new(),
            mss,
            &FormatOptions::default(),
            &MetadataOptions::default(),
        )
        .map_err(|e| format!("Unsupported audio: {e}"))?;
    let mut format = probed.format;
    let track = format.default_track().ok_or("Audio track missing")?;
    let id = track.id;
    let params = track.codec_params.clone();
    let rate = params.sample_rate.ok_or("Sample rate missing")?;
    if !(1000..=192_000).contains(&rate) {
        return Err("Sample rate outside 1–192 kHz".into());
    }
    let full_duration_s = params
        .n_frames
        .map(|n| n as f64 / rate as f64)
        .unwrap_or(0.);
    let mut decoder = symphonia::default::get_codecs()
        .make(&params, &DecoderOptions::default())
        .map_err(|e| format!("Decoder unavailable: {e}"))?;
    let mut channels: Vec<Vec<f32>> = Vec::new();
    let mut truncated = false;
    loop {
        let packet = match format.next_packet() {
            Ok(p) => p,
            Err(Error::IoError(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
            Err(e) => return Err(format!("Demux error: {e}")),
        };
        if packet.track_id() != id {
            continue;
        }
        let decoded = decoder
            .decode(&packet)
            .map_err(|e| format!("Damaged audio packet: {e}"))?;
        if decoded.spec().rate != rate {
            return Err("Changing sample rate is unsupported; split the recording".into());
        }
        let n = decoded.spec().channels.count();
        if n == 0 || n > 8 {
            return Err("Only 1–8 audio channels supported".into());
        }
        if channels.is_empty() {
            channels = vec![Vec::new(); n];
        } else if channels.len() != n {
            return Err("Changing channel layout is unsupported".into());
        }
        let mut buf = SampleBuffer::<f32>::new(decoded.capacity() as u64, *decoded.spec());
        buf.copy_interleaved_ref(decoded);
        for frame in buf.samples().chunks_exact(n) {
            if channels[0].len() >= ((max_s * rate as f64) as usize)
                || channels[0].len() * n >= 32_000_000
            {
                truncated = true;
                break;
            }
            for (j, s) in frame.iter().enumerate() {
                channels[j].push(if s.is_finite() { *s } else { 0. });
            }
        }
        if truncated {
            break;
        }
    }
    if channels.is_empty() || channels[0].is_empty() {
        return Err("No decodable samples".into());
    }
    let duration = channels[0].len() as f64 / rate as f64;
    Ok(Decoded {
        channels,
        rate,
        full_duration_s: full_duration_s.max(duration),
        truncated,
    })
}
/// Windowed-sinc decimation. Averages channels nowhere: anti-phase stereo remains usable.
fn resample(input: &[f32], source: u32, target: u32) -> Vec<f32> {
    if source <= target {
        return input.to_vec();
    }
    let ratio = source as f64 / target as f64;
    let cutoff = 0.44 / ratio;
    let radius = (24. * ratio).ceil().min(384.) as isize;
    // Exact rational polyphase coefficients: compute trigonometry once per input phase,
    // rather than once per tap for every sample of a long recording.
    let gcd = |mut a: u32, mut b: u32| {
        while b != 0 {
            let r = a % b;
            a = b;
            b = r;
        }
        a
    };
    let divisor = gcd(source, target);
    let phases = target / divisor;
    let taps = (2 * radius + 1) as usize;
    let mut kernels = Vec::with_capacity(phases as usize * taps);
    for phase in 0..phases {
        let fraction = phase as f64 / phases as f64;
        for j in -radius..=radius {
            let d = j as f64 - fraction;
            let a = 2. * std::f64::consts::PI * cutoff * d;
            let sinc = if a.abs() < 1e-10 { 1. } else { a.sin() / a };
            let w = 0.42
                + 0.5 * (std::f64::consts::PI * d / radius as f64).cos()
                + 0.08 * (2. * std::f64::consts::PI * d / radius as f64).cos();
            kernels.push(sinc * w);
        }
    }
    let mut out = Vec::with_capacity((input.len() as f64 / ratio) as usize);
    for i in 0..(input.len() as f64 / ratio) as usize {
        let numerator = i as u64 * source as u64;
        let k = (numerator / target as u64) as isize;
        let phase = ((numerator % target as u64) / divisor as u64) as usize;
        let mut sum = 0.;
        let mut norm = 0.;
        for j in -radius..=radius {
            let idx = k + j;
            if idx < 0 || idx >= input.len() as isize {
                continue;
            }
            let weight = kernels[phase * taps + (j + radius) as usize];
            sum += input[idx as usize] as f64 * weight;
            norm += weight;
        }
        out.push((sum / norm.max(1e-10)) as f32);
    }
    out
}
pub fn spectrogram(
    d: &Decoded,
    c: &Config,
) -> Result<(AudioSummary, Spectrogram, Vec<Event>), String> {
    spectrogram_control(d, c, &|| false)
}
pub fn spectrogram_control(
    d: &Decoded,
    c: &Config,
    cancel: &dyn Fn() -> bool,
) -> Result<(AudioSummary, Spectrogram, Vec<Event>), String> {
    let selected = d
        .channels
        .iter()
        .enumerate()
        .max_by(|a, b| energy(a.1).total_cmp(&energy(b.1)))
        .map(|(i, _)| i)
        .unwrap();
    let rate = ((c.max_hz * 2.6).ceil() as u32)
        .next_power_of_two()
        .max(2048)
        .min(d.rate);
    if c.max_hz >= rate as f64 / 2. {
        return Err("Frequency band exceeds recording Nyquist limit".into());
    }
    let samples = resample(&d.channels[selected], d.rate, rate);
    let n = ((c.window_s * rate as f64) as usize).next_power_of_two();
    let hop = (c.hop_s * rate as f64).round().max(1.) as usize;
    if samples.len() < n {
        return Err(format!(
            "Recording shorter than the {:.2}s FFT window",
            n as f64 / rate as f64
        ));
    }
    let frames = (samples.len() - n) / hop + 1;
    let df = rate as f64 / n as f64;
    let low = (c.min_hz / df).ceil() as usize;
    let high = (c.max_hz / df).floor() as usize;
    let bins = high - low + 1;
    if bins * frames > 8_000_000 {
        return Err("Spectrogram exceeds eight million pixels; shorten audio, increase hop_s, or narrow the band".into());
    }
    let fft: Arc<dyn rustfft::Fft<f32>> = FftPlanner::new().plan_fft_forward(n);
    let mut buffer = vec![Complex::new(0., 0.); n];
    let mut scratch = vec![Complex::new(0., 0.); fft.get_inplace_scratch_len()];
    let window: Vec<f32> = (0..n)
        .map(|i| (0.5 - 0.5 * (2. * std::f64::consts::PI * i as f64 / (n - 1) as f64).cos()) as f32)
        .collect();
    let mut pixels = Vec::with_capacity(frames * bins);
    let mut times = Vec::new();
    for frame in 0..frames {
        if frame % 32 == 0 && cancel() {
            return Err("Analysis cancelled".into());
        }
        let offset = frame * hop;
        for i in 0..n {
            buffer[i] = Complex::new(samples[offset + i] * window[i], 0.);
        }
        fft.process_with_scratch(&mut buffer, &mut scratch);
        pixels
            .extend((low..=high).map(|k| 10. * (buffer[k].norm_sqr() / n as f32 + 1e-18).log10()));
        times.push((offset + n / 2) as f64 / rate as f64);
    }
    // Per-bin temporal whitening removes persistent tones. Positive values mean excess dB.
    let mut column = Vec::with_capacity(frames);
    for k in 0..bins {
        column.clear();
        column.extend(
            (0..frames)
                .step_by((frames / 256).max(1))
                .map(|i| pixels[i * bins + k]),
        );
        column.sort_by(f32::total_cmp);
        let base = column[column.len() / 2];
        for i in 0..frames {
            pixels[i * bins + k] -= base;
        }
    }
    let spec = Spectrogram {
        times_s: times,
        min_hz: low as f64 * df,
        bin_hz: df,
        bins,
        power_db: pixels,
    };
    let events = discover(&spec, c);
    let input = &d.channels[selected];
    let clipped = input.iter().filter(|v| v.abs() >= 0.999).count() as f64 / input.len() as f64;
    Ok((
        AudioSummary {
            sample_rate: d.rate,
            channels: d.channels.len(),
            analyzed_channel: selected,
            duration_s: d.full_duration_s,
            analyzed_duration_s: input.len() as f64 / d.rate as f64,
            clipped_fraction: clipped,
        },
        spec,
        events,
    ))
}
fn energy(v: &[f32]) -> f64 {
    v.iter()
        .step_by(16)
        .map(|x| (*x as f64).powi(2))
        .sum::<f64>()
        / (v.len() / 16).max(1) as f64
}
struct Path {
    points: Vec<RidgePoint>,
    last_frame: usize,
}
fn finish(p: Path, c: &Config, out: &mut Vec<Event>) {
    let n = p.points.len();
    if n < 12 {
        return;
    }
    let first = &p.points[0];
    let last = &p.points[n - 1];
    let duration = last.time_s - first.time_s;
    let m = (n / 8).max(1);
    let avg = |v: &[RidgePoint]| v.iter().map(|p| p.frequency_hz).sum::<f64>() / v.len() as f64;
    let drop = avg(&p.points[..m]) - avg(&p.points[n - m..]);
    let negative = p
        .points
        .windows(2)
        .filter(|v| v[1].frequency_hz <= v[0].frequency_hz + 0.1)
        .count() as f64
        / (n - 1) as f64;
    if duration < c.min_event_s
        || drop < c.min_drop_hz
        || negative < 0.6
        || n as f64 / duration < 1. / c.hop_s * 0.45
    {
        return;
    }
    // CPA proposal from the steepest smoothed drop, not the event's arbitrary boundaries.
    let span = (n / 8).clamp(2, 12);
    let mut center = (first.time_s + last.time_s) / 2.;
    let mut steepest = 0.;
    for j in span..n - span {
        let slope = (p.points[j - span].frequency_hz - p.points[j + span].frequency_hz)
            / (p.points[j + span].time_s - p.points[j - span].time_s);
        if slope > steepest {
            steepest = slope;
            center = p.points[j].time_s;
        }
    }
    out.push(Event {
        id: 0,
        start_s: first.time_s,
        end_s: last.time_s,
        center_s: center,
        drop_hz: drop,
        harmonics: 1,
        points: p.points,
    });
}
pub fn discover(s: &Spectrogram, c: &Config) -> Vec<Event> {
    let mut active: Vec<Path> = Vec::new();
    let mut events = Vec::new();
    for (i, &t) in s.times_s.iter().enumerate() {
        let row = &s.power_db[i * s.bins..(i + 1) * s.bins];
        let mut peaks = Vec::new();
        for k in 1..s.bins - 1 {
            if row[k] as f64 >= c.peak_threshold_db && row[k] > row[k - 1] && row[k] >= row[k + 1] {
                let denom = row[k - 1] - 2. * row[k] + row[k + 1];
                let delta = if denom.abs() > 1e-6 {
                    (0.5 * (row[k - 1] - row[k + 1]) / denom).clamp(-0.5, 0.5)
                } else {
                    0.
                };
                peaks.push((
                    s.min_hz + (k as f64 + delta as f64) * s.bin_hz,
                    row[k] as f64,
                ));
            }
        }
        peaks.sort_by(|a, b| b.1.total_cmp(&a.1));
        peaks.truncate(c.peak_candidates);
        let mut used = vec![false; active.len()];
        for (f, strength) in peaks {
            let best = active
                .iter()
                .enumerate()
                .filter(|(j, p)| !used[*j] && i - p.last_frame <= 4)
                .filter_map(|(j, p)| {
                    let last = p.points.last().unwrap();
                    let dt = t - last.time_s;
                    let slope = if p.points.len() > 2 {
                        let prev = &p.points[p.points.len() - 3];
                        ((last.frequency_hz - prev.frequency_hz) / (last.time_s - prev.time_s))
                            .clamp(-15., 5.)
                    } else {
                        0.
                    };
                    let diff = (f - last.frequency_hz - slope * dt).abs();
                    if diff < (2.5 * s.bin_hz + 5. * dt) {
                        Some((j, diff))
                    } else {
                        None
                    }
                })
                .min_by(|a, b| a.1.total_cmp(&b.1));
            let point = RidgePoint {
                time_s: t,
                frequency_hz: f,
                strength_db: strength,
            };
            if let Some((j, _)) = best {
                active[j].points.push(point);
                active[j].last_frame = i;
                used[j] = true;
            } else {
                active.push(Path {
                    points: vec![point],
                    last_frame: i,
                });
                used.push(true);
            }
        }
        let mut next = Vec::new();
        for p in active {
            if i - p.last_frame > 4 {
                finish(p, c, &mut events);
            } else {
                next.push(p);
            }
        }
        active = next;
    }
    for p in active {
        finish(p, c, &mut events);
    }
    events.sort_by(|a, b| {
        a.points[a.points.len() / 2]
            .frequency_hz
            .total_cmp(&b.points[b.points.len() / 2].frequency_hz)
    });
    let mut grouped: Vec<Event> = Vec::new();
    for e in events {
        let parent = grouped.iter_mut().find(|g| {
            let lo = e.start_s.max(g.start_s);
            let hi = e.end_s.min(g.end_s);
            if hi - lo < 0.55 * (e.end_s - e.start_s).min(g.end_s - g.start_s) {
                return false;
            }
            let mut ratios = Vec::new();
            for p in e.points.iter().step_by((e.points.len() / 12).max(1)) {
                if let Some(q) = g.points.iter().min_by(|a, b| {
                    (a.time_s - p.time_s)
                        .abs()
                        .total_cmp(&(b.time_s - p.time_s).abs())
                }) {
                    if (q.time_s - p.time_s).abs() < 1. {
                        ratios.push(p.frequency_hz / q.frequency_hz);
                    }
                }
            }
            if ratios.len() < 3 {
                return false;
            }
            let r = ratios.iter().sum::<f64>() / ratios.len() as f64;
            let k = r.round();
            (1.0..=5.0).contains(&k) && ratios.iter().all(|v| (v - k).abs() < 0.025)
        });
        if let Some(g) = parent {
            g.harmonics += 1;
        } else {
            grouped.push(e);
        }
    }
    grouped = select_diverse_events(grouped, c.max_events);
    grouped.sort_by(|a, b| a.start_s.total_cmp(&b.start_s));
    for (i, e) in grouped.iter_mut().enumerate() {
        e.id = i;
    }
    grouped
}
fn event_quality(e: &Event) -> f64 {
    (e.end_s - e.start_s).sqrt() * e.drop_hz.sqrt() * (e.harmonics as f64).sqrt()
}
fn select_diverse_events(mut events: Vec<Event>, limit: usize) -> Vec<Event> {
    events.sort_by(|a, b| event_quality(b).total_cmp(&event_quality(a)));
    let mut chosen: Vec<Event> = Vec::new();
    let mut alternatives = Vec::new();
    for e in events {
        if chosen.iter().any(|s| {
            (s.center_s - e.center_s).abs()
                < 0.5 * (s.end_s - s.start_s).max(e.end_s - e.start_s) + 8.
        }) {
            alternatives.push(e);
        } else {
            chosen.push(e);
        }
    }
    chosen.truncate(limit);
    for e in alternatives {
        if chosen.len() >= limit {
            break;
        }
        chosen.push(e);
    }
    chosen
}
/// Reconcile long-window precision with short-window tracking of rapidly changing tones.
/// Retain dissimilar explanations; agreeing resolutions are not independent evidence.
pub fn combine_events(mut a: Vec<Event>, b: Vec<Event>, limit: usize) -> Vec<Event> {
    for e in b {
        let matching = a.iter().position(|g| {
            let overlap = e.end_s.min(g.end_s) - e.start_s.max(g.start_s);
            if overlap < 0.6 * (e.end_s - e.start_s).min(g.end_s - g.start_s) {
                return false;
            }
            let mut differences = Vec::new();
            for p in e.points.iter().step_by((e.points.len() / 16).max(1)) {
                if let Some(q) = g.points.iter().min_by(|a, b| {
                    (a.time_s - p.time_s)
                        .abs()
                        .total_cmp(&(b.time_s - p.time_s).abs())
                }) {
                    if (q.time_s - p.time_s).abs() < 0.5 {
                        differences.push((q.frequency_hz - p.frequency_hz).abs());
                    }
                }
            }
            differences.len() >= 3
                && differences.iter().sum::<f64>() / (differences.len() as f64) < 2.
        });
        if let Some(i) = matching {
            if e.end_s - e.start_s > a[i].end_s - a[i].start_s {
                a[i] = e;
            }
        } else {
            a.push(e);
        }
    }
    a = select_diverse_events(a, limit);
    a.sort_by(|a, b| a.start_s.total_cmp(&b.start_s));
    for (i, e) in a.iter_mut().enumerate() {
        e.id = i;
    }
    a
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn polyphase_decimation_preserves_passband_and_rejects_aliases() {
        let tone = |f: f64| {
            (0..16000)
                .map(|i| (std::f64::consts::TAU * f * i as f64 / 8000.).sin() as f32)
                .collect::<Vec<_>>()
        };
        let low = resample(&tone(200.), 8000, 2048);
        let high = resample(&tone(3000.), 8000, 2048);
        let rms = |v: &[f32]| {
            (v[100..v.len() - 100]
                .iter()
                .map(|x| (*x as f64).powi(2))
                .sum::<f64>()
                / (v.len() - 200) as f64)
                .sqrt()
        };
        assert!((rms(&low) - std::f64::consts::FRAC_1_SQRT_2).abs() < 0.01);
        assert!(rms(&high) < 0.005);
    }
}
pub fn preview(s: &Spectrogram) -> Spectrogram {
    let ts = s.times_s.len().div_ceil(500);
    let fs = s.bins.div_ceil(300);
    let bins = s.bins.div_ceil(fs);
    let mut pixels = Vec::new();
    let mut times = Vec::new();
    for i in (0..s.times_s.len()).step_by(ts) {
        times.push(s.times_s[i]);
        for k in (0..s.bins).step_by(fs) {
            let mut v = -100f32;
            for t in i..(i + ts).min(s.times_s.len()) {
                for f in k..(k + fs).min(s.bins) {
                    v = v.max(s.power_db[t * s.bins + f]);
                }
            }
            pixels.push(v.clamp(-15., 60.));
        }
    }
    Spectrogram {
        times_s: times,
        min_hz: s.min_hz,
        bin_hz: s.bin_hz * fs as f64,
        bins,
        power_db: pixels,
    }
}
