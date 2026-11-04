# Historical flight data for collected recordings

These are actual archived aircraft traces, acquired independently from adsb.lol and its original GitHub release assets. They are not synthetic tracks and contain no dopplerprint assets or code.

The public map's half-hour replay files were used only to discover candidate aircraft in the declared regions and times. Their altitude values are barometric. Full per-aircraft readsb traces were then acquired for the Rust geometric-height importer. No replay altitude was silently relabeled as a geometric height.

| Recording | Discovery windows (UTC) | Acquired full traces | Limitation |
|---|---|---:|---|
| FOX Thunderbirds, 2025-09-12 | 12:00–13:00 | 188 of 189 discovered IDs | TBIRD14 is a C-17 support aircraft; it is not proof of an audible F-16 association. |
| FOX Frecce Tricolori, 2024-08-21 | 20:00–21:30 | 245 of 245 | No archived trace is independently associated with the audible formation. |
| DVIDS C-130J Yuma, 2025-02-26 | 17:00–17:30, 19:00–19:30, 21:00–21:30 | 19 of 19 | Sparse sampled windows only; Fat Albert was not identified, which does not establish its absence. |
| FOX Air Force One, 2025-11-09 | 21:30–22:00 | AF1 plus two competitors | 76 regional IDs were discovered; direct trace endpoints failed. AF1 has barometric/MLAT heights. |
| Creator reference, 2026-09-08 | 20:30–21:30 | 36 of 133 IDs within the 20 km town prior | Partial, unfiltered-by-type geographic sample; not complete traffic coverage. |

There are 491 actual full traces across these five cases. The first three cases have 187, 234 and 19 respectively with at least two geometric-altitude records somewhere in the full trace. Both AF1 competitors have geometric heights, but AF1 itself does not. All 36 creator-reference traces have geometric height records. Positions relevant to an acoustic interval must still be checked after filtering; full-day point counts do not guarantee local coverage.

## Creator reference

The 2026-09-08 reference includes PC-12 N22NY (`a1dfeb`), King Air 350 N93CQ (`ace391`), and 34 geographic competitors under `2026-09-08/traces/`. Both replay chunks were recovered from the original daily release because the public map endpoints returned 404.

A 20 km query around the rounded Oyster Bay town prior, 40.88, -73.53, found 133 IDs. We acquired a partial sample through uniformly spaced archive header windows and additional archive hash-directory windows containing multiple eligible aircraft. Neither aircraft type nor acoustic fit score was used to select competitors. The dataset includes airliners, business jets, piston aircraft, turboprops and a helicopter.

Complete source byte ranges, hashes, discovery coverage and selection details are in `2026-09-08/competitive-receipt.json`. The two known aircraft have the expected geographic timing near Oyster Bay: PC-12 near 20:53 UTC and King Air near 21:08 UTC. This does not make their supplied identity a blind association test. Keep a separate two-aircraft baseline and report that the competitive package has only 36 of 133 discovered IDs. Unacquired traffic could change its ranking.

The 2024/2025 N93CQ traces found during date probes are explicitly excluded from this reference. They are retained only as dated acquisition evidence in `original-year-probe.json`; their tracks do not substantiate the 2026 events.

## Reproduce and verify

`python corpus/news-tracks/acquire.py` obtains the declared news replay windows and their full aircraft traces. Direct endpoint availability can change. `manifest.json`, per-day discovery files and trace receipts retain source paths, point counts, hashes, attempted windows and failures.

The original-release fallback requires no large archive download or scratch index cache:

```bash
# Check all 491 acquired trace files against their receipts.
python corpus/news-tracks/verify.py

# Check all 43 exact original archive members, including the ODbL license.
python corpus/news-tracks/restore_ranges.py --verify

# Restore missing/changed members by verified HTTP byte range and SHA-256.
python corpus/news-tracks/restore_ranges.py --restore

# Limit restoration to the creator-reference source files.
python corpus/news-tracks/restore_ranges.py --restore --date 2026-09-08
```

`range-downloads.json` records each path, public source URL, exact byte interval, original member name and digest. Only requested members are downloaded. Every returned Content-Range, byte count and SHA-256 must match. The helper fails if the source changes rather than accepting a different file.

Original releases:

- [2025-09-12](https://github.com/adsblol/globe_history_2025/releases/tag/v2025.09.12-planes-readsb-prod-0)
- [2024-08-21](https://github.com/adsblol/globe_history_2024/releases/tag/v2024.08.21-planes-readsb-prod-0)
- [2025-02-26](https://github.com/adsblol/globe_history_2025/releases/tag/v2025.02.26-planes-readsb-prod-0)
- [2025-11-09](https://github.com/adsblol/globe_history_2025/releases/tag/v2025.11.09-planes-readsb-prod-0)
- [2026-09-08](https://github.com/adsblol/globe_history_2026/releases/tag/v2026.09.08-planes-readsb-prod-0)

## Air Force One evidence

The source trace is ICAO `adfdf9`, Boeing B742 registration `92-9000`, callsign `AF1`, with 838 archived positions. Its closest recorded point to the approximate stadium center (38.9076, -76.8645) is at 2025-11-09T21:37:47.460Z, about 291 m horizontally away, with barometric altitude 850 ft and source `mlat`. This agrees with the [primary pool report](https://www.presidency.ucsb.edu/documents/pool-reports-november-9-2025), which places the stadium flyover at 16:37 EST (21:37 UTC) and landing at Andrews at 16:40.

This establishes flight/event evidence, not the news clip's first-sample UTC, microphone position or acoustic association. The strict geometric-height importer must not manufacture AF1's height. A barometric-height experiment needs a documented pressure/geoid conversion or an altitude uncertainty model.

## Redistribution and static hosting

The original 2025-09-12 release tar contains `./LICENSE-ODbL.txt`. Its exact 25,241-byte contents were acquired with a verified HTTP 206 range and are included here with `license-receipt.json`. The [historical data documentation](https://www.adsb.lol/docs/open-data/historical) also labels the archive ODbL 1.0. The privacy page's CC0 text is a feeder contribution grant; it does not replace the archive database license.

Publish regional/temporal subsets and other derived databases with ODbL licensing, visible adsb.lol attribution and machine-readable provenance. The application code can remain MIT. Prefer compact selected flight packages for GitHub/Pages over global replay chunks. Raw replay chunks are ignored for source control. These data rights do not grant rights to publish FOX audio.

## Validation boundary

A flight in a search package is a candidate, not a confirmed sound source. These files enable real historical-trajectory searches. They do not by themselves establish microphone coordinates, recording clocks, complete airspace coverage or calibrated location error. Preserve that distinction when reporting candidate estimates, ambiguous results and abstentions.
