#!/usr/bin/env python3
"""Acquire the declared short research excerpts or verify existing WAVs.

No credentials, paywall bypass, or private APIs. Requires ffmpeg for acquisition.
Media rights remain with the publishers; FOX excerpts are local research inputs,
not assets to redistribute with the public website.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import wave

ROOT = Path(__file__).resolve().parent


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--verify", action="store_true", help="Check local files without network access")
    parser.add_argument("--id", action="append", help="Only fetch/verify this entry; repeatable")
    args = parser.parse_args()
    manifest = json.loads((ROOT / "manifest.json").read_text())
    selected = [x for x in manifest["recordings"] if not args.id or x["id"] in args.id]
    if args.id and set(args.id) - {x["id"] for x in selected}:
        parser.error("Unknown recording id")
    env = dict(os.environ)
    if env.get("HTTPS_PROXY"):
        env.setdefault("http_proxy", env["HTTPS_PROXY"])
    failures = []
    for item in selected:
        dest = ROOT / item["audio"]["path"]
        if not dest.exists() and not args.verify:
            dest.parent.mkdir(parents=True, exist_ok=True)
            temp = dest.with_suffix(".partial.wav")
            try:
                subprocess.run([
                    "ffmpeg", "-hide_banner", "-loglevel", "error", "-y",
                    "-i", item["acquisition"]["media_url"],
                    "-ss", str(item["acquisition"]["start_seconds"]),
                    "-t", str(item["acquisition"]["duration_seconds"]),
                    "-vn", "-ac", "1", "-ar", "16000", "-map_metadata", "-1",
                    "-bitexact", "-c:a", "pcm_s16le", str(temp),
                ], env=env, timeout=120, check=True)
                temp.replace(dest)
            finally:
                temp.unlink(missing_ok=True)
        if not dest.exists():
            failures.append(item["id"] + ": missing file")
            continue
        actual = digest(dest)
        with wave.open(str(dest), "rb") as audio:
            frames, rate = audio.getnframes(), audio.getframerate()
            fmt = (audio.getnchannels(), audio.getsampwidth(), rate)
        if actual != item["audio"]["sha256"] or fmt != (1, 2, 16000):
            failures.append(item["id"] + ": hash/format differs; inspect decoder/source version")
        else:
            print(f"{item['id']}: verified {frames / rate:.3f} s, SHA-256 {actual}")
    if failures:
        raise SystemExit("\n".join(failures))


if __name__ == "__main__":
    main()
