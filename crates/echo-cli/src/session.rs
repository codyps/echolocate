//! Field-session IO and evaluation (format: docs/SESSIONS.md).
use echo_core::{
    audio::{self, Decoded},
    model::*,
    physics::distance_m,
    session::*,
    Request,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Path, PathBuf},
};

#[derive(Deserialize, Serialize, Clone)]
pub struct Segment {
    pub file: String,
    pub first_frame: i64,
    pub frames: i64,
    #[serde(default)]
    pub sha256: Option<String>,
}
#[derive(Deserialize, Serialize, Clone)]
pub struct AudioInfo {
    pub sample_rate_hz: u32,
    pub channels: usize,
    #[serde(flatten)]
    pub other: serde_json::Map<String, Value>,
}
#[derive(Deserialize, Serialize, Clone)]
pub struct Manifest {
    pub format: String,
    pub format_version: u32,
    pub session_id: String,
    pub monotonic_clock: String,
    #[serde(default)]
    pub complete: bool,
    #[serde(default)]
    pub recorder: Value,
    pub audio: AudioInfo,
    pub segments: Vec<Segment>,
    #[serde(default)]
    pub site_override: Option<SiteOverride>,
    #[serde(default)]
    pub stationary: Option<bool>,
    #[serde(default)]
    pub notes: String,
}
#[derive(Deserialize, Serialize, Clone)]
pub struct Label {
    pub mono_ns: i64,
    pub kind: String,
    #[serde(default)]
    pub azimuth_deg: Option<f64>,
    #[serde(default)]
    pub pitch_deg: Option<f64>,
    #[serde(default)]
    pub text: String,
}

pub struct Session {
    pub dir: PathBuf,
    pub manifest: Manifest,
    pub clock: ClockModel,
    pub site: Site,
    pub labels: Vec<Label>,
    pub total_frames: i64,
    pub warnings: Vec<String>,
}

/// JSON Lines reader: skips blank lines and a truncated final line (crash), rejects other damage.
fn jsonl<T: serde::de::DeserializeOwned>(path: &Path, required: bool) -> Result<Vec<T>, String> {
    let text = match fs::read_to_string(path) {
        Ok(t) => t,
        Err(_) if !required => return Ok(Vec::new()),
        Err(e) => return Err(format!("{}: {e}", path.display())),
    };
    let lines: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
    let mut out = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        let v: Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(_) if i + 1 == lines.len() && !text.ends_with('\n') => break,
            Err(e) => return Err(format!("{}:{}: {e}", path.display(), i + 1)),
        };
        // Records of other types share files (e.g. future sensors); skip ones we cannot parse
        // only when they carry an unrecognized "type".
        match serde_json::from_value::<T>(v.clone()) {
            Ok(r) => out.push(r),
            Err(e)
                if v.get("type").is_some_and(|t| t.is_string())
                    && e.to_string().contains("unknown variant") => {}
            Err(e) => return Err(format!("{}:{}: {e}", path.display(), i + 1)),
        }
    }
    Ok(out)
}

