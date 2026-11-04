//! Independent analytic straight-flight renderer. It does not call the inference solver.
use crate::{model::*, physics::point};
use std::io::Cursor;
pub const START: f64 = 1_762_200_000.;
pub const LAT: f64 = 37.7;
pub const LON: f64 = -122.2;
const HEIGHT: f64 = 20.;
struct Pass {
    id: &'static str,
    cpa: f64,
    y: f64,
    h: f64,
    speed: f64,
    f: f64,
}
fn passes() -> Vec<Pass> {
    vec![
        Pass {
            id: "SIM-PROP-A",
            cpa: 25.,
            y: 350.,
            h: 1100.,
            speed: 95.,
            f: 118.,
        },
        Pass {
            id: "SIM-PROP-B",
            cpa: 69.,
            y: -650.,
            h: 1450.,
            speed: 120.,
            f: 173.,
        },
    ]
}
pub fn package() -> DataPackage {
    let mut flights = Vec::new();
    for p in passes() {
        let points = (-60..=180)
            .step_by(2)
            .map(|t| {
                let x = p.speed * (t as f64 - p.cpa);
                point(
                    START + t as f64,
                    LAT + p.y / 111_320.,
                    LON + x / (111_320. * LAT.to_radians().cos()),
                    HEIGHT + p.h,
                )
            })
            .collect();
        flights.push(Flight {
            id: p.id.into(),
            aircraft_type: "synthetic_prop".into(),
            points,
        });
    }
    // Plausible distractors with different geometry. Truth is never supplied to the search.
    for k in 0..2 {
        let points = (-60..=180)
            .step_by(2)
            .map(|t| {
                point(
                    START + t as f64,
                    LAT + (t as f64 - 40.) * 85. / 111_320.,
                    LON + (k as f64 * 1400. - 800.) / (111_320. * LAT.to_radians().cos()),
                    2000. + k as f64 * 300.,
                )
            })
            .collect();
        flights.push(Flight {
            id: format!("SIM-DECOY-{k}"),
            aircraft_type: "synthetic_prop".into(),
            points,
        });
    }
    DataPackage{schema_version:1,name:"Two independent synthetic propeller passes".into(),source:"Echolocate analytic simulator; generated, not field evidence".into(),license:"MIT".into(),coordinate_frame:"wgs84".into(),clock_domain:"utc".into(),coverage_start_unix_s:START-60.,coverage_end_unix_s:START+180.,search_bounds:Bounds{west:LON-0.025,south:LAT-0.02,east:LON+0.025,north:LAT+0.02},synthetic:true,coverage_notes:vec!["Planar constant-speed sources mapped onto WGS84; still air, flat surface; no real-world accuracy claim".into()],flights}
}
pub fn config() -> Config {
    Config {
        max_hz: 650.,
        bounds: Some(package().search_bounds),
        observer_height_ellipsoid_m: HEIGHT,
        time_prior: TimePrior {
            windows: vec![TimeWindow {
                start_unix_s: START - 10.,
                end_unix_s: START + 10.,
                weight: 1.,
            }],
            ..Default::default()
        },
        max_hypotheses: 200_000,
        ..Default::default()
    }
}
pub fn audio() -> Vec<u8> {
    let rate = 8000;
    let duration = 100.;
    let c = 343.;
    let mut samples = Vec::new();
    let mut seed = 1234567u32;
    for i in 0..(duration * rate as f64) as usize {
        let t = i as f64 / rate as f64;
        seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
        let mut s = (seed as f64 / u32::MAX as f64 - 0.5) * 0.008;
        for p in passes() {
            let rx = p.speed * (t - p.cpa);
            let rv = rx * p.speed;
            let r2 = rx * rx + p.y * p.y + p.h * p.h;
            let a = c * c - p.speed * p.speed;
            let delay = (-rv + (rv * rv + a * r2).sqrt()) / a;
            let tau = t - delay;
            let r = (p.speed.powi(2) * (tau - p.cpa).powi(2) + p.y * p.y + p.h * p.h).sqrt();
            let envelope = (-((tau - p.cpa) / 19.).powi(4)).exp() * 0.12 * (1500. / r).min(1.5);
            for harmonic in 1..=3 {
                s += envelope / harmonic as f64
                    * (std::f64::consts::TAU * p.f * harmonic as f64 * tau).sin();
            }
        }
        // A stationary electrical tone exercises temporal whitening.
        s += 0.015 * (std::f64::consts::TAU * 60. * t).sin();
        samples.push((s.clamp(-1., 1.) * 32767.) as i16);
    }
    let mut cursor = Cursor::new(Vec::new());
    {
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: rate,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut writer = hound::WavWriter::new(&mut cursor, spec).unwrap();
        for s in samples {
            writer.write_sample(s).unwrap();
        }
        writer.finalize().unwrap();
    }
    cursor.into_inner()
}
