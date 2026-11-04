# Portable flight packages

Version 1 is UTF-8 JSON:

```json
{
  "schema_version": 1,
  "name": "Regional date window",
  "source": "URL and acquisition description",
  "license": "ODbL-1.0",
  "coverage_start_unix_s": 1762200000,
  "coverage_end_unix_s": 1762201000,
  "search_bounds": {"west":-123,"south":37,"east":-122,"north":38},
  "synthetic": false,
  "coverage_notes": ["Observed receiver coverage is not complete"],
  "flights": [{
    "id": "unique-aircraft-and-segment-id",
    "aircraft_type": "optional type string",
    "points": [
      {"time_unix_s":1762200000,"lat":37.5,"lon":-122.5,"height_ellipsoid_m":1400},
      {"time_unix_s":1762200005,"lat":37.5,"lon":-122.495,"height_ellipsoid_m":1400}
    ]
  }]
}
```

Positions are WGS84 degrees and **ellipsoidal meters**. Point times are emission-domain UTC seconds. Playback/recording times are reception-domain times; the engine computes their propagation delay.

The file must have unique flight IDs, strictly increasing point timestamps, valid positions and finite heights. Point times must lie inside declared coverage. V1 supports at most 10,000 flight segments and two million points; browser input metadata is additionally capped at 16 MiB. Split larger corpora into tiles/windows. Crossing the antimeridian is not supported.

Trajectory interpolation is piecewise linear in ECEF. Its derivative supplies velocity. Gaps over `max_track_gap_s`, extrapolation, and segments exceeding 350 m/s are refused. Piecewise-linear velocity discontinuities and noisy positions remain limitations; the importer does not invent smoother ground truth.

The readsb importer follows the [published JSON trace format](https://github.com/wiedehopf/readsb/blob/dev/README-json.md): slot 10 gives geometric altitude in feet; an explicit geometric flag can identify slot 3. Per-point `alt_geom` is also accepted. Stale points and positions without geometric height are excluded. Gzip content is detected by its signature, including files named `.json`.

EGM96/EGM2008 orthometric heights and pressure altitudes are not interchangeable with these fields. A small-region constant geoid correction is supported for CSV import only when explicitly supplied. The Pathak source heights are EGM96 MSL; their files are retained in their original frame and have not been silently repackaged as ellipsoidal positions.

No v1 package asserts receiver-network completeness or acoustic audibility. Missing aircraft must not be treated as evidence of silence.
