# Frozen pre-improvement baseline

This is our own earlier implementation, extracted from the previous delivered project archive before this development iteration. It contains no third-party Doppler implementations.

Build with `cargo build --release --locked --manifest-path validation-baseline/Cargo.toml --target-dir target/baseline`. Validation uses the saved native binary and records its SHA-256. Do not tune this source against the evaluation recordings.
