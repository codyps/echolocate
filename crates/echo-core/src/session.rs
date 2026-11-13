//! Field-session timing and site estimation (format: docs/SESSIONS.md).
//!
//! Pure functions over parsed records; file IO lives in the CLI. Large integer clocks are kept
//! as `i64` origins with `f64` offsets so nanosecond structure survives (an `f64` Unix-ns value
//! would only resolve ~256 ns, and boot-time sums can lose more).
use crate::physics::distance_m;
use serde::{Deserialize, Serialize};

/// Seconds from the Unix epoch to the GPS epoch, 1980-01-06T00:00:00Z.
pub const GPS_UNIX_OFFSET_S: i64 = 315_964_800;
/// GPS-UTC offset since 2017-01-01; used only when a GNSS record omits `leap_s`.
pub const DEFAULT_LEAP_S: i64 = 18;
const NS: f64 = 1e9;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClockRecord {
    /// Audio converter timestamp: HAL frame position `frame` was at `mono_ns`.
    /// `read` is the number of frames the app had consumed when it queried.
    Audio {
        frame: i64,
        mono_ns: i64,
        #[serde(default)]
        read: Option<i64>,
    },
    /// `GnssClock` sample: receiver time `time_nanos` was at `mono_ns`.
    Gnss {
        mono_ns: i64,
        #[serde(default)]
        mono_unc_ns: Option<f64>,
        time_nanos: i64,
        full_bias_nanos: i64,
        #[serde(default)]
        bias_nanos: f64,
        #[serde(default)]
        bias_unc_ns: Option<f64>,
        #[serde(default)]
        leap_s: Option<i64>,
    },
    /// System wall clock read between two monotonic reads.
    Wall {
        mono_ns: i64,
        unix_ns: i64,
        #[serde(default)]
        half_gap_ns: Option<f64>,
    },
}

/// Robust affine fit `y = y0 + intercept + slope * (x - x0)` with `y` in integer + fractional parts.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LinearFit {
    pub x0: i64,
    pub y0: i64,
    pub slope: f64,
    pub intercept: f64,
    pub n: usize,
    pub rejected: usize,
    pub rms_residual: f64,
    pub max_abs_residual: f64,
    pub x_min: i64,
    pub x_max: i64,
}
impl LinearFit {
    /// `y - y0` at `x`.
    pub fn offset(&self, x: i64) -> f64 {
        self.intercept + self.slope * (x - self.x0) as f64
    }
    /// Fit after rejecting residuals beyond `max(5 * 1.4826 * MAD, floor)` (three passes).
    /// With one distinct `x`, `default_slope` is assumed.
    pub fn fit(points: &[(i64, i64, f64)], default_slope: f64, floor: f64) -> Option<Self> {
        if points.is_empty() {
            return None;
        }
        let (x0, y0) = (points[0].0, points[0].1);
        let pts: Vec<(f64, f64)> = points
            .iter()
            .map(|&(x, y, frac)| ((x - x0) as f64, (y - y0) as f64 + frac))
            .collect();
        let mut keep = vec![true; pts.len()];
        let mut fit = (0., default_slope);
        for _ in 0..3 {
            fit = least_squares(&pts, &keep, default_slope)?;
            let res: Vec<f64> = pts.iter().map(|&(x, y)| y - fit.0 - fit.1 * x).collect();
            let mut abs: Vec<f64> = res
                .iter()
                .zip(&keep)
                .filter(|(_, k)| **k)
                .map(|(r, _)| r.abs())
                .collect();
            abs.sort_by(f64::total_cmp);
            let limit = (5. * 1.4826 * abs[abs.len() / 2]).max(floor);
            let next: Vec<bool> = res.iter().map(|r| r.abs() <= limit).collect();
            if next == keep || next.iter().filter(|k| **k).count() < 2.min(pts.len()) {
                break;
            }
            keep = next;
        }
        let kept: Vec<f64> = pts
            .iter()
            .zip(&keep)
            .filter(|(_, k)| **k)
            .map(|(&(x, y), _)| y - fit.0 - fit.1 * x)
            .collect();
        let n = kept.len();
        Some(Self {
            x0,
            y0,
            intercept: fit.0,
            slope: fit.1,
            n,
            rejected: pts.len() - n,
            rms_residual: (kept.iter().map(|r| r * r).sum::<f64>() / n as f64).sqrt(),
            max_abs_residual: kept.iter().fold(0., |m: f64, r| m.max(r.abs())),
            x_min: points.iter().map(|p| p.0).min().unwrap(),
            x_max: points.iter().map(|p| p.0).max().unwrap(),
        })
    }
}
fn least_squares(pts: &[(f64, f64)], keep: &[bool], default_slope: f64) -> Option<(f64, f64)> {
    let sel: Vec<(f64, f64)> = pts
        .iter()
        .zip(keep)
        .filter(|(_, k)| **k)
        .map(|(p, _)| *p)
        .collect();
    if sel.is_empty() {
        return None;
    }
    let n = sel.len() as f64;
    let mx = sel.iter().map(|p| p.0).sum::<f64>() / n;
    let my = sel.iter().map(|p| p.1).sum::<f64>() / n;
    let sxx: f64 = sel.iter().map(|p| (p.0 - mx).powi(2)).sum();
    let sxy: f64 = sel.iter().map(|p| (p.0 - mx) * (p.1 - my)).sum();
    let slope = if sxx > 0. { sxy / sxx } else { default_slope };
    Some((my - slope * mx, slope))
}

