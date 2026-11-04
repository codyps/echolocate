#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
# Requires: rustup target add x86_64-unknown-linux-musl
# Pure Rust dependencies allow a self-contained musl executable without a C toolchain.
echo_linker="$(rustc --print sysroot)/lib/rustlib/x86_64-unknown-linux-gnu/bin/rust-lld"
CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_LINKER="$echo_linker" \
  cargo build --release --locked -p echo-locate --target x86_64-unknown-linux-musl
mkdir -p bin
cp target/x86_64-unknown-linux-musl/release/echo-locate bin/echo-locate-linux-x86_64
printf 'Portable static Linux executable: bin/echo-locate-linux-x86_64\n'
