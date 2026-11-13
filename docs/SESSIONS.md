# Field recording sessions (format v1)

A **session** is one stationary recording made by `android/` (the Echolocate Recorder app) or any
other recorder that follows this layout. It carries its own ground truth, so the results of
`echo-locate session-analyze` can be scored against a measured receiver position and a measured
UTC clock instead of a reconstructed map reference.

The phone writes only raw observations. Fitting the clock and estimating the site position happen
in `echo-locate`, so better estimators can be applied to old sessions later.

```
20261113T151500Z-husky/
  session.json      manifest, rewritten at each segment boundary and at stop
  audio/seg-00000.wav, seg-00001.wav, …   sample-contiguous audio segments
  clock.jsonl       timestamp anchors (audio frames, GNSS time, wall clock)
  location.jsonl    GNSS position fixes
  gnss_raw.txt      GnssLogger-compatible "Raw" records for post-processed (PPK/PPP) survey
  sensors.jsonl     barometer samples
  labels.jsonl      operator annotations ("aircraft now", "bell", …)
```

Every `*.jsonl` file holds one JSON object per line. Unknown fields must be ignored by readers, and
missing optional fields mean "not observed", never zero. A truncated final line (from a crash) must
be skipped.

## Clock domain

Every record has `mono_ns`, a reading of one monotonic clock that keeps running while the
device sleeps. `session.json` names it in `monotonic_clock`:

| Platform | `monotonic_clock` | Source |
|---|---|---|
| Android | `android_elapsed_realtime_ns` | `SystemClock.elapsedRealtimeNanos()` (CLOCK_BOOTTIME) |
| iOS (planned) | `apple_mach_continuous_ns` | `mach_continuous_time()` scaled to ns |

The analyzer converts audio frames into `mono_ns` values and then into UTC. It uses two fits:

1. **frame → mono:** a linear fit to `audio` anchors. Its slope absorbs the audio crystal's
   error (often tens of ppm). Its residuals reveal dropped buffers.
2. **mono → UTC:** a linear fit to `gnss` anchors when they exist, which gives the true GPS time
   scale. Otherwise it falls back to `wall` anchors (an NTP-disciplined system clock, much worse).

## `session.json`

```json
{
  "format": "echolocate.session",
  "format_version": 1,
  "session_id": "20261113T151500Z-husky",
  "created_unix_ms": 1763046900000,
  "complete": true,
  "monotonic_clock": "android_elapsed_realtime_ns",
  "recorder": {"app": "echolocate-recorder", "app_version": "0.1.0", "platform": "android",
               "os_version": "16", "manufacturer": "Google", "model": "Pixel 8 Pro", "device": "husky"},
  "audio": {"source": "UNPROCESSED", "unprocessed_supported": true, "sample_rate_hz": 48000,
            "channels": 1, "encoding": "f32le", "container": "wav",
            "input_device": "Built-in microphone (type 15)", "effects_disabled": []},
  "segments": [{"file": "audio/seg-00000.wav", "first_frame": 0, "frames": 12582912, "sha256": "…"}],
  "stationary": true,
  "mic_height_above_ground_m": 1.5,
  "site_override": null,
  "notes": ""
}
```

- Audio is 32-bit IEEE-float WAV: a 58-byte header (`fmt ` with `cbSize` 0, then `fact`, then
  `data`). The recorder applies no software gain, so the samples are the converter's full-scale
  values.
- `segments[].first_frame` is the session-global index of the segment's first audio frame, and
  segments are contiguous: `first_frame[i+1] = first_frame[i] + frames[i]`. A segment is closed
  before it would exceed 48 MiB, so each file stays below the 64 MiB decoder limit. Analysis
  windows are independent of segment boundaries.
- `sha256` is computed by the recorder when it closes the segment. It is absent for a segment
  that was still open when the app died. The app repairs such WAV headers the next time it starts.
- `complete` is `false` until recording stops cleanly.
- `site_override` lets you insert a better surveyed position after the fact, for example a PPK
  solution computed from `gnss_raw.txt`:
  `{"lat": …, "lon": …, "height_ellipsoid_m": …, "horizontal_sigma_m": 0.05, "vertical_sigma_m": 0.1, "source": "RTKLIB PPK vs CORS station XXXX"}`.
  When it is present the analyzer uses it instead of the fix average.

