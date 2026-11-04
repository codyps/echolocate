pub mod audio;
pub mod demo;
pub mod model;
pub mod physics;
pub mod search;
pub mod spectral;
use model::*;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Deserialize, Serialize, Default)]
#[serde(default)]
pub struct Request {
    pub config: Config,
    pub package: Option<DataPackage>,
}

pub fn analyze(bytes: &[u8], request: &Request) -> Result<Analysis, String> {
    analyze_with_control(bytes, request, &|| false)
}
pub fn analyze_with_control(
    bytes: &[u8],
    request: &Request,
    cancel: &dyn Fn() -> bool,
) -> Result<Analysis, String> {
    let c = &request.config;
    c.validate()?;
    if let Some(p) = &request.package {
        p.validate()?;
    }
    if cancel() {
        return Err("Analysis cancelled".into());
    }
    let decoded = audio::decode(bytes, c.max_audio_s)?;
    let (summary, spec, mut events) = audio::spectrogram_control(&decoded, c, cancel)?;
    if c.multiresolution && c.window_s > 0.75 {
        let mut short = c.clone();
        short.window_s = 0.5;
        short.hop_s = short.hop_s.min(0.25);
        short.multiresolution = false;
        let (_, _, fast) = audio::spectrogram_control(&decoded, &short, cancel)?;
        events = audio::combine_events(events, fast, c.max_events);
    }
    let mut warnings=vec!["Research engine: match scores are uncalibrated; no confidence percentages or accuracy guarantee.".into(),"Stationary recorder, constant emitted tone per event, supplied uniform wind, constant supplied observer height. Terrain and height-varying weather are not modeled.".into()];
    if decoded.truncated {
        warnings.push("Audio was truncated at the configured duration or decoded-sample budget; later events were not searched.".into());
    }
    if summary.clipped_fraction > 0.01 {
        warnings.push("More than 1% of the selected channel is clipped.".into());
    }
    if summary.channels > 1 {
        warnings.push(format!("Analyzed channel {} of {}; selected by signal energy, never mixed. Other channels are not independently searched.",summary.analyzed_channel+1,summary.channels));
    }
    let (candidates, heatmap, search, status) = if events.is_empty() {
        (Vec::new(), Vec::new(), None, "no_usable_events".into())
    } else if let Some(p) = &request.package {
        let (mut ranked, mut heat, summary) = search::locate_control(&events, p, c, cancel)?;
        if c.spectral_refine {
            let r = spectral::rerank(&heat, &spec, p, c, cancel)?;
            ranked = r.0;
            heat = r.1;
            warnings.push("Experimental spectrogram reranking uses predicted aircraft passages, including faint passages missed by ridge discovery. Its scores are uncalibrated and the retained search support is heuristic.".into());
        }
        let status = if summary.budget_exhausted {
            "search_incomplete"
        } else if ranked.is_empty() {
            "no_match"
        } else if ranked[0].matched_events < 2 {
            "ambiguous"
        } else {
            "candidates_uncalibrated"
        };
        warnings.extend(p.coverage_notes.iter().cloned());
        if c.time_prior.windows.is_empty() && c.time_prior.estimate_unix_s.is_none() {
            warnings.push("No time prior: searched CPA-derived start-time proposals within the package coverage, not every possible clock offset.".into());
        }
        warnings.push("Search uses heuristic proposals and refinement; completion does not establish global uniqueness. Unscored cells have unknown support.".into());
        if summary.budget_exhausted {
            warnings.push(
                "Candidate or cell budget exhausted. Results represent only the searched subset."
                    .into(),
            );
        }
        (ranked, heat, Some(summary), status.into())
    } else {
        warnings.push("No flight package supplied. Acoustic discovery completed; absolute location cannot be inferred from these events alone.".into());
        (Vec::new(), Vec::new(), None, "needs_flight_data".into())
    };
    Ok(Analysis {
        schema_version: 1,
        engine_version: env!("CARGO_PKG_VERSION").into(),
        status,
        synthetic: request.package.as_ref().is_some_and(|p| p.synthetic),
        input_audio_sha256: format!("{:x}", Sha256::digest(bytes)),
        coordinate_frame: request
            .package
            .as_ref()
            .map(|p| p.coordinate_frame.clone())
            .unwrap_or_else(|| "wgs84".into()),
        clock_domain: request
            .package
            .as_ref()
            .map(|p| p.clock_domain.clone())
            .unwrap_or_else(|| "utc".into()),
        config: c.clone(),
        audio: summary,
        events,
        spectrogram: audio::preview(&spec),
        candidates,
        heatmap,
        search,
        warnings,
        data_source: request.package.as_ref().map(|p| p.source.clone()),
        flights: request
            .package
            .as_ref()
            .map(|p| p.flights.clone())
            .unwrap_or_default(),
        score_kind: if c.spectral_refine {
            "profiled_spectrogram_contrast_db_not_probability"
        } else {
            "profiled_ridge_shape_contrast_not_probability"
        }
        .into(),
    })
}

