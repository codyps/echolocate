use echo_core::{audio, model::Config};
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let c: Config = serde_json::from_slice(&std::fs::read(&a[2]).unwrap()).unwrap();
    let d = audio::decode(&std::fs::read(&a[1]).unwrap(), c.max_audio_s).unwrap();
    let (summary, spec, events) = audio::spectrogram(&d, &c).unwrap();
    std::fs::write(
        &a[3],
        serde_json::to_vec(
            &serde_json::json!({"audio":summary,"spectrogram":spec,"events":events}),
        )
        .unwrap(),
    )
    .unwrap();
}
