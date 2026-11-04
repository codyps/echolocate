use crate::{
    model::*,
    physics::{distance_m, ecef, enu_vector, Trajectory, Vec3},
};

fn match_event(
    e: &Event,
    tr: &Trajectory,
    x: Vec3,
    wind: Vec3,
    start: f64,
    c: &Config,
) -> Option<EventMatch> {
    let stride = e.points.len().div_ceil(32);
    let mut pairs = Vec::new();
    for p in e.points.iter().step_by(stride) {
        let (d, r) = tr.doppler_with_wind(
            x,
            start + p.time_s,
            c.sound_speed_m_s,
            c.max_track_gap_s,
            wind,
        )?;
        if r > c.max_range_m {
            return None;
        }
        pairs.push((p, d));
    }
    if pairs.len() < 5 {
        return None;
    }
    let tone = pairs.iter().map(|(p, d)| p.frequency_hz * d).sum::<f64>()
        / pairs.iter().map(|(_, d)| d * d).sum::<f64>();
    if !(10.0..=5000.0).contains(&tone) {
        return None;
    }
    let mean = pairs.iter().map(|(p, _)| p.frequency_hz).sum::<f64>() / pairs.len() as f64;
    let residual = pairs
        .iter()
        .map(|(p, d)| (p.frequency_hz - tone * d).powi(2))
        .sum::<f64>()
        / pairs.len() as f64;
    let constant = pairs
        .iter()
        .map(|(p, _)| (p.frequency_hz - mean).powi(2))
        .sum::<f64>()
        / pairs.len() as f64;
    // Contrast against a stationary tone; a weak approximate shape cannot win by brightness alone.
    let sigma = c.frequency_sigma_hz;
    let shape_gain = ((constant + sigma * sigma) / (residual + sigma * sigma)).ln() * 2.
        - residual / (2. * sigma * sigma);
    if shape_gain <= 0. || residual.sqrt() > 4. * sigma {
        return None;
    }
    Some(EventMatch {
        event_id: e.id,
        flight_id: tr.id.clone(),
        tone_hz: tone,
        rms_hz: residual.sqrt(),
        shape_gain,
        predicted: pairs
            .iter()
            .map(|(p, d)| RidgePoint {
                time_s: p.time_s,
                frequency_hz: tone * d,
                strength_db: 0.,
            })
            .collect(),
    })
}
fn score(
    lat: f64,
    lon: f64,
    start: f64,
    cell: f64,
    events: &[Event],
    tracks: &[Trajectory],
    package: &DataPackage,
    c: &Config,
) -> Option<Candidate> {
    // Archive coverage is in emission time. A waveform prefix may predate the first track,
    // and a valid arrival can occur after the last transmitted position.
    let earliest = events
        .iter()
        .map(|e| e.start_s)
        .fold(f64::INFINITY, f64::min);
    let latest = events
        .iter()
        .map(|e| e.end_s)
        .fold(f64::NEG_INFINITY, f64::max);
    let wind_speed =
        (c.wind_east_m_s.powi(2) + c.wind_north_m_s.powi(2) + c.wind_up_m_s.powi(2)).sqrt();
    let max_delay = c.max_range_m / (c.sound_speed_m_s - wind_speed);
    if start + latest < package.coverage_start_unix_s
        || start + earliest > package.coverage_end_unix_s + max_delay
    {
        return None;
    }
    let prior = c.time_prior.log_weight(start)?;
    let x = ecef(lat, lon, c.observer_height_ellipsoid_m);
    let wind = enu_vector(lat, lon, c.wind_east_m_s, c.wind_north_m_s, c.wind_up_m_s);
    // Small assignment beam includes clutter/unmatched alternatives and avoids counting one pass twice.
    let mut beam: Vec<(f64, Vec<EventMatch>)> = vec![(prior, Vec::new())];
    for e in events {
        let mut options: Vec<_> = tracks
            .iter()
            .filter_map(|tr| match_event(e, tr, x, wind, start, c))
            .collect();
        options.sort_by(|a, b| b.shape_gain.total_cmp(&a.shape_gain));
        options.truncate(3);
        let mut next = Vec::new();
        for (value, matches) in &beam {
            next.push((*value - 2., matches.clone()));
            for m in &options {
                if matches.iter().any(|prev| prev.flight_id == m.flight_id) {
                    continue;
                }
                let mut mm = matches.clone();
                mm.push(m.clone());
                next.push((*value + m.shape_gain, mm));
            }
        }
        next.sort_by(|a, b| b.0.total_cmp(&a.0));
        next.truncate(8);
        beam = next;
    }
    let (value, matches) = beam.into_iter().next()?;
    if matches.is_empty() {
        return None;
    }
    Some(Candidate {
        lat,
        lon,
        start_unix_s: start,
        score: value,
        matched_events: matches.len(),
        cell_size_m: cell,
        matches,
        ridge_score: None,
        spectral_matches: Vec::new(),
    })
}
fn clocks(x: Vec3, events: &[Event], tracks: &[Trajectory], c: &Config) -> Vec<f64> {
    let mut proposed = Vec::new();
    // Independent time estimates seed the search too, so incomplete CPA geometry is not the only route.
    for w in &c.time_prior.windows {
        if w.end_unix_s - w.start_unix_s <= 120. {
            let n = ((w.end_unix_s - w.start_unix_s) / c.clock_step_s).ceil() as usize;
            for i in 0..=n {
                proposed.push((w.start_unix_s + i as f64 * c.clock_step_s).min(w.end_unix_s));
            }
        }
    }
    if let Some(mu) = c.time_prior.estimate_unix_s {
        proposed.push(mu);
    }
    // Any proposal can be clutter. Do not make the earliest ridge a mandatory clock anchor.
    for anchor in events {
        for tr in tracks {
            if let Some((emit, r)) = tr.cpa(x, c.max_track_gap_s) {
                if r > c.max_range_m {
                    continue;
                }
                let center = emit + r / c.sound_speed_m_s - anchor.center_s;
                let n = (c.clock_radius_s / c.clock_step_s).floor() as i32;
                for k in -n..=n {
                    let t = center + k as f64 * c.clock_step_s;
                    if c.time_prior.log_weight(t).is_some() {
                        proposed.push(t);
                    }
                }
            }
        }
    }
    proposed.sort_by(f64::total_cmp);
    proposed.dedup_by(|a, b| (*a - *b).abs() < c.clock_step_s * 0.35);
    proposed
}
pub fn locate(
    events: &[Event],
    p: &DataPackage,
    c: &Config,
) -> Result<(Vec<Candidate>, Vec<Candidate>, SearchSummary), String> {
    locate_control(events, p, c, &|| false)
}
pub fn locate_control(
    events: &[Event],
    p: &DataPackage,
    c: &Config,
    cancel: &dyn Fn() -> bool,
) -> Result<(Vec<Candidate>, Vec<Candidate>, SearchSummary), String> {
    let bounds = c.bounds.unwrap_or(p.search_bounds);
    if bounds.west < p.search_bounds.west
        || bounds.east > p.search_bounds.east
        || bounds.south < p.search_bounds.south
        || bounds.north > p.search_bounds.north
    {
        return Err("Requested bounds extend beyond the package's declared search coverage".into());
    }
    let tracks: Vec<_> = p.flights.iter().map(Trajectory::new).collect();
    let mut heatmap: Vec<Candidate> = Vec::new();
    let mut evaluated = 0;
    let mut cells = 0;
    let mut exhausted = false;
    let dy = c.grid_spacing_m / 111_320.;
    let dx = dy / ((bounds.north + bounds.south) / 2.).to_radians().cos();
    let nx = ((bounds.east - bounds.west) / dx).ceil() as usize;
    let ny = ((bounds.north - bounds.south) / dy).ceil() as usize;
    if nx.saturating_mul(ny) > c.max_cells {
        return Err(format!(
            "Search needs {} coarse cells; increase grid_spacing_m or max_cells (currently {})",
            nx.saturating_mul(ny),
            c.max_cells
        ));
    }
    'grid: for j in 0..ny {
        for i in 0..nx {
            if cancel() {
                return Err("Analysis cancelled".into());
            }
            let lat = bounds.south + (j as f64 + 0.5) * (bounds.north - bounds.south) / ny as f64;
            let lon = bounds.west + (i as f64 + 0.5) * (bounds.east - bounds.west) / nx as f64;
            if evaluated >= c.max_hypotheses || cells >= c.max_cells {
                exhausted = true;
                break 'grid;
            }
            let mut best: Option<Candidate> = None;
            cells += 1;
            for start in clocks(
                ecef(lat, lon, c.observer_height_ellipsoid_m),
                events,
                &tracks,
                c,
            ) {
                if evaluated >= c.max_hypotheses {
                    exhausted = true;
                    break;
                }
                evaluated += 1;
                if let Some(v) = score(lat, lon, start, c.grid_spacing_m, events, &tracks, p, c) {
                    if best.as_ref().is_none_or(|b| v.score > b.score) {
                        best = Some(v);
                    }
                }
            }
            if let Some(v) = best {
                heatmap.push(v);
            }
        }
    }
    for level in 0..c.refinement_levels {
        if exhausted {
            break;
        }
        let mut seeds = heatmap.clone();
        seeds.sort_by(|a, b| b.score.total_cmp(&a.score));
        // Keep separate spatial basins as refinement seeds, including the mirror solution.
        let mut modes: Vec<Candidate> = Vec::new();
        for s in seeds {
            if modes
                .iter()
                .all(|m| distance_m((s.lat, s.lon), (m.lat, m.lon)) > c.grid_spacing_m)
            {
                modes.push(s);
            }
            if modes.len() >= 8 {
                break;
            }
        }
        let spacing = c.grid_spacing_m / 2f64.powi(level as i32 + 1);
        let dt = c.clock_step_s / 2f64.powi(level as i32 + 1);
        'refine: for seed in modes {
            for j in -1..=1 {
                for i in -1..=1 {
                    if cancel() {
                        return Err("Analysis cancelled".into());
                    }
                    let lat = seed.lat + j as f64 * spacing / 111_320.;
                    let lon = seed.lon + i as f64 * spacing / (111_320. * lat.to_radians().cos());
                    if !bounds.contains(lat, lon) {
                        continue;
                    }
                    if cells >= c.max_cells {
                        exhausted = true;
                        break 'refine;
                    }
                    cells += 1;
                    let mut best = None;
                    for k in -2..=2 {
                        if evaluated >= c.max_hypotheses {
                            exhausted = true;
                            break 'refine;
                        }
                        evaluated += 1;
                        if let Some(v) = score(
                            lat,
                            lon,
                            seed.start_unix_s + k as f64 * dt,
                            spacing,
                            events,
                            &tracks,
                            p,
                            c,
                        ) {
                            if best.as_ref().is_none_or(|b: &Candidate| v.score > b.score) {
                                best = Some(v);
                            }
                        }
                    }
                    if let Some(v) = best {
                        heatmap.push(v);
                    }
                }
            }
        }
    }
    heatmap.sort_by(|a, b| b.score.total_cmp(&a.score));
    let mut ranked: Vec<Candidate> = Vec::new();
    for v in &heatmap {
        if ranked
            .iter()
            .all(|r| distance_m((v.lat, v.lon), (r.lat, r.lon)) > c.mode_separation_m)
        {
            ranked.push(v.clone());
        }
        if ranked.len() >= 10 {
            break;
        }
    }
    // Heatmap scores are profiled over clocks/assignments, never labelled posterior probabilities.
    for v in &mut heatmap {
        v.matches.clear();
    }
    Ok((ranked,heatmap,SearchSummary{bounds,hypotheses_evaluated:evaluated,cells_evaluated:cells,budget_exhausted:exhausted,method:"coarse regional lattice; CPA clock proposals; joint ridge-shape score; multi-basin refinement; heuristic pruning".into(),time_prior:c.time_prior.clone()}))
}