impl Session {
    pub fn open(dir: &Path) -> Result<Self, String> {
        let m: Manifest = crate::read_json(&dir.join("session.json"))?;
        if m.format != "echolocate.session" || m.format_version != 1 {
            return Err(format!(
                "Unsupported session format {} v{}",
                m.format, m.format_version
            ));
        }
        if m.segments.is_empty() {
            return Err("Session has no audio segments".into());
        }
        let mut next = 0;
        for s in &m.segments {
            if s.first_frame != next || s.frames <= 0 {
                return Err(format!(
                    "Segment {} is not contiguous (first_frame {} expected {next})",
                    s.file, s.first_frame
                ));
            }
            if s.file.contains("..") || Path::new(&s.file).is_absolute() {
                return Err(format!("Segment path {} escapes the session", s.file));
            }
            next += s.frames;
        }
        let mut warnings = Vec::new();
        if !m.complete {
            warnings.push(
                "Session was not stopped cleanly (complete=false); trailing data may be missing."
                    .into(),
            );
        }
        if m.stationary == Some(false) {
            warnings.push(
                "Session is marked non-stationary; the engine assumes a fixed receiver.".into(),
            );
        }
        let records: Vec<ClockRecord> = jsonl(&dir.join("clock.jsonl"), true)?;
        let clock = ClockModel::fit(&records, m.audio.sample_rate_hz as f64)?;
        warnings.extend(clock.warnings.iter().cloned());
        let fixes: Vec<Fix> = jsonl(&dir.join("location.jsonl"), m.site_override.is_none())?;
        let site = estimate_site(&fixes, m.site_override.as_ref())?;
        warnings.extend(site.warnings.iter().cloned());
        let labels = jsonl(&dir.join("labels.jsonl"), false)?;
        Ok(Self {
            dir: dir.to_path_buf(),
            total_frames: next,
            manifest: m,
            clock,
            site,
            labels,
            warnings,
        })
    }
    pub fn rate(&self) -> u32 {
        self.manifest.audio.sample_rate_hz
    }
    pub fn duration_s(&self) -> f64 {
        self.total_frames as f64 / self.rate() as f64
    }
    pub fn utc_span(&self) -> (f64, f64) {
        (
            self.clock.utc_of_frame(0),
            self.clock.utc_of_frame(self.total_frames),
        )
    }
    pub fn bounds(&self, radius_m: f64) -> Bounds {
        let dlat = radius_m / 111_320.;
        let dlon = dlat / self.site.lat.to_radians().cos().max(0.01);
        Bounds {
            west: (self.site.lon - dlon).max(-180.),
            east: (self.site.lon + dlon).min(180.),
            south: (self.site.lat - dlat).max(-85.),
            north: (self.site.lat + dlat).min(85.),
        }
    }
    /// Decode one segment, checking it against the manifest.
    fn decode_segment(&self, i: usize, verify: bool) -> Result<Decoded, String> {
        let s = &self.manifest.segments[i];
        let path = self.dir.join(&s.file);
        let bytes = fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        if verify {
            if let Some(h) = &s.sha256 {
                if &format!("{:x}", Sha256::digest(&bytes)) != h {
                    return Err(format!("{}: SHA-256 differs from session.json", s.file));
                }
            }
        }
        let d = audio::decode(&bytes, f64::MAX)?;
        if d.rate != self.rate() || d.channels.len() != self.manifest.audio.channels {
            return Err(format!(
                "{}: rate/channels differ from session.json",
                s.file
            ));
        }
        if d.channels[0].len() as i64 != s.frames {
            return Err(format!(
                "{}: holds {} frames, session.json declares {}",
                s.file,
                d.channels[0].len(),
                s.frames
            ));
        }
        Ok(d)
    }
}

pub fn info(dir: &Path, radius_m: f64) -> Result<Value, String> {
    let s = Session::open(dir)?;
    let (t0, t1) = s.utc_span();
    let b = s.bounds(radius_m);
    let mut days: Vec<String> = Vec::new();
    for d in [t0, t1] {
        let day = iso_utc(d)[..10].to_string();
        if !days.contains(&day) {
            days.push(day);
        }
    }
    let bstr = format!("{:.5},{:.5},{:.5},{:.5}", b.west, b.south, b.east, b.north);
    let fetch: Vec<String> = days
        .iter()
        .map(|d| {
            format!(
                "python scripts/fetch_adsb_day.py {d} --bounds={bstr} --output {}",
                s.dir.join(format!("flights-{d}.package.json")).display()
            )
        })
        .collect();
    let mut kinds = std::collections::BTreeMap::<String, usize>::new();
    for l in &s.labels {
        *kinds.entry(l.kind.clone()).or_default() += 1;
    }
    Ok(json!({
        "session_id": s.manifest.session_id,
        "recorder": s.manifest.recorder,
        "audio": s.manifest.audio,
        "segments": s.manifest.segments.len(),
        "duration_s": s.duration_s(),
        "utc_start": iso_utc(t0), "utc_end": iso_utc(t1),
        "utc_start_unix_s": t0, "utc_end_unix_s": t1,
        "clock": s.clock,
        "site": s.site,
        "search_bounds": b,
        "labels": kinds,
        "fetch_flights": fetch,
        "warnings": s.warnings,
    }))
}

