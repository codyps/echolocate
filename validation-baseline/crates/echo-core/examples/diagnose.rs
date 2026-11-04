//! Known-point physics diagnostic. This supplies receiver coordinates and is NOT blind validation.
use echo_core::{
    model::{DataPackage, Event},
    physics::{ecef, enu_vector, Trajectory},
};
fn main() {
    let a: Vec<String> = std::env::args().collect();
    if a.len() != 8 {
        eprintln!("diagnose ANALYSIS PACKAGE LAT LON START_UNIX HEIGHT MAX_GAP");
        std::process::exit(1);
    }
    let value: serde_json::Value = serde_json::from_slice(&std::fs::read(&a[1]).unwrap()).unwrap();
    let events: Vec<Event> = serde_json::from_value(value["analysis"]["events"].clone()).unwrap();
    let pack: DataPackage = serde_json::from_slice(&std::fs::read(&a[2]).unwrap()).unwrap();
    let lat: f64 = a[3].parse().unwrap();
    let lon: f64 = a[4].parse().unwrap();
    let start: f64 = a[5].parse().unwrap();
    let h: f64 = a[6].parse().unwrap();
    let gap: f64 = a[7].parse().unwrap();
    let x = ecef(lat, lon, h);
    for e in events {
        for f in &pack.flights {
            let tr = Trajectory::new(f);
            let pairs: Vec<_> = e
                .points
                .iter()
                .filter_map(|p| {
                    tr.doppler_with_wind(
                        x,
                        start + p.time_s,
                        343.,
                        gap,
                        enu_vector(lat, lon, 0., 0., 0.),
                    )
                    .map(|(d, _)| (p.frequency_hz, d))
                })
                .collect();
            if pairs.len() < 5 {
                continue;
            }
            let tone = pairs.iter().map(|(f, d)| f * d).sum::<f64>()
                / pairs.iter().map(|(_, d)| d * d).sum::<f64>();
            let rms = (pairs
                .iter()
                .map(|(f, d)| (f - tone * d).powi(2))
                .sum::<f64>()
                / pairs.len() as f64)
                .sqrt();
            println!(
                "event={} flight={} support={}/{} tone={:.3} rms={:.3}",
                e.id,
                f.id,
                pairs.len(),
                e.points.len(),
                tone,
                rms
            );
        }
    }
}
