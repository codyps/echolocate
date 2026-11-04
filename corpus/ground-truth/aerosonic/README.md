# AeroSonicDB selected aircraft recordings

Four original, unchanged60-second mono22,050Hz PCM16 WAV files were acquired directly from v1.1.2 Zenodo using HTTP byte ranges. Member CRC32s and lengths were checked; full archive MD5 was not verified. Source metadata, license and localSHA256 hashes retained.

These improve the news labels: each has a filename recording clock, ADS-B aircraft ID, site ID, microphone ID, and human event boundaries. **They do not supply exact microphone coordinates.** The publisher deliberately provides nearby public locations without a displacement bound. Distance to that reference is a coarse diagnostic, not microphone error. Filename timezone and calibrated audio-clock accuracy are unstated; both Adelaide-local and UTC interpretations are preserved.

| File | Aircraft | Public site | Filename clock |
|---|---|---|---|
| 7C1CA5_2023-05-09_12-52-42_2_1 | PC12 / VH-FXZ | 2 | 12-52-42 |
| 7C79B7_2023-05-09_12-39-52_2_1 | DA42 / VH-YBT | 2 | 12-39-52 |
| 7C6275_2023-05-09_14-22-08_0_1 | C208 / VH-TQF | 0 | 14-22-08 |
| 7C3FCB_2023-05-09_15-10-12_0_1 | BE20 / VH-MVX | 0 | 15-10-12 |

## Attribution and use

Blake Downward, AeroSonicDB (YPAD-0523), v1.1.2, DOI10.5281/zenodo.10215080. Audio is CC BY-NC4.0; the repository software license does not replace the dataset license. Keep audio out of the default general-purpose public static package. Independently obtained flight tracks retain their source license.

Source locations.json is malformed: escaped whitespace and one missing closing brace. Original retained unchanged; locations.normalized.json repairs syntax only, preserving numbers and the original misspelled lattitude key.

Source text: SOURCE_README.md. Evidence/hashes: manifest.json. Restore: `python prepare.py --download`; verify: `python prepare.py --verify`.