pub struct AnalyzeOptions {
    pub config: Config,
    pub package: DataPackage,
    pub window_s: f64,
    pub step_s: f64,
    pub radius_m: f64,
    pub use_time_prior: bool,
    pub label_tolerance_s: f64,
    pub verify: bool,
}

#[derive(Serialize)]
struct WindowReport {
    index: usize,
    file: String,
    first_frame: i64,
    frames: i64,
    utc_start: String,
    utc_start_unix_s: f64,
    status: String,
    events: usize,
    top: Option<Value>,
    nearest_cell: Option<Value>,
    aircraft_labels: usize,
    labels_with_event: usize,
    events_with_label: usize,
}

/// Window starts: every `step` frames while a full window fits, then one window ending at the end.
fn plan(total: i64, window: i64, step: i64) -> Vec<(i64, i64)> {
    if total <= window {
        return vec![(0, total)];
    }
    let mut v = Vec::new();
    let mut s = 0;
    while s + window <= total {
        v.push((s, window));
        s += step;
    }
    let last = v.last().unwrap().0 + window;
    if last < total {
        v.push((total - window, window));
    }
    v
}

pub fn analyze(dir: &Path, out: &Path, o: AnalyzeOptions) -> Result<Value, String> {
    let s = Session::open(dir)?;
    o.package.validate()?;
    if !(o.window_s >= 10. && o.window_s <= 3600. && o.step_s > 0. && o.step_s <= o.window_s) {
        return Err("Need 10 <= window-s <= 3600 and 0 < step-s <= window-s".into());
    }
    let rate = s.rate() as f64;
    let windows = plan(
        s.total_frames,
        (o.window_s * rate) as i64,
        (o.step_s * rate) as i64,
    );
    let mut warnings = s.warnings.clone();
    let pb = o.package.search_bounds;
    if !pb.contains(s.site.lat, s.site.lon) {
        warnings.push("The recorded site lies outside the flight package's search bounds; candidates cannot include the truth.".into());
    }
    let (t0, t1) = s.utc_span();
    if t1 < o.package.coverage_start_unix_s || t0 > o.package.coverage_end_unix_s {
        warnings.push("The flight package's time coverage does not overlap this session.".into());
    }
    let bounds = o.config.bounds.unwrap_or_else(|| {
        let b = s.bounds(o.radius_m);
        Bounds {
            west: b.west.max(pb.west),
            east: b.east.min(pb.east),
            south: b.south.max(pb.south),
            north: b.north.min(pb.north),
        }
    });
    if !bounds.valid() {
        return Err("Search bounds around the site do not intersect the package bounds".into());
    }
    fs::create_dir_all(out).map_err(|e| e.to_string())?;
    let mut cache: Option<(usize, Decoded)> = None;
    let mut reports = Vec::new();
    for (wi, &(start, len)) in windows.iter().enumerate() {
        // Assemble the window from every overlapping segment.
        let mut chans = vec![Vec::with_capacity(len as usize); s.manifest.audio.channels];
        for (si, seg) in s.manifest.segments.iter().enumerate() {
            let (a, b) = (seg.first_frame, seg.first_frame + seg.frames);
            if b <= start || a >= start + len {
                continue;
            }
            if cache.as_ref().map(|c| c.0) != Some(si) {
                cache = Some((si, s.decode_segment(si, o.verify)?));
            }
            let d = &cache.as_ref().unwrap().1;
            let lo = (start.max(a) - a) as usize;
            let hi = ((start + len).min(b) - a) as usize;
            for (c, ch) in chans.iter_mut().enumerate() {
                ch.extend_from_slice(&d.channels[c][lo..hi]);
            }
        }
        let mut hasher = Sha256::new();
        for ch in &chans {
            for x in ch {
                hasher.update(x.to_le_bytes());
            }
        }
        let fingerprint = format!("{:x}", hasher.finalize());
        let decoded = Decoded {
            channels: chans,
            rate: s.rate(),
            full_duration_s: len as f64 / rate,
            truncated: false,
        };
        let utc = s.clock.utc_of_frame(start);
        let mut c = o.config.clone();
        c.max_audio_s = c.max_audio_s.max(len as f64 / rate + 1.);
        c.bounds = Some(bounds);
        c.observer_height_ellipsoid_m = s.site.height_ellipsoid_m;
        if o.use_time_prior {
            let sigma = s.clock.sigma_s;
            c.time_prior = TimePrior {
                windows: vec![TimeWindow {
                    start_unix_s: utc - 3. * sigma,
                    end_unix_s: utc + 3. * sigma,
                    weight: 1.,
                }],
                estimate_unix_s: Some(utc),
                sigma_s: Some(sigma),
            };
        }
        let request = Request {
            config: c,
            package: Some(o.package.clone()),
        };
        let mut a = echo_core::analyze_decoded(&decoded, fingerprint, &request, &|| false)?;
        a.warnings.push(format!(
            "Session {} window {wi}: frames {start}..{} ({}), UTC from {} clock, sigma {:.3} s; time prior {}.",
            s.manifest.session_id,
            start + len,
            iso_utc(utc),
            s.clock.utc_source,
            s.clock.sigma_s,
            if o.use_time_prior { "from session clock" } else { "not applied (blind)" }
        ));
        let file = format!("window-{wi:03}.json");
        crate::save(&out.join(&file), &json!({"ok": true, "analysis": a}))?;
        reports.push(score_window(
            &s,
            &a,
            wi,
            file,
            start,
            len,
            utc,
            o.label_tolerance_s,
        ));
        eprintln!(
            "window {wi}/{}: {} ({} events)",
            windows.len(),
            reports.last().unwrap().status,
            reports.last().unwrap().events
        );
    }
    let tops: Vec<f64> = reports
        .iter()
        .filter_map(|r| {
            r.top
                .as_ref()
                .and_then(|t| t["distance_to_site_m"].as_f64())
        })
        .collect();
    let labels: usize = reports.iter().map(|r| r.aircraft_labels).sum();
    let summary = json!({
        "session_id": s.manifest.session_id,
        "site": s.site,
        "clock": s.clock,
        "package": o.package.name,
        "search_bounds": bounds,
        "time_prior": if o.use_time_prior { "session_clock" } else { "none" },
        "window_s": o.window_s, "step_s": o.step_s,
        "windows_overlap": o.step_s < o.window_s,
        "windows": reports,
        "aggregate": {
            "windows": reports.len(),
            "windows_with_candidates": tops.len(),
            "median_top_distance_m": median(&tops),
            "best_top_distance_m": tops.iter().cloned().reduce(f64::min),
            "aircraft_labels_in_windows": labels,
        },
        "notes": [
            "Distances compare uncalibrated top-ranked candidates with the recorded site; they are not confidence statements.",
            "Overlapping windows share audio: do not fuse or count them as independent recordings.",
            "Labels are counted once per window, so with overlap a label can appear in two windows."
        ],
        "warnings": warnings,
    });
    crate::save(&out.join("session-summary.json"), &summary)?;
    Ok(summary)
}