/// Audio frame → monotonic → UTC model for one session.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ClockModel {
    pub frame_to_mono: LinearFit,
    pub mono_to_utc: LinearFit,
    /// `gnss` (GPS time scale) or `wall` (system clock fallback).
    pub utc_source: String,
    /// One-sigma uncertainty of `utc_of_frame`, seconds (includes a source-dependent floor).
    pub sigma_s: f64,
    pub nominal_sample_rate_hz: f64,
    pub measured_sample_rate_hz: f64,
    pub sample_rate_error_ppm: f64,
    /// Spread of `frame - read`; much more than the record buffer suggests lost audio.
    pub buffer_lag_spread_frames: Option<i64>,
    pub anchors: AnchorCounts,
    pub warnings: Vec<String>,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct AnchorCounts {
    pub audio: usize,
    pub gnss: usize,
    pub wall: usize,
}

pub const GNSS_SIGMA_FLOOR_S: f64 = 0.05;
pub const WALL_SIGMA_FLOOR_S: f64 = 0.25;

impl ClockModel {
    pub fn fit(records: &[ClockRecord], nominal_rate: f64) -> Result<Self, String> {
        let mut audio = Vec::new();
        let mut gnss = Vec::new();
        let mut wall = Vec::new();
        let mut lags = Vec::new();
        let mut gnss_unc = Vec::new();
        for r in records {
            match *r {
                ClockRecord::Audio {
                    frame,
                    mono_ns,
                    read,
                } => {
                    audio.push((frame, mono_ns, 0.));
                    if let Some(read) = read {
                        lags.push(frame - read);
                    }
                }
                ClockRecord::Gnss {
                    mono_ns,
                    mono_unc_ns,
                    time_nanos,
                    full_bias_nanos,
                    bias_nanos,
                    bias_unc_ns,
                    leap_s,
                } => {
                    let gps_ns = time_nanos
                        .checked_sub(full_bias_nanos)
                        .ok_or("GNSS overflow")?;
                    let unix_ns = gps_ns
                        + (GPS_UNIX_OFFSET_S - leap_s.unwrap_or(DEFAULT_LEAP_S)) * 1_000_000_000;
                    gnss.push((mono_ns, unix_ns, -bias_nanos));
                    gnss_unc.push(
                        (mono_unc_ns.unwrap_or(0.).powi(2) + bias_unc_ns.unwrap_or(0.).powi(2))
                            .sqrt(),
                    );
                }
                ClockRecord::Wall {
                    mono_ns, unix_ns, ..
                } => wall.push((mono_ns, unix_ns, 0.)),
            }
        }
        if !(nominal_rate > 0.) {
            return Err("Session sample rate must be positive".into());
        }
        let ns_per_frame = NS / nominal_rate;
        let frame_to_mono = LinearFit::fit(&audio, ns_per_frame, 50_000.)
            .ok_or("Session has no audio timestamp anchors in clock.jsonl")?;
        let mut warnings = Vec::new();
        let (mono_to_utc, utc_source, floor, extra) = if let Some(f) =
            LinearFit::fit(&gnss, 1., 1_000.)
        {
            let unc = gnss_unc.iter().sum::<f64>() / gnss_unc.len() as f64;
            (f, "gnss", GNSS_SIGMA_FLOOR_S, unc)
        } else if let Some(f) = LinearFit::fit(&wall, 1., 1_000_000.) {
            warnings.push("No GNSS clock anchors: UTC comes from the system wall clock (NTP/NITZ disciplined), not GPS time.".into());
            (f, "wall", WALL_SIGMA_FLOOR_S, 0.)
        } else {
            return Err("Session has neither GNSS nor wall-clock anchors in clock.jsonl".into());
        };
        let sigma_s = ((frame_to_mono.rms_residual.powi(2)
            + mono_to_utc.rms_residual.powi(2)
            + extra.powi(2))
        .sqrt()
            / NS)
            .max(floor);
        let measured = NS / frame_to_mono.slope;
        let ppm = (measured / nominal_rate - 1.) * 1e6;
        if frame_to_mono.max_abs_residual > 2e6 {
            warnings.push(format!("Audio timestamps deviate up to {:.1} ms from a uniform rate; buffers may have been dropped.", frame_to_mono.max_abs_residual / 1e6));
        }
        if ppm.abs() > 300. {
            warnings.push(format!("Measured audio rate differs from nominal by {ppm:.0} ppm; check the recorder's sample-rate metadata."));
        }
        if utc_source == "gnss" && (mono_to_utc.slope - 1.).abs() > 2e-4 {
            warnings.push(format!(
                "Monotonic clock runs {:.0} ppm from GPS time; anchors may be inconsistent.",
                (mono_to_utc.slope - 1.) * 1e6
            ));
        }
        let buffer_lag_spread_frames =
            (!lags.is_empty()).then(|| lags.iter().max().unwrap() - lags.iter().min().unwrap());
        if let Some(spread) = buffer_lag_spread_frames {
            if spread as f64 > 2. * nominal_rate {
                warnings.push(format!("Captured-vs-read frame lag varied by {spread} frames (> 2 s); audio was probably lost to buffer overruns."));
            }
        }
        if frame_to_mono.rejected > 0 || mono_to_utc.rejected > 0 {
            warnings.push(format!(
                "Rejected {} audio and {} {} clock anchors as outliers.",
                frame_to_mono.rejected, mono_to_utc.rejected, utc_source
            ));
        }
        Ok(Self {
            frame_to_mono,
            mono_to_utc,
            utc_source: utc_source.into(),
            sigma_s,
            nominal_sample_rate_hz: nominal_rate,
            measured_sample_rate_hz: measured,
            sample_rate_error_ppm: ppm,
            buffer_lag_spread_frames,
            anchors: AnchorCounts {
                audio: audio.len(),
                gnss: gnss.len(),
                wall: wall.len(),
            },
            warnings,
        })
    }
    /// Monotonic time of `frame` as (integer base, fractional offset) nanoseconds.
    fn mono_of_frame(&self, frame: i64) -> (i64, f64) {
        (self.frame_to_mono.y0, self.frame_to_mono.offset(frame))
    }
    fn utc_of_mono_parts(&self, base: i64, off: f64) -> f64 {
        let m = &self.mono_to_utc;
        let x = (base - m.x0) as f64 + off;
        let y = m.intercept + m.slope * x;
        let secs = m.y0.div_euclid(1_000_000_000);
        let ns = m.y0.rem_euclid(1_000_000_000) as f64;
        secs as f64 + (ns + y) / NS
    }
    /// UTC (Unix seconds) at which session frame `frame` was sampled.
    pub fn utc_of_frame(&self, frame: i64) -> f64 {
        let (b, o) = self.mono_of_frame(frame);
        self.utc_of_mono_parts(b, o)
    }
    pub fn utc_of_mono(&self, mono_ns: i64) -> f64 {
        self.utc_of_mono_parts(mono_ns, 0.)
    }
    /// Fractional session frame sampled at `mono_ns`.
    pub fn frame_of_mono(&self, mono_ns: i64) -> f64 {
        let f = &self.frame_to_mono;
        ((mono_ns - f.y0) as f64 - f.intercept) / f.slope + f.x0 as f64
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Fix {
    pub mono_ns: i64,
    #[serde(default)]
    pub provider: String,
    pub lat: f64,
    pub lon: f64,
    #[serde(default)]
    pub h_ellipsoid_m: Option<f64>,
    #[serde(default)]
    pub h_acc_m: Option<f64>,
    #[serde(default)]
    pub v_acc_m: Option<f64>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SiteOverride {
    pub lat: f64,
    pub lon: f64,
    pub height_ellipsoid_m: f64,
    pub horizontal_sigma_m: f64,
    #[serde(default)]
    pub vertical_sigma_m: Option<f64>,
    pub source: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Site {
    pub lat: f64,
    pub lon: f64,
    pub height_ellipsoid_m: f64,
    pub horizontal_sigma_m: f64,
    pub vertical_sigma_m: Option<f64>,
    pub source: String,
    pub fixes_used: usize,
    pub fixes_rejected: usize,
    pub scatter_rms_m: Option<f64>,
    pub max_deviation_m: Option<f64>,
    pub warnings: Vec<String>,
}

/// Site position: the override if supplied, else an accuracy-weighted mean of GNSS fixes.
/// Sigma is the median reported accuracy (fix errors are time-correlated, so the standard
/// error of the mean would be falsely small).
pub fn estimate_site(fixes: &[Fix], over: Option<&SiteOverride>) -> Result<Site, String> {
    if let Some(o) = over {
        if !(o.lat.abs() <= 90. && o.lon.abs() <= 180. && o.height_ellipsoid_m.is_finite())
            || !(o.horizontal_sigma_m >= 0.)
        {
            return Err("Invalid site_override".into());
        }
        return Ok(Site {
            lat: o.lat,
            lon: o.lon,
            height_ellipsoid_m: o.height_ellipsoid_m,
            horizontal_sigma_m: o.horizontal_sigma_m,
            vertical_sigma_m: o.vertical_sigma_m,
            source: format!("site_override: {}", o.source),
            fixes_used: 0,
            fixes_rejected: 0,
            scatter_rms_m: None,
            max_deviation_m: None,
            warnings: vec![],
        });
    }
    let gps: Vec<&Fix> = fixes.iter().filter(|f| f.provider == "gps").collect();
    let pool: Vec<&Fix> = if gps.is_empty() {
        fixes.iter().collect()
    } else {
        gps
    };
    let valid: Vec<&Fix> = pool
        .into_iter()
        .filter(|f| {
            f.lat.abs() <= 90.
                && f.lon.abs() <= 180.
                && f.h_acc_m.is_some_and(|a| a.is_finite() && a > 0.)
        })
        .collect();
    if valid.is_empty() {
        return Err("No usable location fixes (location.jsonl empty or without accuracy); supply site_override".into());
    }
    let mut accs: Vec<f64> = valid.iter().map(|f| f.h_acc_m.unwrap()).collect();
    accs.sort_by(f64::total_cmp);
    let median = accs[accs.len() / 2];
    let kept: Vec<&Fix> = valid
        .iter()
        .copied()
        .filter(|f| f.h_acc_m.unwrap() <= 2. * median)
        .collect();
    let (mut sw, mut slat, mut slon) = (0., 0., 0.);
    for f in &kept {
        let w = f.h_acc_m.unwrap().powi(-2);
        sw += w;
        slat += w * f.lat;
        slon += w * f.lon;
    }
    let (lat, lon) = (slat / sw, slon / sw);
    let (mut hw, mut hs) = (0., 0.);
    let mut vaccs = Vec::new();
    for f in &kept {
        if let Some(h) = f.h_ellipsoid_m.filter(|h| h.is_finite()) {
            let v = f
                .v_acc_m
                .filter(|v| *v > 0.)
                .unwrap_or(2. * f.h_acc_m.unwrap());
            hw += v.powi(-2);
            hs += v.powi(-2) * h;
            vaccs.push(v);
        }
    }
    if hw == 0. {
        return Err("No location fix reports an ellipsoidal height; supply site_override".into());
    }
    vaccs.sort_by(f64::total_cmp);
    let d: Vec<f64> = kept
        .iter()
        .map(|f| distance_m((lat, lon), (f.lat, f.lon)))
        .collect();
    let scatter = (d.iter().map(|x| x * x).sum::<f64>() / d.len() as f64).sqrt();
    let max_dev = d.iter().fold(0., |m: f64, x| m.max(*x));
    let mut kept_accs: Vec<f64> = kept.iter().map(|f| f.h_acc_m.unwrap()).collect();
    kept_accs.sort_by(f64::total_cmp);
    let sigma = kept_accs[kept_accs.len() / 2].max(scatter);
    let mut warnings = Vec::new();
    if max_dev > 3. * sigma.max(5.) {
        warnings.push(format!("Fixes spread up to {max_dev:.0} m from the mean; the recorder may not have been stationary."));
    }
    Ok(Site {
        lat,
        lon,
        height_ellipsoid_m: hs / hw,
        horizontal_sigma_m: sigma,
        vertical_sigma_m: Some(vaccs[vaccs.len() / 2]),
        source: "weighted mean of recorded GNSS fixes".into(),
        fixes_used: kept.len(),
        fixes_rejected: valid.len() - kept.len(),
        scatter_rms_m: Some(scatter),
        max_deviation_m: Some(max_dev),
        warnings,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    const UNIX0: i64 = 1_762_200_000;
    /// Records from a synthetic device: audio crystal +40 ppm, boot clock -3 ppm vs GPS.
    pub fn synthetic(seconds: i64, rate: i64) -> Vec<ClockRecord> {
        let boot0: i64 = 987_654_321_000;
        let mut v = Vec::new();
        for s in 0..=seconds {
            let frame = s * rate;
            let mono = boot0 + (frame as f64 * 1e9 / (rate as f64 * (1. + 40e-6))) as i64;
            v.push(ClockRecord::Audio {
                frame,
                mono_ns: mono,
                read: Some(frame - 4800 - (s % 3) * 100),
            });
            let unix_ns = UNIX0 * 1_000_000_000 + ((mono - boot0) as f64 * (1. + 3e-6)) as i64;
            let gps_ns = unix_ns - (GPS_UNIX_OFFSET_S - 18) * 1_000_000_000;
            v.push(ClockRecord::Gnss {
                mono_ns: mono,
                mono_unc_ns: Some(300.),
                time_nanos: 5_000_000_000 + s * 1_000_000_000,
                full_bias_nanos: 5_000_000_000 + s * 1_000_000_000 - gps_ns,
                bias_nanos: 0.,
                bias_unc_ns: Some(10.),
                leap_s: Some(18),
            });
        }
        v
    }
    #[test]
    fn gnss_clock_maps_frames_to_utc() {
        let rate = 48_000;
        let mut recs = synthetic(600, rate);
        // A wild audio anchor must be rejected, not bend the fit.
        recs.push(ClockRecord::Audio {
            frame: 300 * rate,
            mono_ns: 0,
            read: None,
        });
        let m = ClockModel::fit(&recs, rate as f64).unwrap();
        assert_eq!(m.utc_source, "gnss");
        assert!((m.utc_of_frame(0) - UNIX0 as f64).abs() < 1e-6);
        let end = m.utc_of_frame(600 * rate);
        let expected = UNIX0 as f64 + 600. / (1. + 40e-6) * (1. + 3e-6);
        assert!((end - expected).abs() < 1e-6, "{end} vs {expected}");
        assert!((m.sample_rate_error_ppm - 40.).abs() < 0.1);
        assert_eq!(m.frame_to_mono.rejected, 1);
        assert_eq!(m.sigma_s, GNSS_SIGMA_FLOOR_S);
        let mono = 987_654_321_000 + 10_000_000_000;
        assert!((m.frame_of_mono(mono) - 10. * 48_000. * (1. + 40e-6)).abs() < 0.01);
    }
    #[test]
    fn wall_clock_fallback_is_labelled_and_wider() {
        let recs = vec![
            ClockRecord::Audio {
                frame: 0,
                mono_ns: 1_000,
                read: None,
            },
            ClockRecord::Wall {
                mono_ns: 1_000,
                unix_ns: UNIX0 * 1_000_000_000,
                half_gap_ns: None,
            },
        ];
        let m = ClockModel::fit(&recs, 48_000.).unwrap();
        assert_eq!(m.utc_source, "wall");
        assert_eq!(m.sigma_s, WALL_SIGMA_FLOOR_S);
        assert!((m.utc_of_frame(48_000) - (UNIX0 as f64 + 1.)).abs() < 1e-9);
        assert!(!m.warnings.is_empty());
    }
    #[test]
    fn missing_anchors_are_errors() {
        assert!(ClockModel::fit(&[], 48_000.).is_err());
    }
    #[test]
    fn site_uses_good_fixes_and_ellipsoidal_height() {
        let mut fixes: Vec<Fix> = (0..60)
            .map(|i| Fix {
                mono_ns: i,
                provider: "gps".into(),
                lat: 40.0 + ((i % 5) as f64 - 2.) * 1e-6,
                lon: -73.0,
                h_ellipsoid_m: Some(-12.),
                h_acc_m: Some(3.),
                v_acc_m: Some(5.),
            })
            .collect();
        fixes.push(Fix {
            mono_ns: 99,
            provider: "gps".into(),
            lat: 40.01,
            lon: -73.0,
            h_ellipsoid_m: Some(500.),
            h_acc_m: Some(50.),
            v_acc_m: Some(80.),
        });
        let s = estimate_site(&fixes, None).unwrap();
        assert_eq!(s.fixes_rejected, 1);
        assert!(distance_m((s.lat, s.lon), (40.0, -73.0)) < 0.5);
        assert!((s.height_ellipsoid_m + 12.).abs() < 1e-9);
        assert!(s.warnings.is_empty());
        let o = SiteOverride {
            lat: 1.,
            lon: 2.,
            height_ellipsoid_m: 3.,
            horizontal_sigma_m: 0.05,
            vertical_sigma_m: None,
            source: "PPK".into(),
        };
        assert_eq!(estimate_site(&fixes, Some(&o)).unwrap().lat, 1.);
    }
}
