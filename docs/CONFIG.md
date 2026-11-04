# Configuration and inference contract

All configuration is JSON. Missing fields use defaults; unknown fields are rejected. `web/demo.config.json` contains a complete generated example. Geographic packages use Unix seconds in UTC. Source-local research packages explicitly declare an arbitrary embedded coordinate frame and source clock; their numbers must not be interpreted as geographic coordinates or UTC.

| Field | Default | Meaning |
|---|---:|---|
| `coherent_ridges` | false | Use bounded second-order paths across overlapping time/frequency bands; web research preset enables it |
| `robust_shape` | false | Robust shape vs fitted linear-chirp null; fixed contiguous support alternatives; explicit coverage and RMS gate |
| `min_hz`, `max_hz` | 35, 1000 | Analyzed spectral band, constrained by source Nyquist frequency |
| `window_s`, `hop_s` | 2, 0.25 | FFT window (rounded upward to a power of two in samples), frame hop |
| `multiresolution` | true | Combine the primary pass with 0.5-second windows for fast-changing tones |
| `max_audio_s` | 900 | Maximum analyzed prefix; truncation is explicit in results |
| `min_event_s`, `min_drop_hz` | 8, 2 | Minimum ridge duration and descending frequency change |
| `peak_threshold_db` | 9 | Excess power above a per-frequency temporal median |
| `peak_candidates` | 24 | Per-frame spectral proposal budget; larger values are experimental development settings |
| `max_events` | 6 | Event proposal limit, not a count of confirmed aircraft |
| `bounds` | package bounds | Object `{west,south,east,north}`; must lie within package bounds |
| `time_prior` | unconstrained | Time windows and/or Gaussian estimate, described below |
| `grid_spacing_m` | 400 | Approximate coarse geographic spacing |
| `refinement_levels` | 2 | Half-spacing refinement of up to eight separated basins |
| `max_cells` | 2500 | Hard cell budget, including refinement |
| `max_hypotheses` | 100000 | Hard location/start-time hypothesis budget |
| `clock_step_s`, `clock_radius_s` | 2, 20 | Clock proposal spacing and search radius around steepest-drop/CPA clock proposals |
| `sound_speed_m_s` | 343 | Fixed speed of sound relative to the air |
| `wind_east_m_s`, `wind_north_m_s`, `wind_up_m_s` | 0 | Uniform wind in the observer ENU frame |
| `observer_height_ellipsoid_m` | 0 | Supplied constant WGS84 ellipsoidal observer height |
| `max_range_m` | 20000 | Maximum modeled aircraft-to-observer slant range |
| `frequency_sigma_hz` | 1 | Ridge error scale in the uncalibrated matching score |
| `max_track_gap_s` | 30 | Never interpolate across a larger gap |

| `mode_separation_m` | 300 | Minimum separation of reported candidate basins |
| `spectral_refine` | false | Experimental direct-spectrogram reranking; fails current geographic validation |
| `spectral_max_cells` | 512 | Maximum retained ridge cells to rerank, capped at 5000 |

`min_event_s` may be lowered to 0.5 s for short research recordings; this changes discovery sensitivity and is not an accuracy guarantee.

The engine selects the channel with greatest sampled signal energy and never averages channels. Other channels are not independently analyzed. It decimates through a windowed-sinc anti-alias filter. Eight million spectrogram pixels and 32 million decoded channel samples are hard resource limits in addition to file and duration limits.

## Time uncertainty

A bounded interval:

```json
{"time_prior":{"windows":[{"start_unix_s":1762200000,"end_unix_s":1762200600,"weight":1}]}}
```

A Gaussian estimate:

```json
{"time_prior":{"estimate_unix_s":1762200000,"sigma_s":60}}
```

Several disjoint windows are supported. Window weights are relative score densities inside each interval; overlapping weights add. A Gaussian and windows may be combined. This prior shapes candidate ranking; the output is not a calibrated posterior. An exact-time constraint can use a zero-width window. UI times are explicitly UTC, regardless of browser timezone.

Without a time prior, CPA-derived clock proposals span the package coverage. This does not exhaust all possible offsets. With short intervals (at most 120 seconds), the engine also directly samples the prior interval to avoid relying only on CPA proposals.

## Matching

Each location/time hypothesis predicts frequency ratios along each real trajectory using emission time. A constant emitted tone is fitted analytically for each event/flight. Its residual is compared with a stationary-tone null. An assignment beam retains matched and unmatched explanations and prevents a flight segment being counted more than once. All events share one recording start; there are no separate event time shifts.

Default output `score_kind` is `profiled_ridge_shape_contrast_not_probability`. Scores maximize over clock and assignments rather than integrating over uncertainty. The map therefore displays profile match strength. Unscored cells have unknown support. No credible percentages, uniqueness statements, or significance claims are justified by these values.

Experimental spectral mode uses `profiled_spectrogram_contrast_db_not_probability`. It profiles up to two nonoverlapping flight-predicted acoustic windows in retained ridge cells; it is not an exhaustive blind search. `search-events` always uses ridge scoring, and `rerank-spectral` requires the original matching audio fingerprint.

Coherent mode retains at most 16 distinct anchor clocks per geographic cell before joint scoring. `search.anchor_fits_evaluated` counts these fits; a separate hard cap of `16 * max_hypotheses` applies. Reaching either budget yields `search_incomplete`. Cancellation is checked during coherent-path and anchor searches. This pruning is heuristic, not exhaustive clock integration.

`data/coherent.config.json` matches the tested research spectral preset: 35–1000 Hz, 2-second windows, 0.5-second hops, up to 16 proposals, a 2 Hz RMS scale and one FFT resolution. Core defaults preserve legacy behavior. The web UI starts in coherent research mode; loading the synthetic demo explicitly selects legacy mode.

Robust matching compares each full ridge and three fixed contiguous alternatives, charging a search penalty for shorter support. `EventMatch.event_coverage` reports the used fraction; `inlier_fraction` is conditional on that support, and `rms_hz` is the ordinary raw frequency RMS over scored retained samples. It is not a confidence level. Overlapping supports are prevented from counting as independent passages. Unmatched proposals add no evidence in robust mode.

`fuse-site` requires an identical coarse grid (`refinement_levels=0`), scoring settings, frame and observer height. Time priors may differ; every recording has its own clock. The output requires support from every requested recording and rejects duplicate audio fingerprints. Caller-supplied assumptions of one stationary site and independent recordings still need justification. All site scores remain uncalibrated.

## Statuses

| Status | Meaning |
|---|---|
| `no_usable_events` | No descending tonal ridge satisfied current criteria |
| `needs_flight_data` | Events were found; no external trajectories were supplied |
| `no_match` | No candidate survived within the configured search |
| `ambiguous` | The best candidate explains fewer than two event proposals |
| `candidates_uncalibrated` | Multiple events support candidates; confidence remains uncalibrated |
| `search_incomplete` | A search budget stopped evaluation; ranking is partial |

A status is not a proof that the recording lacks aircraft or that a place is correct. An invalid request, unsupported input, oversized grid or cancellation returns an explicit error instead of a fabricated location.