#[allow(clippy::too_many_arguments)]
fn score_window(
    s: &Session,
    a: &Analysis,
    index: usize,
    file: String,
    start: i64,
    len: i64,
    utc: f64,
    tol: f64,
) -> WindowReport {
    let site = (s.site.lat, s.site.lon);
    let top = a.candidates.first().map(|c| {
        json!({
            "lat": c.lat, "lon": c.lon, "score": c.score, "matched_events": c.matched_events,
            "distance_to_site_m": distance_m((c.lat, c.lon), site),
            "clock_error_s": c.start_unix_s - utc,
        })
    });
    let mut heat: Vec<&Candidate> = a.heatmap.iter().collect();
    heat.sort_by(|x, y| y.score.total_cmp(&x.score));
    let nearest_cell = heat
        .iter()
        .enumerate()
        .min_by(|x, y| {
            distance_m((x.1.lat, x.1.lon), site).total_cmp(&distance_m((y.1.lat, y.1.lon), site))
        })
        .map(|(rank, c)| {
            json!({"rank": rank + 1, "of": heat.len(), "distance_to_site_m": distance_m((c.lat, c.lon), site), "cell_size_m": c.cell_size_m})
        });
    // Event centres and labels compared in window-relative seconds.
    let rate = s.rate() as f64;
    let label_t: Vec<f64> = s
        .labels
        .iter()
        .filter(|l| l.kind == "aircraft")
        .map(|l| (s.clock.frame_of_mono(l.mono_ns) - start as f64) / rate)
        .filter(|t| *t >= 0. && *t < len as f64 / rate)
        .collect();
    let near = |a: f64, b: f64| (a - b).abs() <= tol;
    let labels_with_event = label_t
        .iter()
        .filter(|t| a.events.iter().any(|e| near(e.center_s, **t)))
        .count();
    let events_with_label = a
        .events
        .iter()
        .filter(|e| label_t.iter().any(|t| near(e.center_s, *t)))
        .count();
    WindowReport {
        index,
        file,
        first_frame: start,
        frames: len,
        utc_start: iso_utc(utc),
        utc_start_unix_s: utc,
        status: a.status.clone(),
        events: a.events.len(),
        top,
        nearest_cell,
        aircraft_labels: label_t.len(),
        labels_with_event,
        events_with_label,
    }
}