## `clock.jsonl`

```json
{"type":"audio","frame":96000,"mono_ns":123456789000,"read":95200}
{"type":"gnss","mono_ns":123456790000,"mono_unc_ns":500,"time_nanos":…,"full_bias_nanos":…,"bias_nanos":0.25,"bias_unc_ns":12.0,"leap_s":18,"hw_discontinuity":0}
{"type":"wall","mono_ns":123456791000,"unix_ns":1763046900123000000,"half_gap_ns":800}
```

- `audio`: `AudioRecord.getTimestamp(…, TIMEBASE_BOOTTIME)`, logged about once per second.
  `frame` is the HAL's capture position, which corresponds to the moment `mono_ns` at the
  converter. `read` is how many frames the app had consumed when it asked. The file frame index is
  assumed to equal `frame`: Android counts both from `startRecording()`. If `frame - read` swings
  by more than the record buffer, audio was lost to an overrun, and the analyzer warns.
- `gnss`: from `GnssClock`. GPS time is `time_nanos - (full_bias_nanos + bias_nanos)` ns since
  1980-01-06T00:00:00Z. UTC is that minus `leap_s`; the analyzer uses 18 s when `leap_s` is
  absent. `mono_ns` is `GnssClock.elapsedRealtimeNanos`, the boot-time instant matching
  `time_nanos`.
- `wall`: `System.currentTimeMillis()` read between two `mono_ns` reads. `half_gap_ns` bounds the
  read jitter.

## `location.jsonl`

```json
{"mono_ns":…,"unix_ms":…,"provider":"gps","lat":40.87,"lon":-73.52,"h_ellipsoid_m":21.3,"h_msl_m":53.9,
 "h_acc_m":3.8,"v_acc_m":6.0,"speed_m_s":0.0,"bearing_deg":null}
```

`h_ellipsoid_m` is the WGS84 ellipsoidal height, which the engine needs (`Location.getAltitude()`
on Android, `ellipsoidalAltitude` on iOS). `h_msl_m` is recorded for reference only and is never
substituted for it.

## `gnss_raw.txt`

These are comma-separated `Raw,…` rows using the column names of Google's GnssLogger app. The
header line `# Raw,utcTimeMillis,TimeNanos,…` names each column. Tools that read GnssLogger logs
can turn the file into RINEX for post-processing against reference stations, which gives
decimetre-level site truth. The recorder asks for full-tracking measurements (no duty cycling).

## `sensors.jsonl`

```json
{"type":"pressure","mono_ns":…,"hpa":1013.2}
```

## `labels.jsonl`

```json
{"mono_ns":…,"unix_ms":…,"kind":"aircraft","azimuth_deg":231.0,"pitch_deg":34.0,"text":""}
```

`kind` is one of `aircraft`, `bell`, `wind`, `speech`, `other`. Azimuth is measured from true north
using the device's rotation vector plus geomagnetic declination, and refers to the direction the
top of the phone points. It is approximate. Aircraft labels are ground truth for event detection,
not for location.

## Analysis

```bash
target/release/echo-locate session-info SESSION_DIR
# prints the UTC span, clock-fit quality, site estimate, search bounds and the adsb.lol fetch command

target/release/echo-locate session-analyze SESSION_DIR \
  --package flights.package.json --config data/coherent.config.json --output-dir runs/SESSION
```

`session-analyze` cuts the continuous recording into overlapping windows (600 s windows starting
every 480 s by default). For each window it sets:

- `time_prior`: the measured UTC of the window's first sample (Gaussian estimate, plus a ±3σ
  window that the search samples directly). The minimum σ is 50 ms for GNSS time and 250 ms for
  wall-clock time. Pass `--time-prior none` to measure how well the engine recovers the clock
  blind.
- `observer_height_ellipsoid_m`: the site's ellipsoidal height.
- `bounds`: the configured bounds if given, otherwise ±`--radius-m` (default 5 km) around the
  site, clipped to the package's bounds.

It writes `window-NNN.json` files (ordinary analysis envelopes) and a `session-summary.json` that
scores each window against the site: the top-candidate distance, the rank of the heatmap cell
nearest the site, the clock error of the top candidate, and how acoustic events line up with
`aircraft` labels. Overlapping windows share audio, so they are not independent recordings. Do not
feed overlapping windows to `fuse-site` as if they were.
