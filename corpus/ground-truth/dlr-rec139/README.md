# DLR RECORD139 — measured local receiver truth

Source: Timo Schumacher and Henri Siller (DLR), **RECORD139 — Benchmark for Acoustic Source Localization of Flyover Microphone Array Measurements**, DOI [10.5281/zenodo.20494053](https://zenodo.org/records/20494053), CC BY4.0.

This is actual calibrated audio from one Airbus A320-232 flyover in2016, acquired with231 microphones. The source provides microphone positions, aircraft reference positions/velocities indexed to recorded samples, wind vectors, and thermodynamic measurements. Six channels were selected by evenly spaced array indices before running the localization engine. They are correlated observations of one flight, not six independent sessions.

## Acquired files

- Six `mic-NNN.wav` files: original7-second signal per microphone, normalized by one recorded gain into IEEE float32 WAV. No trimming, denoising, tone synthesis or time warping.
- Six `mic-NNN.pressure-pa.npy` files: unchanged original calibrated float32 pressure rows from the HDF5 dataset.
- `metadata.json`: all231 source microphone coordinates, measurement conditions, three trajectory reference states, literal HDF5 dataset attributes and timing caveats.
- `trajectory.reference.csv`: reference states with both zero/one-based sample-time interpretations.
- `receiver-truth.json`: selected microphone coordinates for evaluation, independent of the acoustic estimator.
- `manifest.json`: channel selection, hashes, byte ranges, normalization gains and evidence limits.
- `source_record.json`, `a3202_V2500.dat`, `IntegrationDefinition_Wings_LG.yml`: original publication metadata and accompanying source files.

The full311,789,336-byte HDF5 file was not downloaded. Metadata and selected contiguous channel rows required8,161,792 bytes; each acquired row has a SHA256, as do exported audio/pressure files. Therefore the source's full-fileMD5 is **not** claimed verified. `prepare.py` can restore the same selected byte ranges and verify file identities.

## Coordinate and time interpretation

Microphone coordinates and trajectory states are in source local Cartesian meters. No latitude/longitude, original geographic anchor, or absoluteUTC is supplied. A geographic engine can embed this orthogonal frame around an arbitrary WGS84 anchor for numerical evaluation, provided the report states that the anchor is artificial. Distances between estimated and supplied receivers are then **real local-frame localization errors**, not claims about the original Earth coordinate. Do not use the inferred answer to choose the coordinate transform or search bounds.

The source microphone positions have millimeter decimal precision but no supplied metrological accuracy bound. Do not equate decimal precision with surveyed accuracy.

`ReferenceSample` is described literally as “The Sample Number of the recorded data that the other trajectory parameters refer to.” `ReferencePosition` is described as the position at that sample. The sample-index origin and whether this reference clock denotes emission or reception are not explicitly stated in the retrieved HDF5 attributes. Both sample-index interpretations are preserved; they differ by20.75microseconds. The emission/reception convention can change a predicted arrival by~0.6seconds and must be declared, not adjusted against receiver truth and reported as independent clock validation.

The reference states are:

| Source reference | Published sample | Sample/rate seconds | Position x,y,z m | Velocity x,y,z m/s |
|---|---:|---:|---|---|
| Trajectory060 |89926|1.865994|124.712,2.156,198.636|-88.034,-0.317,-9.484|
| Trajectory090 |152624|3.167000|8.824,-0.027,186.220|-87.969,-0.500,-9.185|
| Trajectory120 |210406|4.365994|-97.146,-1.134,175.365|-87.969,-0.473,-8.677|

Use the exact values in the JSON/CSV, rather than rounded table values. Each reference group also includes its own wind vector and aircraft orientation. Source speed of sound is339.8900146484375m/s, humidity38%, static temperature287.6499938964844K and pressure101325Pa. Still-air/constant-frequency assumptions remain model limitations. Only three source reference states are supplied; do not silently extrapolate them across the entire7seconds as measured ground truth.

## Reproduction

```bash
python prepare.py --verify
python prepare.py --download
```

The helper uses standard-library HTTP range acquisition and file-format conversion only. Signal processing and inference remain in Rust. Source data and derived audio may be redistributed with attribution under CC BY4.0; the repository code remains under its own license.