fn median(v: &[f64]) -> Option<f64> {
    let mut v = v.to_vec();
    v.sort_by(f64::total_cmp);
    (!v.is_empty()).then(|| v[v.len() / 2])
}

/// ISO-8601 UTC with milliseconds, without a date-time dependency.
pub fn iso_utc(t: f64) -> String {
    let ms = (t * 1000.).round() as i64;
    let (days, rem) = (ms.div_euclid(86_400_000), ms.rem_euclid(86_400_000));
    // Howard Hinnant's civil_from_days.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}.{:03}Z",
        rem / 3_600_000,
        rem / 60_000 % 60,
        rem / 1000 % 60,
        rem % 1000
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use echo_core::demo;

    #[test]
    fn iso_formatting() {
        assert_eq!(iso_utc(0.), "1970-01-01T00:00:00.000Z");
        assert_eq!(iso_utc(1_762_200_000.25), "2025-11-03T20:00:00.250Z");
        assert_eq!(iso_utc(951_782_400.), "2000-02-29T00:00:00.000Z");
    }
    #[test]
    fn window_plan_covers_tail() {
        assert_eq!(plan(100, 600, 480), vec![(0, 100)]);
        assert_eq!(plan(1000, 600, 480), vec![(0, 600), (400, 600)]);
        assert_eq!(plan(1080, 600, 480), vec![(0, 600), (480, 600)]);
    }

    /// Write the synthetic demo as a two-segment session with a skewed but GNSS-anchored clock.
    fn synth_session(dir: &Path) {
        let wav = demo::audio();
        let d = audio::decode(&wav, f64::MAX).unwrap();
        let samples = &d.channels[0];
        let rate = d.rate as i64;
        fs::create_dir_all(dir.join("audio")).unwrap();
        let split = samples.len() * 3 / 5;
        let mut segs = Vec::new();
        for (i, part) in [&samples[..split], &samples[split..]].iter().enumerate() {
            let mut cur = std::io::Cursor::new(Vec::new());
            let spec = hound::WavSpec {
                channels: 1,
                sample_rate: d.rate,
                bits_per_sample: 32,
                sample_format: hound::SampleFormat::Float,
            };
            let mut w = hound::WavWriter::new(&mut cur, spec).unwrap();
            for x in part.iter() {
                w.write_sample(*x).unwrap();
            }
            w.finalize().unwrap();
            let bytes = cur.into_inner();
            let file = format!("audio/seg-{i:05}.wav");
            fs::write(dir.join(&file), &bytes).unwrap();
            segs.push(json!({"file": file, "first_frame": if i == 0 {0} else {split}, "frames": part.len(), "sha256": format!("{:x}", Sha256::digest(&bytes))}));
        }
        let manifest = json!({
            "format":"echolocate.session","format_version":1,"session_id":"synthetic-demo",
            "monotonic_clock":"android_elapsed_realtime_ns","complete":true,"stationary":true,
            "audio":{"sample_rate_hz":rate,"channels":1,"encoding":"f32le","container":"wav","source":"synthetic"},
            "segments":segs,
        });
        fs::write(dir.join("session.json"), manifest.to_string()).unwrap();
        // Audio crystal +25 ppm; boot clock offset arbitrary; GNSS gives exact UTC.
        let boot0: i64 = 5_555_000_000_000;
        let mut clock = String::new();
        for s in 0..=100i64 {
            let frame = s * rate;
            let mono = boot0 + (frame as f64 * 1e9 / (rate as f64 * (1. + 25e-6))) as i64;
            clock +=
                &json!({"type":"audio","frame":frame,"mono_ns":mono,"read":frame-400}).to_string();
            clock.push('\n');
            let unix_ns = (demo::START as i64) * 1_000_000_000 + (mono - boot0);
            let gps_ns = unix_ns - (GPS_UNIX_OFFSET_S - 18) * 1_000_000_000;
            let tn = 7_000_000_000 + s * 1_000_000_000;
            clock += &json!({"type":"gnss","mono_ns":mono,"time_nanos":tn,"full_bias_nanos":tn - gps_ns,"bias_nanos":0.0,"leap_s":18}).to_string();
            clock.push('\n');
            clock += &json!({"type":"future_thing","mono_ns":mono}).to_string();
            clock.push('\n');
        }
        clock += "{\"type\":\"audio\",\"fra"; // crash-truncated final line
        fs::write(dir.join("clock.jsonl"), clock).unwrap();
        let mut loc = String::new();
        for i in 0..60 {
            loc += &json!({"mono_ns": boot0 + i * 1_000_000_000, "provider":"gps", "lat": demo::LAT + ((i % 3) as f64 - 1.) * 2e-6, "lon": demo::LON, "h_ellipsoid_m": 20.0, "h_acc_m": 3.0, "v_acc_m": 5.0}).to_string();
            loc.push('\n');
        }
        fs::write(dir.join("location.jsonl"), loc).unwrap();
        let label_mono = boot0 + 25_000_000_000; // "aircraft now" at pass A's CPA
        fs::write(
            dir.join("labels.jsonl"),
            json!({"mono_ns": label_mono, "kind": "aircraft"}).to_string() + "\n",
        )
        .unwrap();
    }

    #[test]
    fn synthetic_session_end_to_end() {
        let dir = std::env::temp_dir().join(format!("echo-session-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        synth_session(&dir);
        let info = info(&dir, 2000.).unwrap();
        assert_eq!(info["clock"]["utc_source"], "gnss");
        assert!((info["utc_start_unix_s"].as_f64().unwrap() - demo::START).abs() < 1e-3);
        assert!((info["site"]["height_ellipsoid_m"].as_f64().unwrap() - 20.).abs() < 1e-9);
        let mut config = demo::config();
        config.bounds = None;
        let summary = analyze(
            &dir,
            &dir.join("out"),
            AnalyzeOptions {
                config,
                package: demo::package(),
                window_s: 600.,
                step_s: 480.,
                radius_m: 2000.,
                use_time_prior: true,
                label_tolerance_s: 30.,
                verify: true,
            },
        )
        .unwrap();
        let w = &summary["windows"][0];
        eprintln!("{}", serde_json::to_string_pretty(w).unwrap());
        assert!(w["top"]["distance_to_site_m"].as_f64().unwrap() < 800.);
        assert!(w["top"]["clock_error_s"].as_f64().unwrap().abs() < 1.);
        assert_eq!(w["aircraft_labels"], 1);
        assert_eq!(w["labels_with_event"], 1);
        assert!(dir.join("out/window-000.json").exists());
        // Tampered audio is refused.
        let p = dir.join("audio/seg-00001.wav");
        let mut b = fs::read(&p).unwrap();
        let n = b.len() - 1;
        b[n] ^= 1;
        fs::write(&p, b).unwrap();
        let mut c = demo::config();
        c.bounds = None;
        let err = analyze(
            &dir,
            &dir.join("out2"),
            AnalyzeOptions {
                config: c,
                package: demo::package(),
                window_s: 600.,
                step_s: 480.,
                radius_m: 2000.,
                use_time_prior: true,
                label_tolerance_s: 30.,
                verify: true,
            },
        )
        .err()
        .unwrap();
        assert!(err.contains("SHA-256"), "{err}");
        let _ = fs::remove_dir_all(&dir);
    }
}
