# Validation status

This implementation has a working automatic synthetic geolocation path, native/Wasm parity checks, a tested web interface, native job-service checks, and measured real-audio detector diagnostics. The revised coherent estimator improves separate-recording ranking while retaining substantial location errors; see [the frozen independent test](ALGORITHM_IMPROVEMENT.md). Earlier measurements below retain their original scope; see [historical estimates](HISTORICAL_VALIDATION.md) and [measured local-coordinate errors](LOCAL_LOCATION_VALIDATION.md). The earlier detector-only receipt below is retained with its original scope.

## Executed checks

| Check | Result |
|---|---|
| Rust workspace tests | 24 passing: propagation-time physics, missing trajectory gaps, invalid inputs/priors, silent audio, right-channel selection, cancellation, time exclusions, readsb geometric heights and synthetic end-to-end inference |
| Node-executed native/Wasm parity tests | 8 passing including verified full-precision hosted-data reconstruction, hash/size checks, and the 3 native/Wasm parity and error checks |
| Real Chromium | Automatic browser Wasm inference, result export, alternate selection, 390 px responsive viewport, worker cancellation and no-package abstention passed, including actual flight downloads at root and a Pages project subpath; no page errors or external network requests |
| Native service | Actual inference job, saved inputs/results, malformed requests, cross-origin write rejection and cooperative cancellation passed |
| Corpus integrity | SHA-256 checks passed for all four news/public-affairs audio files and all selected research source files; research source ZIP checked against Zenodo's published checksum |
| Real-audio processing | All seven recordings decoded and completed unattended discovery; every no-package run withheld geographic candidates |
| Historical archive acquisition | 491 historical traces acquired; source hashes/parsed point counts verified, including 43 original archive members; 7 static packages hash/size/license-checked. Complete global daily archives were not downloaded |

The synthetic renderer uses an analytic emission-time solution in a local planar model. Inference uses ECEF trajectory interpolation and an iterative retarded-time solver, giving a partly independent physics check rather than generating frequencies with the scoring function itself. The scenario includes two propeller-like sources, harmonic tones, a stationary hum, noise, four possible aircraft tracks, and a time interval. No event windows, ridge curves, location truth, or correct aircraft IDs are supplied to the inference request. Its geographic search package and time prior are intentionally bounded. Exact measured error and source fingerprints are in `validation-receipt.json`.

This synthetic example cannot establish field accuracy or global-search completeness. Its geometry and recording conditions are simpler than real audio.

## Actual news and public-affairs audio

| Source excerpt | Duration | Automatic event proposals | Geographic output without flight package |
|---|---:|---:|---|
| FOX 5 DC Thunderbirds / National Mall | 60.0 s | 5 | Withheld |
| FOX Frecce Tricolori / Washington, DC | 45.0 s | 0 | Withheld |
| LiveNOW Air Force One / stadium flyover | 45.0 s | 0 | Withheld |
| DVIDS C-130J / MCAS Yuma | 49.0 s | 0 | Withheld |

These excerpts exercise quiet audio, commentary, possible edits, jet formations and a propeller source. The first clip's source has a near-silent left channel; this motivated a regression check for informative right-channel audio. The presence or absence of event proposals is not a confirmed aircraft-detection label. In particular the C-130J case remains a detector failure/unsupported-case candidate to inspect, not a success hidden by abstention.

The source pages establish a reported event and broad location, but not precise microphone coordinates, continuous source audio, or first-sample UTC. We did not substitute publication or container-creation time for capture time. They are therefore ineligible for coordinate-error scoring. See `../corpus/manifest.json` and the acquisition records.

## Independent measured frequency ridges

Three Pathak recordings were compared with their published UTC frequency measurements. Because waveform-start UTC is unverified, the comparison fits one waveform offset and small-integer harmonic ratios. The table uses the current multi-resolution detector:

| Recording | Published reference span covered at one fitted offset | Reference-equivalent RMS on compatible spans |
|---|---:|---:|
| N373DF_R06 | 53.7% | 1.68 Hz |
| N784MB_R04 | 73.1% | 1.95 Hz |
| N788BM_R03 | 31.5% | 2.03 Hz |

Coverage is a union of compatible intervals, not a sum of overlapping harmonics. Fits require at least eight seconds and twenty paired points with at least 80% of a detected interval overlapping the supplied reference. The exploratory compatibility threshold is 5 Hz at the reference harmonic. Searching offsets and harmonic ratios broadens the chance-match opportunity; these numbers do not measure statistical significance, clock accuracy, aircraft identification or location error.

The short-window detector pass was added after examining a long-window failure on N373DF. These recordings are consequently development data. Compared with the preceding single-resolution version, coverage improved on that recording and fell on the other two under the fixed event-proposal limit. This tradeoff, extra/spurious proposals and incomplete pass recovery are unresolved. No held-out or generalization claim is made.

All point-wise metrics, hashes, fitted offsets and unexplained detections are in `../corpus/pathak/detector-comparison.json`; the human-readable explanation and reproducible comparison script are beside it.

## What remains unverified

- Blind location inference on independently synchronized audio with known microphone coordinates and competing historical traffic.
- Confidence calibration, false-confident-match rates, terrain/wind sensitivity, recorder clock drift and time-varying source pitch.
- Nationwide search recall, broad date ranges, multiple day-package coordination and complete large-archive acquisition.
- Browser behavior beyond tested Chromium, long/mobile memory-pressure cases and hardware-dependent performance.

The current CPU and memory limits, explicit uncertainty states, model warnings and absence of probability claims are part of the output contract. They do not substitute for those experiments.
