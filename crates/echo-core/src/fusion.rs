//! Shared-site fusion of independently clocked recordings on an identical coarse grid.
//! Scores are profiled heuristic objectives, never probabilities.
use crate::{model::*, physics::distance_m};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
pub fn fuse(a: &[Analysis]) -> Result<Value, String> {
    if a.len() < 2 {
        return Err("Provide at least two separate recordings".into());
    }
    let first = &a[0];
    let mut seen = HashSet::new();
    for r in a {
        if !seen.insert(&r.input_audio_sha256) {
            return Err("Duplicate audio fingerprint; correlated copies cannot be counted as separate recordings".into());
        }
        if r.config.refinement_levels != 0 {
            return Err("Fusion requires the same coarse grid with refinement_levels=0".into());
        }
        if r.coordinate_frame != first.coordinate_frame
            || r.synthetic != first.synthetic
            || r.config.grid_spacing_m != first.config.grid_spacing_m
            || r.config.observer_height_ellipsoid_m != first.config.observer_height_ellipsoid_m
        {
            return Err("Fusion frame, observer height, and lattice must agree".into());
        }
        let comparable = |r: &Analysis| {
            let mut v = serde_json::to_value(&r.config).unwrap();
            v.as_object_mut().unwrap().remove("time_prior");
            v.as_object_mut().unwrap().remove("bounds");
            v
        };
        if comparable(r) != comparable(first) {
            return Err("Use identical inference settings apart from time priors and bounds".into());
        }
        if r.coordinate_frame != "wgs84" && r.data_source != first.data_source {
            return Err("Local frames require the same declared source/embedding".into());
        }
        if r.score_kind != first.score_kind {
            return Err("Use the same scoring method for every recording".into());
        }
    }
    let notes=["Each recording has its own profiled clock. Independence and a shared stationary recorder are supplied assumptions, not established by hashes.","All requested recordings must contribute at each reported cell. Failed recordings are not silently dropped.","Equal-weight sum of heuristic scores; not a calibrated posterior or confidence region."];
    let hashes: Vec<_> = a.iter().map(|r| &r.input_audio_sha256).collect();
    if a.iter().any(|r| r.search.is_none() || r.heatmap.is_empty()) {
        return Ok(
            json!({"schema_version":1,"status":"insufficient_evidence","recording_sha256":hashes,"candidates":[],"heatmap":[],"warnings":notes}),
        );
    }
    let b = first.search.as_ref().unwrap().bounds;
    for r in a {
        let q = r.search.as_ref().unwrap().bounds;
        if [q.west, q.east, q.south, q.north] != [b.west, b.east, b.south, b.north] {
            return Err("Fusion search bounds must agree exactly".into());
        }
    }
    let maps: Vec<HashMap<(u64, u64), &Candidate>> = a
        .iter()
        .map(|r| {
            r.heatmap
                .iter()
                .filter(|c| c.matched_events > 0)
                .map(|c| ((c.lat.to_bits(), c.lon.to_bits()), c))
                .collect()
        })
        .collect();
    let mut joint = Vec::new();
    for (&key, &cell) in &maps[0] {
        let cells: Vec<_> = maps.iter().filter_map(|m| m.get(&key).copied()).collect();
        if cells.len() != a.len() {
            continue;
        }
        let score = cells.iter().map(|c| c.score).sum::<f64>();
        let clocks:Vec<_>=cells.iter().zip(a).map(|(c,r)|json!({"audio_sha256":r.input_audio_sha256,"start_unix_s":c.start_unix_s,"score":c.score,"matched_events":c.matched_events})).collect();
        joint.push(json!({"lat":cell.lat,"lon":cell.lon,"score":score,"recordings":a.len(),"independent_clocks":clocks}));
    }
    joint.sort_by(|x, y| {
        y["score"]
            .as_f64()
            .unwrap()
            .total_cmp(&x["score"].as_f64().unwrap())
            .then_with(|| {
                x["lat"]
                    .as_f64()
                    .unwrap()
                    .total_cmp(&y["lat"].as_f64().unwrap())
            })
            .then_with(|| {
                x["lon"]
                    .as_f64()
                    .unwrap()
                    .total_cmp(&y["lon"].as_f64().unwrap())
            })
    });
    let mut ranked: Vec<Value> = Vec::new();
    for c in &joint {
        if ranked.iter().all(|r| {
            distance_m(
                (r["lat"].as_f64().unwrap(), r["lon"].as_f64().unwrap()),
                (c["lat"].as_f64().unwrap(), c["lon"].as_f64().unwrap()),
            ) > first.config.mode_separation_m
        }) {
            ranked.push(c.clone());
        }
        if ranked.len() == 10 {
            break;
        }
    }
    let incomplete = a
        .iter()
        .any(|r| r.search.as_ref().unwrap().budget_exhausted);
    Ok(
        json!({"schema_version":1,"status":if joint.is_empty(){"no_common_support"}else if incomplete{"search_incomplete"}else{"candidates_uncalibrated"},"coordinate_frame":first.coordinate_frame,"clock_domain":"independent recording clocks","bounds":b,"grid_spacing_m":first.config.grid_spacing_m,"recording_sha256":hashes,"score_kind":"equal_sum_of_profiled_recording_scores_not_probability","candidates":ranked,"heatmap":joint,"warnings":notes}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    fn analysis(hash: &str) -> Analysis {
        let c = Config {
            refinement_levels: 0,
            ..Config::default()
        };
        let b = Bounds {
            west: 0.,
            east: 0.1,
            south: 0.,
            north: 0.1,
        };
        Analysis {
            schema_version: 1,
            engine_version: "test".into(),
            status: "ambiguous".into(),
            synthetic: false,
            input_audio_sha256: hash.into(),
            coordinate_frame: "wgs84".into(),
            clock_domain: "utc".into(),
            config: c,
            audio: AudioSummary {
                sample_rate: 8000,
                channels: 1,
                analyzed_channel: 0,
                duration_s: 60.,
                analyzed_duration_s: 60.,
                clipped_fraction: 0.,
            },
            events: vec![],
            spectrogram: Spectrogram {
                times_s: vec![],
                min_hz: 0.,
                bin_hz: 1.,
                bins: 0,
                power_db: vec![],
            },
            candidates: vec![],
            heatmap: vec![Candidate {
                lat: 0.01,
                lon: 0.02,
                start_unix_s: 100.,
                score: 3.,
                matched_events: 1,
                cell_size_m: 400.,
                matches: vec![],
                ridge_score: None,
                spectral_matches: vec![],
            }],
            search: Some(SearchSummary {
                bounds: b,
                hypotheses_evaluated: 1,
                anchor_fits_evaluated: 0,
                cells_evaluated: 1,
                budget_exhausted: false,
                method: "test".into(),
                time_prior: TimePrior::default(),
            }),
            warnings: vec![],
            data_source: None,
            flights: vec![],
            score_kind: "test-score".into(),
        }
    }
    #[test]
    fn duplicate_recording_is_rejected_and_missing_evidence_is_not_dropped() {
        let a = analysis("a");
        assert!(fuse(&[a.clone(), a.clone()])
            .unwrap_err()
            .contains("Duplicate"));
        let mut b = analysis("b");
        b.heatmap.clear();
        let r = fuse(&[a, b]).unwrap();
        assert_eq!(r["status"], "insufficient_evidence");
        assert_eq!(r["recording_sha256"].as_array().unwrap().len(), 2);
    }
    #[test]
    fn shared_cells_sum_evidence_but_preserve_separate_clocks() {
        let a = analysis("a");
        let mut b = analysis("b");
        b.heatmap[0].start_unix_s = 200.;
        b.heatmap[0].score = 5.;
        let r = fuse(&[a, b]).unwrap();
        assert_eq!(r["candidates"][0]["score"], 8.);
        assert_eq!(
            r["candidates"][0]["independent_clocks"][1]["start_unix_s"],
            200.
        );
    }
}
