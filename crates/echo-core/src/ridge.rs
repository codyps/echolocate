//! Bounded second-order path search. No aircraft, capture clock, or site enters discovery.
use crate::model::*;

pub fn discover(
    s: &Spectrogram,
    c: &Config,
    cancel: &dyn Fn() -> bool,
) -> Result<Vec<Event>, String> {
    let frames = s.times_s.len();
    if frames < 12 || s.bins < 15 {
        return Ok(vec![]);
    }
    let hop = (s.times_s[1] - s.times_s[0]).max(0.01);
    let width = ((96. / hop).ceil() as usize).min(frames);
    let advance = (width / 2).max(1);
    let down = ((4. * hop / s.bin_hz).ceil() as i32).clamp(1, 8);
    let velocities: Vec<i32> = (-down..=1).collect();
    let nv = velocities.len();
    let states = s.bins * nv;
    let offset = (4. / s.bin_hz).ceil().max(3.) as usize;
    let mut events = Vec::new();
    let mut paths: Vec<(usize, Vec<RidgePoint>)> = Vec::new();
    let mut windows = vec![(0, advance.min(frames))];
    windows.extend(
        (0..frames)
            .step_by(advance)
            .map(|b| (b, (b + width).min(frames))),
    );
    for (tile, (begin, end)) in windows.into_iter().enumerate() {
        if cancel() {
            return Err("Analysis cancelled".into());
        }
        let nt = end - begin;
        if nt < 12 {
            break;
        }
        let mut emission = vec![0f32; nt * s.bins];
        for i in 0..nt {
            for k in offset..s.bins - offset {
                let row = &s.power_db[(begin + i) * s.bins..(begin + i + 1) * s.bins];
                let local = row[k] - 0.5 * (row[k - offset] + row[k + offset]);
                // Clipping limits one transient's influence. Broadband bursts subtract away.
                emission[i * s.bins + k] =
                    local.clamp(-6., 12.) - (c.peak_threshold_db * 0.35).max(2.) as f32;
            }
        }
        let base_emission = emission;
        let mut band_low = s.min_hz;
        while band_low < s.min_hz + (s.bins - 1) as f64 * s.bin_hz {
            let low = (((band_low - s.min_hz) / s.bin_hz).floor() as usize).max(offset);
            let high =
                (((band_low * 4. - s.min_hz) / s.bin_hz).ceil() as usize).min(s.bins - offset);
            band_low *= 2.;
            if high <= low + 8 {
                continue;
            }
            let mut emission = base_emission.clone();
            for _ in 0..4 {
                let mut previous = vec![0f32; states];
                let mut current = previous.clone();
                let mut back = vec![255u8; nt * states];
                let mut best = (0f32, 0usize, 0usize);
                for i in 0..nt {
                    if i % 32 == 0 && cancel() {
                        return Err("Analysis cancelled".into());
                    }
                    current.fill(0.);
                    for k in low..high {
                        for (v, &delta) in velocities.iter().enumerate() {
                            let from = k as i32 - delta;
                            if from < low as i32 || from >= high as i32 {
                                continue;
                            }
                            let mut prior = 0f32;
                            let mut pv = 255u8;
                            for w in v.saturating_sub(2)..=(v + 2).min(nv - 1) {
                                let bend = (velocities[w] - delta) as f32;
                                let value = previous[from as usize * nv + w] - 0.65 * bend * bend;
                                if value > prior {
                                    prior = value;
                                    pv = w as u8;
                                }
                            }
                            let slope = delta as f32;
                            let value =
                                (prior + emission[i * s.bins + k] - 0.025 * slope * slope).max(0.);
                            let state = k * nv + v;
                            current[state] = value;
                            back[i * states + state] = pv;
                            if value > best.0 {
                                best = (value, i, state);
                            }
                        }
                    }
                    std::mem::swap(&mut previous, &mut current);
                }
                if best.0 < 20. {
                    break;
                }
                let mut path = Vec::new();
                let mut state = best.2;
                for i in (0..=best.1).rev() {
                    let k = state / nv;
                    let v = state % nv;
                    path.push((i, k));
                    let pv = back[i * states + state];
                    if pv == 255 {
                        break;
                    }
                    let old = k as i32 - velocities[v];
                    state = old as usize * nv + pv as usize;
                }
                path.reverse();
                let mut points = Vec::new();
                let mut support = 0;
                for &(i, k) in &path {
                    if emission[i * s.bins + k] > 0. {
                        support += 1
                    }
                    let row = &s.power_db[(begin + i) * s.bins..(begin + i + 1) * s.bins];
                    let peak = (k.saturating_sub(1)..=(k + 1).min(s.bins - 2))
                        .max_by(|&a, &b| row[a].total_cmp(&row[b]))
                        .unwrap();
                    let denom = row[peak - 1] - 2. * row[peak] + row[peak + 1];
                    let sub = if denom.abs() > 1e-6 {
                        (0.5 * (row[peak - 1] - row[peak + 1]) / denom).clamp(-0.5, 0.5)
                    } else {
                        0.
                    };
                    points.push(RidgePoint {
                        time_s: s.times_s[begin + i],
                        frequency_hz: s.min_hz + (peak as f64 + sub as f64) * s.bin_hz,
                        strength_db: row[peak] as f64,
                    });
                    for m in k.saturating_sub(3)..=(k + 3).min(s.bins - 1) {
                        emission[i * s.bins + m] = -12.;
                    }
                }
                if support as f64 / path.len() as f64 >= 0.55 {
                    paths.push((tile, points));
                }
            }
        }
    }
    for (_, points) in paths {
        crate::audio::add_path(points, c, &mut events);
    }
    Ok(events)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn silent_spectrum_produces_no_coherent_path_and_cancellation_is_checked() {
        let s = Spectrogram {
            times_s: (0..80).map(|i| i as f64 * 0.5).collect(),
            min_hz: 35.,
            bin_hz: 1.,
            bins: 200,
            power_db: vec![0.; 80 * 200],
        };
        assert!(discover(&s, &Config::default(), &|| false)
            .unwrap()
            .is_empty());
        assert!(discover(&s, &Config::default(), &|| true)
            .unwrap_err()
            .contains("cancelled"));
    }
}
