use echo_core::{model::*, physics::distance_m};
use flate2::read::GzDecoder;
use serde_json::Value;
use std::{
    collections::BTreeMap,
    fs,
    io::{Read, Seek, SeekFrom},
    path::Path,
};
pub fn bounds(s: &str) -> Result<Bounds, String> {
    let parts: Vec<f64> = s
        .split(',')
        .map(|v| v.trim().parse::<f64>().map_err(|e| e.to_string()))
        .collect::<Result<_, _>>()?;
    if parts.len() != 4 {
        return Err("Bounds require west,south,east,north".into());
    }
    let b = Bounds {
        west: parts[0],
        south: parts[1],
        east: parts[2],
        north: parts[3],
    };
    if !b.valid() {
        return Err("Invalid bounds".into());
    }
    Ok(b)
}
fn package(
    mut flights: Vec<Flight>,
    b: Bounds,
    source: String,
    license: String,
    notes: Vec<String>,
) -> Result<DataPackage, String> {
    // readsb leg flags can flush a segment before end-of-file normalization.
    // Normalize every segment, rejecting conflicting duplicate timestamps rather than choosing a position.
    let mut conflicts = 0;
    let mut invalid = 0;
    for f in &mut flights {
        let before = f.points.len();
        f.points.retain(|p| {
            [p.time_unix_s, p.lat, p.lon, p.height_ellipsoid_m]
                .iter()
                .all(|v| v.is_finite())
                && p.lat.abs() <= 90.
                && p.lon.abs() <= 180.
                && (-500.0..=30_000.).contains(&p.height_ellipsoid_m)
        });
        invalid += before - f.points.len();
        f.points
            .sort_by(|a, b| a.time_unix_s.total_cmp(&b.time_unix_s));
        let mut clean = Vec::new();
        let mut at = 0;
        while at < f.points.len() {
            let mut end = at + 1;
            while end < f.points.len() && f.points[end].time_unix_s == f.points[at].time_unix_s {
                end += 1;
            }
            let first = &f.points[at];
            if f.points[at + 1..end].iter().any(|p| {
                distance_m((p.lat, p.lon), (first.lat, first.lon)) > 1.
                    || (p.height_ellipsoid_m - first.height_ellipsoid_m).abs() > 1.
            }) {
                conflicts += 1;
            } else {
                clean.push(first.clone());
            }
            at = end;
        }
        f.points = clean;
    }
    flights.retain(|f| f.points.len() >= 2);
    let mut notes = notes;
    notes.push(format!("All segments sorted and normalized; removed {conflicts} conflicting duplicate-time groups and {invalid} invalid coordinate/height points."));
    let start = flights
        .iter()
        .flat_map(|f| &f.points)
        .map(|p| p.time_unix_s)
        .fold(f64::INFINITY, f64::min);
    let end = flights
        .iter()
        .flat_map(|f| &f.points)
        .map(|p| p.time_unix_s)
        .fold(f64::NEG_INFINITY, f64::max);
    if flights.is_empty() {
        return Err("No usable flight segments. Geometric heights, positions and at least two distinct timestamps are required.".into());
    }
    let p = DataPackage {
        schema_version: 1,
        name: "Imported regional trajectories".into(),
        source,
        license,
        coordinate_frame: "wgs84".into(),
        clock_domain: "utc".into(),
        coverage_start_unix_s: start,
        coverage_end_unix_s: end,
        search_bounds: b,
        synthetic: false,
        coverage_notes: notes,
        flights,
    };
    p.validate()?;
    Ok(p)
}
fn files(path: &Path, out: &mut Vec<std::path::PathBuf>) -> Result<(), String> {
    if path.is_file() {
        out.push(path.to_path_buf());
    } else {
        for entry in fs::read_dir(path).map_err(|e| e.to_string())? {
            let p = entry.map_err(|e| e.to_string())?.path();
            if p.is_symlink() {
                continue;
            }
            if p.is_dir() {
                files(&p, out)?;
            } else if p.extension().is_some_and(|x| x == "gz" || x == "json") {
                out.push(p);
            }
        }
    }
    Ok(())
}
pub fn readsb(
    input: &Path,
    b: Bounds,
    start: Option<f64>,
    end: Option<f64>,
) -> Result<DataPackage, String> {
    let mut paths = Vec::new();
    files(input, &mut paths)?;
    paths.sort();
    let mut flights = Vec::new();
    let mut missing = 0;
    let mut stale = 0;
    let mut duplicates = 0;
    let margin_lat = 50_000. / 111_320.;
    let margin_lon = margin_lat / ((b.south + b.north) / 2.).to_radians().cos();
    for path in paths {
        let mut f = fs::File::open(&path).map_err(|e| e.to_string())?;
        let mut magic = [0u8; 2];
        let n = f.read(&mut magic).map_err(|e| e.to_string())?;
        f.seek(SeekFrom::Start(0)).map_err(|e| e.to_string())?;
        let reader: Box<dyn Read> = if n == 2 && magic == [0x1f, 0x8b] {
            Box::new(GzDecoder::new(f))
        } else {
            Box::new(f)
        };
        let mut bytes = Vec::new();
        reader
            .take(32 * 1024 * 1024 + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| e.to_string())?;
        if bytes.len() > 32 * 1024 * 1024 {
            return Err(format!("Trace too large: {}", path.display()));
        }
        let v: Value =
            serde_json::from_slice(&bytes).map_err(|e| format!("{}: {e}", path.display()))?;
        let Some(trace) = v["trace"].as_array() else {
            continue;
        };
        let base = v["timestamp"].as_f64().ok_or("Trace timestamp missing")?;
        let id = v["icao"].as_str().unwrap_or("unknown");
        let kind = v["t"].as_str().unwrap_or("");
        let mut points = Vec::new();
        let mut segment = 0;
        for row in trace {
            let Some(r) = row.as_array() else {
                continue;
            };
            if r.len() < 7 {
                continue;
            }
            let flags = r[6].as_u64().unwrap_or(0);
            if flags & 2 != 0 && !points.is_empty() {
                if points.len() >= 2 {
                    flights.push(Flight {
                        id: format!("{id}-{}-{segment}", base as i64),
                        aircraft_type: kind.into(),
                        points: std::mem::take(&mut points),
                    });
                } else {
                    points.clear();
                }
                segment += 1;
            }
            if flags & 1 != 0 {
                stale += 1;
                continue;
            }
            let Some(t) = r[0].as_f64().map(|dt| dt + base) else {
                continue;
            };
            if start.is_some_and(|s| t < s) || end.is_some_and(|e| t > e) {
                continue;
            }
            let (Some(lat), Some(lon)) = (r[1].as_f64(), r[2].as_f64()) else {
                continue;
            };
            if lat < b.south - margin_lat
                || lat > b.north + margin_lat
                || lon < b.west - margin_lon
                || lon > b.east + margin_lon
            {
                continue;
            }
            // readsb docs: slot 10 is geometric feet; flag 8 identifies geometric slot 3.
            let geom = r
                .get(10)
                .and_then(Value::as_f64)
                .or_else(|| {
                    r.get(8)
                        .and_then(|x| x.get("alt_geom"))
                        .and_then(Value::as_f64)
                })
                .or_else(|| if flags & 8 != 0 { r[3].as_f64() } else { None });
            let Some(alt) = geom else {
                missing += 1;
                continue;
            };
            points.push(TrackPoint {
                time_unix_s: t,
                lat,
                lon,
                height_ellipsoid_m: alt * 0.3048,
            });
        }
        points.sort_by(|a, b| a.time_unix_s.total_cmp(&b.time_unix_s));
        let before = points.len();
        duplicates += points
            .windows(2)
            .filter(|p| p[0].time_unix_s == p[1].time_unix_s)
            .count();
        let _ = before;
        if points.len() >= 2 {
            flights.push(Flight {
                id: format!("{id}-{}-{segment}", base as i64),
                aircraft_type: kind.into(),
                points,
            });
        }
    }
    package(flights,b,"adsb.lol historical readsb traces; https://www.adsb.lol/docs/open-data/historical".into(),"ODbL-1.0".into(),vec![format!("Import skipped {missing} positions without geometric altitude and {stale} stale positions; removed {duplicates} duplicate timestamps."),"Aircraft coverage is incomplete and not independently measured. No barometric altitude was silently treated as ellipsoidal height. Queries include a 50 km aircraft margin; the inference range remains separately configured.".into()])
}
pub fn csv(
    input: &Path,
    b: Bounds,
    source: String,
    license: String,
    geoid: Option<f64>,
) -> Result<DataPackage, String> {
    if geoid.is_some_and(|n| !n.is_finite() || n.abs() > 150.) {
        return Err("Geoid undulation must be finite and within ±150 m".into());
    }
    let mut reader = ::csv::Reader::from_path(input).map_err(|e| e.to_string())?;
    let headers = reader.headers().map_err(|e| e.to_string())?.clone();
    let idx = |name: &str| headers.iter().position(|h| h == name);
    let time = idx("time_unix_s").ok_or("CSV needs time_unix_s")?;
    let lat = idx("lat").ok_or("CSV needs lat")?;
    let lon = idx("lon").ok_or("CSV needs lon")?;
    let ellipsoid = idx("height_ellipsoid_m");
    let msl = idx("height_egm96_msl_m").or_else(|| idx("height_msl_m"));
    if ellipsoid.is_none() && (msl.is_none() || geoid.is_none()) {
        return Err("CSV needs height_ellipsoid_m, or MSL heights plus explicit --geoid-undulation-m. Never relabel MSL as ellipsoid.".into());
    }
    let mut groups: BTreeMap<String, Vec<TrackPoint>> = BTreeMap::new();
    for row in reader.records() {
        let row = row.map_err(|e| e.to_string())?;
        let num = |i: usize| {
            row.get(i)
                .ok_or("CSV value missing".to_owned())
                .and_then(|s| s.parse::<f64>().map_err(|e| e.to_string()))
        };
        let alt = if let Some(i) = ellipsoid {
            num(i)?
        } else {
            num(msl.unwrap())? + geoid.unwrap()
        };
        let id = idx("flight_id")
            .and_then(|i| row.get(i))
            .unwrap_or("imported");
        let seg = idx("segment_index").and_then(|i| row.get(i)).unwrap_or("0");
        groups
            .entry(format!("{id}-{seg}"))
            .or_default()
            .push(TrackPoint {
                time_unix_s: num(time)?,
                lat: num(lat)?,
                lon: num(lon)?,
                height_ellipsoid_m: alt,
            });
    }
    let mut flights = Vec::new();
    for (id, mut points) in groups {
        points.sort_by(|a, b| a.time_unix_s.total_cmp(&b.time_unix_s));
        for p in points.windows(2) {
            if p[0].time_unix_s == p[1].time_unix_s
                && distance_m((p[0].lat, p[0].lon), (p[1].lat, p[1].lon)) > 1.
            {
                return Err(format!("Conflicting duplicate timestamp in {id}"));
            }
        }
        points.dedup_by(|a, b| a.time_unix_s == b.time_unix_s);
        if points.len() >= 2 {
            flights.push(Flight {
                id,
                aircraft_type: String::new(),
                points,
            });
        }
    }
    package(flights,b,source,license,vec!["Imported CSV; source completeness and microphone position are not established.".into(),format!("Geoid undulation applied to MSL heights: {geoid:?}. A constant correction is only suitable for a small regional package.")])
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn every_leg_is_normalized_and_bad_geometric_heights_are_counted() {
        let point = |t, h| TrackPoint {
            time_unix_s: t,
            lat: 37.5,
            lon: -122.5,
            height_ellipsoid_m: h,
        };
        let flights = vec![Flight {
            id: "leg0".into(),
            aircraft_type: String::new(),
            points: vec![
                point(3., 1000.),
                point(1., 1000.),
                point(1., 1000.),
                point(2., 1e8),
            ],
        }];
        let p = package(
            flights,
            bounds("-123,37,-122,38").unwrap(),
            "source".into(),
            "ODbL-1.0".into(),
            vec![],
        )
        .unwrap();
        assert_eq!(p.flights[0].points.len(), 2);
        assert_eq!(p.flights[0].points[0].time_unix_s, 1.);
        assert!(p.coverage_notes[0].contains("1 invalid"));
    }
    #[test]
    fn bounds_reject_antimeridian_and_invalid_values() {
        assert!(bounds("170,-10,-170,10").is_err());
        assert!(bounds("NaN,0,1,1").is_err());
        assert!(bounds("-123,37,-122,38").is_ok());
    }
    #[test]
    fn geometric_readsb_height_and_stale_filter() {
        let dir = std::env::temp_dir().join(format!("echo-trace-test-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("trace.json"),r#"{"icao":"abc123","timestamp":1000,"trace":[[0,37.5,-122.5,1000,100,90,0,0,null,"adsb_icao",1100],[1,37.5,-122.499,1000,100,90,1,0,null,"adsb_icao",1100],[2,37.5,-122.498,1000,100,90,8,0,null,"adsb_icao",null],[3,37.5,-122.497,1000,100,90,0]]}"#).unwrap();
        let p = readsb(&dir, bounds("-123,37,-122,38").unwrap(), None, None).unwrap();
        assert_eq!(p.flights[0].points.len(), 2);
        assert!((p.flights[0].points[0].height_ellipsoid_m - 335.28).abs() < 1e-6);
        assert!((p.flights[0].points[1].height_ellipsoid_m - 304.8).abs() < 1e-6);
        fs::remove_dir_all(dir).unwrap();
    }
}
