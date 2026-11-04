# Independent aircraft reference recordings

Source: Praveen Pathak, [Aeroplane data full analysis and raw files](https://zenodo.org/records/23424278),
DOI [10.5281/zenodo.23424278](https://doi.org/10.5281/zenodo.23424278),
published October 16, 2026. The source API declares
[CC BY 4.0](https://creativecommons.org/licenses/by/4.0/).
Retain that attribution when distributing these files or derived fixtures.

These files were acquired directly from Zenodo, independently of any software
reimplementation. No dopplerprint files, labels, source code, or datasets were
used. The original 23,096,453-byte constant-speed/constant-height ZIP matched its
published MD5, `d34b32ae57477074d6e8563300a7fc19`.

| Case | Original WAV duration | Supplied ridge UTC interval |
| --- | ---: | --- |
| N373DF_R06 | 49.938 s | May 13, 2026, 20:03:45.776–20:04:25.370 UTC |
| N784MB_R04 | 85.233 s | May 14, 2026, 17:41:37.239–17:42:52.471 UTC |
| N788BM_R03 | 68.027 s | May 17, 2026, 19:29:49.139–19:30:47.165 UTC |

Each folder contains the original mono 48 kHz PCM16 `audio.wav`, ADS-B KML,
author-supplied UTC frequency ridge CSV, and one-page result PDF. Those source
bytes are unchanged. `manifest.json` contains hashes and the source's summary
values. `result_page.txt` is extracted PDF text. `track.normalized.csv` and
`ridge.reference.json` are generated representations for inspection/integration.

## Supported validation

The raw recordings can test the automatic detector against independent published
frequency curves. The UTC ridges and matching flight trajectories can test
propagation and association when supplied events are allowed. Treat those as two
separate measurements: supplying the author's ridge is not automatic discovery.

These three cases were chosen from the source category before running our engine.
They are development fixtures, not a held-out generalization set. The catalog
contains only the source-identified flight for each case, so success does not
establish association accuracy among all aircraft in the surrounding airspace.

## Ground truth limitations found during source inspection

The KML files have aircraft tracks only. The per-case PDFs and the combined
41-page result atlas show a phone at the origin of relative along/across-track
plots but contain no latitude/longitude for that phone. WAV headers contain no
capture-clock metadata. No absolute microphone coordinate or height was found
in the inspected source materials, including the frequency-extraction tool.

The summary field `received_start_utc` equals the **first supplied ridge point**.
The WAVs extend approximately ten seconds beyond each ridge's time span. Thus
that value is not proven to be the UTC time of the first WAV sample. The source
tool supports manual/file-name-derived timing, but the normalized `audio.wav`
names have lost the original timestamp. Keep waveform start unknown until its
alignment is independently established. Do not invent a centered ten-second
padding convention.

Accordingly all three cases are marked ineligible for independent absolute
location-error scoring. One can estimate a coordinate or audio-clock alignment
from these measurements, but that inferred value must not then be presented as
independent ground truth.

## Conversion requirements

KML `gx:coord` order is longitude, latitude, altitude. Here altitude is explicitly
**EGM96 orthometric MSL**, as documented by the dataset and file names. The engine
package currently expects ellipsoidal heights: convert using `h = H_EGM96 + N`,
with a documented EGM96 geoid undulation, and use a consistent vertical reference
for the observer. Merely renaming the MSL altitude field would produce a datum
error. A local common-height approximation may be useful experimentally if
clearly labeled; it is not a datum conversion.

Each KML has three `gx:Track` segments (before flight, flight, after flight).
Preserve those boundaries and do not interpolate across long gaps. The normalized
CSV sorts times within each segment and retains the first position at duplicated
timestamps. It records duplicate/conflict counts in the manifest; the N784MB and
N788BM sources contain conflicting duplicate positions and need that uncertainty
considered. The cleanest initial reference is N373DF_R06, whose duplicate points
are identical.

`ridge.reference.json` uses the first ridge point as its explicit UTC origin,
and gives relative `time_s` values compatible with the engine's event-point
representation. `strength_db: 0` is an unavailable-strength placeholder; it is
not measured SNR. `center_s` is the midpoint of the selected ridge interval,
not a measured closest-approach time. Preserve those distinctions in any adapter.

## Reproduction

```sh
python corpus/pathak/prepare.py --verify
python corpus/pathak/prepare.py
python corpus/pathak/prepare.py --download
```

Verification is offline. Preparation regenerates normalized tracks, reference
events, and the manifest from existing files. `--download` restores only the
three selected source folders from the verified ZIP before preparation. It uses
Python's standard library and the public Zenodo URL. `source_record.json` records
the source API metadata, licensing, filenames, sizes, and published checksums.

The parent news manifest remains separate and unchanged.
