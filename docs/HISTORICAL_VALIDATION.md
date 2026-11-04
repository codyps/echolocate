# Historical flight data and earlier location validation

The measurements below describe the earlier implementation. See [the revised algorithm and frozen separate-recording evaluation](ALGORITHM_IMPROVEMENT.md) for the current coherent research mode.

The current locator fails the available real-data validation. Candidate generation and passing software tests must not be represented as validated location accuracy.

## Actual estimates on the original recording

The raw creator-linked 18-minute excerpt is continuous, and only the recording date is supplied as the clock prior. No hand-drawn frequency curves or individual acoustic windows are supplied. The region is a finite Oyster Bay town prior, not a national search. Both high-recall variants are development experiments, not held-out evaluation.

| Method | Estimated latitude, longitude | Distance to creator map reference | Status |
|---|---|---:|---|
| Ridge default / supplied two aircraft | 40.864033, -73.543904 | 2.04 km | ambiguous |
| High recall ridge / 36 aircraft | 40.841065, -73.502344 | 4.45 km | candidates_uncalibrated |
| Experimental spectral / 36 aircraft | 40.864063, -73.544687 | 2.08 km | candidates_uncalibrated |

The creator’s displayed reference is 40.876728, -73.526434. The prior reconstruction coordinate 40.876466, -73.524951 is separately preserved. These distances are to a published reference, not measured microphone errors. The creator’s on-screen date 9/8/26 and visual camera-site identification at 19 Bayview Ave provide stronger evidence than a publication timestamp. Exact waveform UTC remains unverified.

The competitive package contains 36 real aircraft traces selected geographically and without an acoustic score/type filter, including 34 competitors. This is a sample of 133 replay-discovered regional aircraft, so source coverage remains incomplete. The supplied-two-aircraft run is explicitly assisted association.

## Measured local receiver positions

- constant-velocity-extension: 6/6 estimates; median error 132.0 m, origin-only baseline 3.2 m; 0 estimates beat the origin-only baseline.
- measured-knots: 0/6 estimates; no coordinate-error statistic because no estimate was returned; 0 estimates beat the origin-only baseline.

See [per-channel errors and protocol](LOCAL_LOCATION_VALIDATION.md). The measured-knot protocol uses no extrapolation and returns no usable events. The full 7 s experiment explicitly extends a constant-velocity model from source metadata; it returns inaccurate/ambiguous positions. Both retain the source-clock convention caveat. All six channels share one jet flyover and cannot establish multi-session generalization.

## Existing news reports

| Recording | Available package segments | Result | Accuracy eligible |
|---|---:|---|---|
| fox-thunderbirds | 153 | ambiguous | No: microphone/clock metadata absent |
| fox-frecce | 248 | no_usable_events | No: microphone/clock metadata absent |
| fox-af1 | 1 | no_usable_events | No: microphone/clock metadata absent |
| dvids-yuma | 20 | no_usable_events | No: microphone/clock metadata absent |

AF1 historical positions confirm the reported stadium flyover, but its source trace is MLAT/barometric only. It is retained as historical evidence and excluded from the strict geometric-height package; the available package contains a competitor, not AF1. No barometric height was silently relabeled as geometric. Yuma coverage is sampled, not a complete day. These failures do not prove no aircraft were audible.

## What the data changed

- Actual readsb archives contain approximately 20 s waypoint spacing. The default interpolation gap limit is now 30 s, while larger gaps remain excluded.
- Duplicate/out-of-order source timestamps and invalid geometric heights were found in real data. Every segment is normalized, and conflicting/invalid points are counted and rejected.
- Event selection now preserves temporal diversity, and every event can propose the common clock. The earliest detected ridge is no longer mandatory.
- Uniform wind is supported and tested against independent propagation geometry.
- Experimental direct-spectrogram scoring tests flight-predicted windows rather than requiring a detected ridge for each flyover. It refuses short clipped evidence, counts distinct spectral frames once, and prevents two aircraft hypotheses from double-counting the same time interval. It still fails this geographic benchmark and is disabled by default.
- Cached automatic events can be searched against competing packages without repeating audio decoding, while recording their original audio fingerprint.

## Static hosting deliverables

Seven logical packages contain 5,603,566 bytes of flight data; the largest shard is 261,884 bytes. The complete catalog is lossless after documented source cleaning and includes SHA-256, coverage, coordinate frames and license metadata.

See [STATIC_DATA.md](STATIC_DATA.md) for GitHub Pages workflow, source attribution, ODbL-derived-database distribution and actual hosting-budget checks. No live repository/site was published.

## Next scientific requirement

Do not advertise geographic accuracy yet. The next useful validation set needs several stationary recorders with independently measured geographic position/height, synchronized waveform clocks, continuous propeller-aircraft passes and competing traffic. Algorithm work should focus on faint-pass recall, source-frequency/trajectory model error, null comparisons across the full search and calibrated abstention. The current data exposes those weaknesses rather than concealing them.