/// Identical JSON protocol on native and Wasm. Decode/compute errors are structured, not panics.
pub fn invoke(request: &[u8], audio: &[u8]) -> Vec<u8> {
    let result = serde_json::from_slice::<Request>(request)
        .map_err(|e| format!("Invalid request JSON: {e}"))
        .and_then(|r| analyze(audio, &r));
    let value = match result {
        Ok(a) => serde_json::json!({"ok":true,"analysis":a}),
        Err(error) => serde_json::json!({"ok":false,"error":error}),
    };
    serde_json::to_vec(&value)
        .unwrap_or_else(|_| b"{\"ok\":false,\"error\":\"Serialization failure\"}".to_vec())
}

// Minimal owned-buffer ABI avoids a JavaScript numerical implementation or generated runtime.
// Only the worker calls it, serially. Pointer/length contracts are internal to that trusted bridge.
#[cfg(target_arch = "wasm32")]
mod wasm {
    #[no_mangle]
    pub extern "C" fn echo_alloc(len: u32) -> u32 {
        let data = vec![0u8; len as usize].into_boxed_slice();
        Box::into_raw(data) as *mut u8 as u32
    }
    #[no_mangle]
    pub unsafe extern "C" fn echo_free(ptr: u32, len: u32) {
        if ptr != 0 {
            drop(Box::from_raw(std::ptr::slice_from_raw_parts_mut(
                ptr as *mut u8,
                len as usize,
            )));
        }
    }
    #[no_mangle]
    pub unsafe extern "C" fn echo_run(req: u32, req_len: u32, audio: u32, audio_len: u32) -> u64 {
        let response = super::invoke(
            std::slice::from_raw_parts(req as *const u8, req_len as usize),
            std::slice::from_raw_parts(audio as *const u8, audio_len as usize),
        )
        .into_boxed_slice();
        let len = response.len() as u32;
        let ptr = Box::into_raw(response) as *mut u8 as u32;
        ((ptr as u64) << 32) | len as u64
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn uniform_wind_matches_independent_propagation_geometry() {
        let f = Flight {
            id: "wind".into(),
            aircraft_type: String::new(),
            points: (0..=100)
                .map(|t| physics::point(t as f64, 0., (t as f64 - 50.) * 80. / 111_320., 1000.))
                .collect(),
        };
        let tr = physics::Trajectory::new(&f);
        let observer = physics::ecef(0., 0., 0.);
        let wind = physics::Vec3(0., 12., 3.);
        let tau = 42.;
        let (p, v) = tr.state(tau, 2.).unwrap();
        let r = observer.sub(p);
        let dot = r.dot(wind);
        let a = 343f64.powi(2) - wind.dot(wind);
        let delay = (-dot + (dot * dot + a * r.dot(r)).sqrt()) / a;
        let d = r.sub(wind.mul(delay));
        let k = d.mul(1. / d.norm());
        let ce = 343. + wind.dot(k);
        let expected = ce / (ce - v.dot(k));
        let got = tr
            .doppler_with_wind(observer, tau + delay, 343., 2., wind)
            .unwrap()
            .0;
        assert!((expected - got).abs() < 1e-6);
    }
    use physics::*;
    #[test]
    fn propagation_is_emission_time_not_reception_time() {
        let f = Flight {
            id: "test".into(),
            aircraft_type: "".into(),
            points: (0..=100)
                .map(|t| point(t as f64, 0., (t as f64 - 50.) * 100. / 111_320., 1000.))
                .collect(),
        };
        let tr = Trajectory::new(&f);
        let obs = ecef(0., 0., 0.);
        let tau = 40.;
        let (pos, v) = tr.state(tau, 2.).unwrap();
        let distance = pos.sub(obs).norm();
        let (factor, range) = tr.doppler(obs, tau + distance / 343., 343., 2.).unwrap();
        let expected = 1. / (1. + pos.sub(obs).dot(v) / distance / 343.);
        assert!((factor - expected).abs() < 1e-6);
        assert!((range - distance).abs() < 0.01);
        assert!(tr.doppler(obs, -100., 343., 2.).is_none());
    }
    #[test]
    fn trajectory_does_not_bridge_missing_data() {
        let f = Flight {
            id: "g".into(),
            aircraft_type: "".into(),
            points: vec![point(0., 0., 0., 1000.), point(100., 0., 0.1, 1000.)],
        };
        let tr = Trajectory::new(&f);
        assert!(tr.state(50., 15.).is_none());
        assert!(tr.cpa(ecef(0., 0., 0.), 15.).is_none());
    }
    #[test]
    fn invalid_priors_and_bounds_rejected() {
        let mut c = Config::default();
        c.time_prior.sigma_s = Some(-1.);
        assert!(c.validate().is_err());
        c.time_prior = TimePrior::default();
        c.max_hz = c.min_hz;
        assert!(c.validate().is_err());
    }
    #[test]
    fn disjoint_time_priors_preserve_exclusions() {
        let p = TimePrior {
            windows: vec![
                TimeWindow {
                    start_unix_s: 0.,
                    end_unix_s: 10.,
                    weight: 1.,
                },
                TimeWindow {
                    start_unix_s: 20.,
                    end_unix_s: 30.,
                    weight: 0.5,
                },
            ],
            ..Default::default()
        };
        assert!(p.log_weight(15.).is_none());
        assert!(p.log_weight(25.).unwrap() < p.log_weight(5.).unwrap());
    }
    #[test]
    fn corrupt_audio_is_structured_error() {
        let v: serde_json::Value = serde_json::from_slice(&invoke(b"{}", b"not audio")).unwrap();
        assert_eq!(v["ok"], false);
    }
    #[test]
    fn silence_never_invents_a_location() {
        let mut cursor = std::io::Cursor::new(Vec::new());
        {
            let mut w = hound::WavWriter::new(
                &mut cursor,
                hound::WavSpec {
                    channels: 1,
                    sample_rate: 8000,
                    bits_per_sample: 16,
                    sample_format: hound::SampleFormat::Int,
                },
            )
            .unwrap();
            for _ in 0..80000 {
                w.write_sample(0i16).unwrap();
            }
            w.finalize().unwrap();
        }
        let r = analyze(
            &cursor.into_inner(),
            &Request {
                config: Config::default(),
                package: Some(demo::package()),
            },
        )
        .unwrap();
        assert_eq!(r.status, "no_usable_events");
        assert!(r.candidates.is_empty());
    }
    #[test]
    fn automatic_two_pass_localization() {
        let r = analyze(
            &demo::audio(),
            &Request {
                config: demo::config(),
                package: Some(demo::package()),
            },
        )
        .unwrap();
        eprintln!(
            "events={} status={} best={:?}",
            r.events.len(),
            r.status,
            r.candidates
                .first()
                .map(|v| (v.lat, v.lon, v.start_unix_s, v.score, v.matched_events))
        );
        assert!(r.events.len() >= 2);
        let best = &r.candidates[0];
        assert!(distance_m((best.lat, best.lon), (demo::LAT, demo::LON)) < 800.);
        assert!((best.start_unix_s - demo::START).abs() < 5.);
        assert_eq!(best.matched_events, 2);
    }
    #[test]
    fn unknown_config_keys_are_errors() {
        let r: serde_json::Value =
            serde_json::from_slice(&invoke(br#"{"config":{"max_hypothesez":1}}"#, b"x")).unwrap();
        assert_eq!(r["ok"], false);
        assert!(r["error"].as_str().unwrap().contains("unknown field"));
    }
    #[test]
    fn cancellation_is_explicit() {
        let err = analyze_with_control(&demo::audio(), &Request::default(), &|| true).unwrap_err();
        assert!(err.contains("cancelled"));
    }
    #[test]
    fn out_of_coverage_time_has_no_candidates() {
        let mut c = demo::config();
        c.time_prior = TimePrior {
            windows: vec![TimeWindow {
                start_unix_s: demo::START + 10000.,
                end_unix_s: demo::START + 10001.,
                weight: 1.,
            }],
            ..Default::default()
        };
        let event = Event {
            id: 0,
            start_s: 0.,
            end_s: 10.,
            center_s: 5.,
            drop_hz: 10.,
            harmonics: 1,
            points: (0..=40)
                .map(|i| RidgePoint {
                    time_s: i as f64 / 4.,
                    frequency_hz: 130. - i as f64 / 4.,
                    strength_db: 20.,
                })
                .collect(),
        };
        let (ranked, _, _) = search::locate(&[event], &demo::package(), &c).unwrap();
        assert!(ranked.is_empty());
    }
    #[test]
    fn energetic_right_channel_is_not_lost() {
        let mut cursor = std::io::Cursor::new(Vec::new());
        {
            let mut w = hound::WavWriter::new(
                &mut cursor,
                hound::WavSpec {
                    channels: 2,
                    sample_rate: 8000,
                    bits_per_sample: 16,
                    sample_format: hound::SampleFormat::Int,
                },
            )
            .unwrap();
            for i in 0..80000 {
                w.write_sample(0i16).unwrap();
                w.write_sample(
                    (12000. * (std::f64::consts::TAU * 100. * i as f64 / 8000.).sin()) as i16,
                )
                .unwrap();
            }
            w.finalize().unwrap();
        }
        let result = analyze(&cursor.into_inner(), &Request::default()).unwrap();
        assert_eq!(result.audio.channels, 2);
        assert_eq!(result.audio.analyzed_channel, 1);
        assert!(result.candidates.is_empty());
    }
}
