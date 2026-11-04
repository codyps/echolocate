# Dated news-audio research corpus

Four actual audio excerpts are saved in `audio/`: three broadcaster news clips
and one U.S. Marine Corps public-affairs clip featuring a turboprop aircraft.
The excerpts total 199 seconds and approximately 6.4 MB. Each is mono 16 kHz
PCM16 WAV. `manifest.json` records the public source, public media URL, event date
and region, publication timestamp, extraction, signal measurements, and SHA-256.

| ID | Source and reported event | Evidence and intended use |
| --- | --- | --- |
| `fox-thunderbirds` | [FOX 5 DC](https://www.fox5dc.com/video/1706587), National Mall flyover, September 12, 2025 | Same-day news page identifies the city and morning flyover. Jet-formation stress case; 60-second excerpt. |
| `fox-frecce` | [FOX 5 DC](https://www.fox5dc.com/video/1504530), Frecce Tricolori over Washington, August 21, 2024 | [Associated report](https://www.fox5dc.com/news/italian-air-forces-frecce-tricolori-fly-over-dc-wednesday-afternoon) describes the route and late-afternoon event. Jet-formation stress case; 45-second excerpt. |
| `fox-af1` | [LiveNOW from FOX](https://www.livenowfox.com/video/1737993), Air Force One over the Commanders stadium, Landover, November 9, 2025 | News caption identifies stadium and occasion. Large-jet broadcast stress case; 45-second excerpt. |
| `dvids-yuma` | [U.S. Marine Corps / DVIDS](https://www.dvidshub.net/video/955827/fat-albert-flyover-with-silent-drill-platoon-mcas-yuma-landscape), Fat Albert C-130J at MCAS Yuma, February 26, 2025 | Source explicitly separates capture date from March 19 publication and locates a photo shoot on the runway. Turboprop detection candidate; 49-second excerpt. |

## What these files validate

These files exercise decoding, real broadcast signal levels, event proposal,
diagnostics, and safe handling of insufficient evidence. They do **not** yet
validate geographic accuracy: none has a verified microphone coordinate or an
established first-sample UTC interval, and historical flight tracks have not been
matched. Every manifest entry therefore has `accuracy_eligible: false` and
`doppler_presence_verified: false`. This prevents public event metadata from
quietly becoming invented recorder ground truth.

The geographic boxes are researcher-chosen search regions based on the reported
event; they are not measured microphone bounds. Date/time windows describe the
reported event day, not every sample in an edited news report. Source footage may
contain narration, music, cuts, several microphones, or substituted audio. Label
ambient segments and establish continuity before using inter-event intervals.

No clip was selected using the localization engine, so the initial corpus does
not depend on its preferred answer. All four are development/smoke inputs; a
future held-out accuracy set must come from independent recording sessions with
measured time and microphone uncertainty.

The Thunderbirds source's first 60 seconds have an almost silent left channel
and a much stronger right channel. The present normalized mono file downmixes
both. A source-channel comparison is a useful regression case for the engine;
automatic channel selection should not assume the first channel is useful.
The source MP4's container creation timestamp is October 11, 2025, later than the
September 12 news event, demonstrating why container metadata is not a capture
timestamp.

## Verification and acquisition

From the project directory:

```sh
python corpus/fetch_audio.py --verify
python corpus/fetch_audio.py --id dvids-yuma
```

The first command is offline and checks files, hashes, sample width, rate, and
channel count. The second downloads only when the selected WAV is absent and
requires `ffmpeg`; omit `--id` to acquire all missing excerpts. The extractor
preserves configured proxy routing, uses public playback URLs, applies the
declared excerpt boundaries, and refuses hash changes. Decoder upgrades or
publisher replacements can change output hashes and require review.

The initial collection used yt-dlp only to discover publicly exposed media URLs
and FFmpeg for decoding. No accounts, credentials, private APIs, or access-control
bypasses were used. Full original videos remain in task scratch space rather
than the project. The public MP4 URLs for two FOX clips point to much larger
assets; extraction stops at 45 seconds instead of retaining those entire videos.

## Media rights and distribution

FOX media remains publisher-owned; these excerpts are local research inputs
collected at the user's request. They are ignored by Git and must not be copied
into the static website or a public source release. The DVIDS page marks its work
public domain subject to the [DVIDS copyright information](https://www.dvidshub.net/about/copyright).
The project keeps all real corpus audio outside the public static assets by
default. Synthetic demo media can be distributed separately.
