#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
cargo build --release --locked -p echo-locate
cargo build --release --locked -p echo-core --target wasm32-unknown-unknown
cp target/wasm32-unknown-unknown/release/echo_core.wasm web/echo_core.wasm
target/release/echo-locate demo --output web
mkdir -p dist
cp web/index.html web/style.css web/app.js web/data-client.js web/worker.js web/echo_core.wasm web/demo.wav web/demo.package.json web/demo.config.json dist/
python scripts/licenses.py
cp docs/THIRD_PARTY_NOTICES.txt dist/NOTICE.txt
if [[ -d web/data ]]; then cp -R web/data dist/; fi
touch dist/.nojekyll
printf 'Native binary: target/release/echo-locate\nStatic bundle: dist/\nRun: target/release/echo-locate serve --web dist\n'
