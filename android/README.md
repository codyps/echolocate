# Echolocate Recorder (Android)

This app records field sessions that `echo-locate session-info` and `session-analyze` can use. Each
session holds unprocessed audio, timestamps that map each audio sample to GPS time, GNSS fixes
with WGS84 ellipsoidal height, raw GNSS measurements for post-processed surveying, barometric
pressure, and operator labels. The format is described in [../docs/SESSIONS.md](../docs/SESSIONS.md).

It is built for the Pixel 8 Pro, which has UNPROCESSED audio capture, `GnssClock` with an
elapsed-realtime mapping, and dual-frequency raw measurements. It needs Android 12 (API 31) or later.

## Build and install

Requirements: JDK 17 or later and the Android SDK (platform 36). From this directory:

```bash
./gradlew :app:testDebugUnitTest :app:assembleDebug
adb install -r app/build/outputs/apk/debug/app-debug.apk
```

Or open `android/` in Android Studio and run it. `local.properties` (which holds `sdk.dir`) is not
committed.

## Recording a session

1. Optional, for the best raw GNSS: turn on **Developer options → Force full GNSS measurements**.
   The app also asks for full tracking itself.
2. Put the phone on a tripod in the open, away from walls. Use a foam or furry windscreen if you
   have one; wind is the main source of low-frequency contamination.
3. Enter the mic height and notes, then press **Start recording**. Wait for the **GPS time**
   counter to start rising and for the fix accuracy to settle.
4. When an aircraft passes, press **✈ Aircraft now** with the top of the phone pointed at it. The
   azimuth and elevation are saved with the label. Use the other buttons for bells, wind gusts,
   speech, and anything else that may confuse the analysis.
5. Press **Stop**. Sessions are about 690 MB per hour (48 kHz mono float).

The screen stays on while the app is in front. Recording continues in a foreground service if you
switch away; the notification shows elapsed time, fix accuracy, and clock state.

## Getting sessions off the phone

- **Export** in the session list writes `Downloads/Echolocate/<session>.zip`.
- Or copy directly: `adb pull /sdcard/Android/data/io.github.codyps.echolocate.recorder/files/sessions/`

If the app is killed while recording, the next launch repairs the WAV headers, rebuilds the segment
list, and marks the session `repaired`. It stays `complete: false`.

## Analyze

```bash
target/release/echo-locate session-info sessions/<id>           # clock fit, site, fetch command
python scripts/fetch_adsb_day.py <date> --bounds=<from session-info> --output flights.package.json
target/release/echo-locate session-analyze sessions/<id> --package flights.package.json \
  --config data/coherent.config.json --output-dir runs/<id>
```

For centimetre-to-decimetre site truth, convert `gnss_raw.txt` (GnssLogger `Raw` columns) to RINEX
and post-process it against a nearby CORS or reference station. Then put the result into
`session.json` as `site_override`.

## Layout

| File | Role |
|---|---|
| `RecorderService.kt` | Foreground service that owns a session; status shared with the UI |
| `AudioCapture.kt` | UNPROCESSED float capture, effects disabled, boot-time audio timestamps, level meter |
| `GnssCapture.kt` | GPS-provider fixes, `GnssClock` anchors, GnssLogger-format raw rows, wall anchors |
| `SensorCapture.kt` | Barometer, and rotation vector for the pointing direction of labels |
| `SessionStore.kt` | Session directory, segment rollover and hashing, manifest, crash repair (JVM-testable) |
| `WavWriter.kt` | Float WAV writer with periodic header patching |
| `MainActivity.kt` | Compose UI: setup, live status, label buttons, session list and export |

## Not yet done

- No iOS recorder. The format reserves `apple_mach_continuous_ns`.
- Not tested on a device yet. Things to check: that UNPROCESSED is reported on the Pixel 8 Pro,
  that `AudioTimestamp` in `TIMEBASE_BOOTTIME` looks sane, and that the `gnss_raw.txt` columns load
  in your RINEX converter.
- No scheduled bell-window recording, and no reference mode for building a catalogue of bells.
