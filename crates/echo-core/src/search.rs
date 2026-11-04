use crate::{
    model::*,
    physics::{distance_m, ecef, enu_vector, Trajectory, Vec3},
};

pub fn match_event(
    e: &Event,
    tr: &Trajectory,
    x: Vec3,
    wind: Vec3,
    start: f64,
    c: &Config,
) -> Option<EventMatch> {
    if !c.robust_shape {
        return match_window(e, tr, x, wind, start, c);
    }
    let n = e.points.len();
    let mut best = match_window(e, tr, x, wind, start, c);
    // Fixed, declared supports avoid noisy endpoints without selecting arbitrary
    // individual residuals. Charge for the three additional window hypotheses.
    for (lo, hi) in [(0, n * 3 / 4), (n / 4, n), (n / 4, n * 3 / 4)] {
        if hi - lo < 20 || e.points[hi - 1].time_s - e.points[lo].time_s < c.min_event_s.max(16.) {
            continue;
        }
        let mut window = e.clone();
        window.points = e.points[lo..hi].to_vec();
        if let Some(mut m) = match_window(&window, tr, x, wind, start, c) {
            m.shape_gain -= 2. * 4f64.ln();
            m.event_coverage = (hi - lo) as f64 / n as f64;
            if m.shape_gain > 0. && best.as_ref().is_none_or(|b| m.shape_gain > b.shape_gain) {
                best = Some(m);
            }
        }
    }
    best
}
fn match_window(
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
    let mut tone = pairs.iter().map(|(p, d)| p.frequency_hz * d).sum::<f64>()
        / pairs.iter().map(|(_, d)| d * d).sum::<f64>();
    if !(10.0..=5000.0).contains(&tone) {
        return None;
    }
    if c.robust_shape {
        for _ in 0..6 {
            let mut num = 0.;
            let mut den = 0.;
            for (p, d) in &pairs {
                let error = (p.frequency_hz - tone * d).abs();
                let w = (2. * c.frequency_sigma_hz / error.max(1e-9)).min(1.);
                num += w * p.frequency_hz * d;
                den += w * d * d;
            }
            tone = num / den;
        }
    }
    let mean = pairs.iter().map(|(p, _)| p.frequency_hz).sum::<f64>() / pairs.len() as f64;
    let mut residual = pairs
        .iter()
        .map(|(p, d)| (p.frequency_hz - tone * d).powi(2))
        .sum::<f64>()
        / pairs.len() as f64;
    let raw_residual = residual;
    let constant = pairs
        .iter()
        .map(|(p, _)| (p.frequency_hz - mean).powi(2))
        .sum::<f64>()
        / pairs.len() as f64;
    // Contrast against a stationary tone; a weak approximate shape cannot win by brightness alone.
    let sigma = c.frequency_sigma_hz;
    let shape_gain = ((constant + sigma * sigma) / (residual + sigma * sigma)).ln() * 2.
        - residual / (2. * sigma * sigma);
    let mut shape_gain = shape_gain;
    let mut inlier_fraction = 1.;
    if c.robust_shape {
        let cap = 3. * sigma;
        let loss = |r: f64| {
            if r.abs() <= cap {
                r * r
            } else {
                2. * cap * r.abs() - cap * cap
            }
        };
        // The competing linear-chirp null prevents smooth monotone clutter winning
        // merely because it is less stationary than the old constant-tone null.
        let tmean = pairs.iter().map(|(p, _)| p.time_s).sum::<f64>() / pairs.len() as f64;
        let mut intercept = mean;
        let mut slope = 0.;
        for _ in 0..6 {
            let mut sw = 0.;
            let mut st = 0.;
            let mut sy = 0.;
            let mut stt = 0.;
            let mut sty = 0.;
            for (p, _) in &pairs {
                let t = p.time_s - tmean;
                let r = p.frequency_hz - intercept - slope * t;
                let w = (2. * sigma / r.abs().max(1e-9)).min(1.);
                sw += w;
                st += w * t;
                sy += w * p.frequency_hz;
                stt += w * t * t;
                sty += w * t * p.frequency_hz;
            }
            let det = sw * stt - st * st;
            if det > 1e-12 {
                intercept = (sy * stt - sty * st) / det;
                slope = (sty * sw - st * sy) / det;
            }
        }
        residual = pairs
            .iter()
            .map(|(p, d)| loss(p.frequency_hz - tone * d))
            .sum::<f64>()
            / pairs.len() as f64;
        let linear = pairs
            .iter()
            .map(|(p, _)| loss(p.frequency_hz - intercept - slope * (p.time_s - tmean)))
            .sum::<f64>()
            / pairs.len() as f64;
        inlier_fraction = pairs
            .iter()
            .filter(|(p, d)| (p.frequency_hz - tone * d).abs() <= cap)
            .count() as f64
            / pairs.len() as f64;
        shape_gain = 4. * ((linear + sigma * sigma) / (residual + sigma * sigma)).ln()
            - residual / (2. * sigma * sigma);
        if inlier_fraction < 0.65 {
            return None;
        }
    }
    if shape_gain <= 0.
        || residual.sqrt() > 4. * sigma
        || (c.robust_shape && raw_residual.sqrt() > sigma)
    {
        return None;
    }
    Some(EventMatch {
        event_coverage: 1.,
        inlier_fraction,
        event_id: e.id,
        flight_id: tr.id.clone(),
        tone_hz: tone,
        rms_hz: raw_residual.sqrt(),
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
            // Clutter is the zero-evidence alternative. A negative clutter
            // cost would add an unearned bonus for every weak match.
            next.push((
                *value - if c.robust_shape { 0. } else { 2. },
                matches.clone(),
            ));
            for m in &options {
                if matches.iter().any(|prev| {
                    prev.flight_id == m.flight_id
                        || (c.robust_shape && {
                            let lo = m.predicted[0].time_s.max(prev.predicted[0].time_s);
                            let hi = m
                                .predicted
                                .last()
                                .unwrap()
                                .time_s
                                .min(prev.predicted.last().unwrap().time_s);
                            hi - lo
                                > 0.5
                                    * (m.predicted.last().unwrap().time_s - m.predicted[0].time_s)
                                        .min(
                                            prev.predicted.last().unwrap().time_s
                                                - prev.predicted[0].time_s,
                                        )
                        })
                }) {
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
fn clocks(
    x: Vec3,
    wind: Vec3,
    events: &[Event],
    tracks: &[Trajectory],
    c: &Config,
    anchor_fits: &mut usize,
    cancel: &dyn Fn() -> bool,
) -> Result<Vec<f64>, String> {
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
    let mut anchors = Vec::new();
    // Any proposal can be clutter. Do not make the earliest ridge a mandatory clock anchor.
    'anchors: for anchor in events {
        for tr in tracks {
            if cancel() {
                return Err("Analysis cancelled".into());
            }
            if let Some((emit, r)) = tr.cpa(x, c.max_track_gap_s) {
                if r > c.max_range_m {
                    continue;
                }
                // For uniform straight-line motion, the steepest received Doppler
                // drop occurs at the emission-clock CPA time, not CPA arrival.
                // Adding r/c here biases distant-pass clocks by tens of seconds.
                let center = emit - anchor.center_s;
                let n = (c.clock_radius_s / c.clock_step_s).floor() as i32;
                for k in -n..=n {
                    let t = center + k as f64 * c.clock_step_s;
                    if c.time_prior.log_weight(t).is_some() {
                        if c.robust_shape {
                            if *anchor_fits >= c.max_hypotheses.saturating_mul(16) {
                                break 'anchors;
                            }
                            *anchor_fits += 1;
                            if let Some(m) = match_event(anchor, tr, x, wind, t, c) {
                                anchors.push((t, m.shape_gain));
                            }
                        } else {
                            proposed.push(t);
                        }
                    }
                }
            }
        }
    }
    if c.robust_shape {
        anchors.sort_by(|a, b| b.1.total_cmp(&a.1));
        let mut selected: Vec<f64> = Vec::new();
        for (t, _) in anchors {
            if selected.iter().all(|s| (s - t).abs() >= c.clock_step_s) {
                selected.push(t);
            }
            if selected.len() == 16 {
                break;
            }
        }
        proposed.extend(selected);
    }
    proposed.sort_by(f64::total_cmp);
    proposed.dedup_by(|a, b| (*a - *b).abs() < c.clock_step_s * 0.35);
    Ok(proposed)
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
    let mut anchor_fits = 0;
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
                enu_vector(lat, lon, c.wind_east_m_s, c.wind_north_m_s, c.wind_up_m_s),
                events,
                &tracks,
                c,
                &mut anchor_fits,
                cancel,
            )? {
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
            if c.robust_shape && anchor_fits >= c.max_hypotheses.saturating_mul(16) {
                exhausted = true;
                break 'grid;
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
    Ok((
        ranked,
        heatmap,
        SearchSummary {
            bounds,
            hypotheses_evaluated: evaluated,
            anchor_fits_evaluated: anchor_fits,
            cells_evaluated: cells,
            budget_exhausted: exhausted,
            method: if c.robust_shape {
                "coarse regional lattice; bounded 16-clock anchor pruning; robust nonlinear shape vs linear-chirp null; fixed support alternatives; heuristic refinement".into()
            } else {
                "coarse regional lattice; CPA clock proposals; joint ridge-shape score; multi-basin refinement; heuristic pruning".into()
            },
            time_prior: c.time_prior.clone(),
        },
    ))
}

#[cfg(test)]
mod robust_tests {
    use super::*;
    fn trajectory() -> Trajectory {
        Trajectory {
            id: "independent-pass".into(),
            t: (0..=8).map(|k| k as f64 * 25.).collect(),
            p: (0..=8)
                .map(|k| Vec3(-10000. + k as f64 * 2500., 0., 1000.))
                .collect(),
        }
    }
    fn frequency(t: f64) -> f64 {
        let mach = 100. / 343.;
        let q = (t - 100.) / 10.;
        let u = (q - mach * (q * q + 1. - mach * mach).sqrt()) / (1. - mach * mach);
        120. / (1. + mach * u / (1. + u * u).sqrt())
    }
    fn event() -> Event {
        Event {
            id: 0,
            start_s: 0.,
            end_s: 80.,
            center_s: 40.,
            drop_hz: 60.,
            harmonics: 1,
            points: (0..=160)
                .map(|i| {
                    let t = i as f64 * 0.5;
                    RidgePoint {
                        time_s: t,
                        frequency_hz: frequency(60. + t),
                        strength_db: 15.,
                    }
                })
                .collect(),
        }
    }
    #[test]
    fn steepest_received_drop_seeds_emission_cpa_without_double_delay() {
        let tr = trajectory();
        let e = event();
        let mut c = Config::default();
        c.clock_radius_s = 0.;
        let seeds = clocks(
            Vec3::default(),
            Vec3::default(),
            &[e],
            &[tr],
            &c,
            &mut 0,
            &|| false,
        )
        .unwrap();
        assert!(seeds.iter().any(|s| (s - 60.).abs() < 1e-6));
        let steepest = (950..1050)
            .map(|i| i as f64 / 10.)
            .max_by(|a, b| {
                let slope = |t: f64| frequency(t - 0.01) - frequency(t + 0.01);
                slope(*a).total_cmp(&slope(*b))
            })
            .unwrap();
        assert!((steepest - 100.).abs() < 0.11);
    }
    #[test]
    fn linear_chirp_clutter_is_not_evidence_for_a_flight() {
        let mut e = event();
        for p in &mut e.points {
            p.frequency_hz = 160. - 0.8 * p.time_s;
        }
        let mut c = Config::default();
        c.robust_shape = true;
        c.frequency_sigma_hz = 2.;
        assert!(
            match_event(&e, &trajectory(), Vec3::default(), Vec3::default(), 60., &c).is_none()
        );
    }
    #[test]
    fn contaminated_ends_use_declared_contiguous_support_and_keep_raw_rms() {
        let mut e = event();
        for p in &mut e.points {
            if p.time_s < 18. || p.time_s > 62. {
                p.frequency_hz += 35.;
            }
        }
        let mut c = Config::default();
        c.robust_shape = true;
        c.frequency_sigma_hz = 2.;
        let m = match_event(&e, &trajectory(), Vec3::default(), Vec3::default(), 60., &c).unwrap();
        assert!(m.event_coverage < 0.6);
        assert!(m.rms_hz < 0.001);
        assert!(m.inlier_fraction > 0.99);
        assert!(m.predicted.first().unwrap().time_s >= 18.);
        assert!(m.predicted.last().unwrap().time_s <= 62.);
    }
}
