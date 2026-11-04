use echo_core::{model::*, physics::*, search::match_event};
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let v: serde_json::Value = serde_json::from_slice(&std::fs::read(&a[1]).unwrap()).unwrap();
    let result: Analysis = serde_json::from_value(v["analysis"].clone()).unwrap();
    let pack: DataPackage = serde_json::from_slice(&std::fs::read(&a[2]).unwrap()).unwrap();
    let mut c = result.config;
    c.robust_shape = true;
    let x = ecef(40.876728, -73.526434, -30.);
    for e in result.events {
        for f in &pack.flights {
            let tr = Trajectory::new(f);
            for dt in [-2., 0., 2.] {
                if let Some(m) = match_event(&e, &tr, x, Vec3(0., 0., 0.), 1788900750.8 + dt, &c) {
                    println!(
                        "e{} {} dt{dt} {} rms{} inlier{} tone{}",
                        e.id, m.flight_id, m.shape_gain, m.rms_hz, m.inlier_fraction, m.tone_hz
                    );
                }
            }
        }
    }
}
