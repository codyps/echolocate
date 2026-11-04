#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
binary=target/release/echo-locate
if [[ ! -x "$binary" ]] && [[ "$(uname -s)" == Linux && "$(uname -m)" == x86_64 ]] && [[ -x bin/echo-locate-linux-x86_64 ]]; then
  binary=bin/echo-locate-linux-x86_64
fi
if [[ ! -x "$binary" || ! -f web/echo_core.wasm ]]; then
  bash scripts/build.sh
  binary=target/release/echo-locate
fi
web_directory=web
if [[ -f dist/echo_core.wasm ]]; then web_directory=dist; fi
exec "$binary" serve --web "$web_directory" "$@"
