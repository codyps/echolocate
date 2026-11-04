use echo_core::{model::*, physics::*};
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let p: DataPackage = serde_json::from_slice(&std::fs::read(&a[1]).unwrap()).unwrap();
    let x = ecef(40.876728, -73.526434, -30.);
    for f in p.flights {
        let tr = Trajectory::new(&f);
        let (t, r) = tr.cpa(x, 30.).unwrap();
        println!(
            "{} cpa emit{} arrive{} r{}",
            tr.id,
            t - 1788900750.8,
            t + r / 343. - 1788900750.8,
            r
        );
        for tt in [935., 950., 965., 980., 995., 1010., 1025.] {
            let d = tr.doppler(x, 1788900750.8 + tt, 343., 30.);
            println!("{tt} {d:?}");
        }
    }
}
