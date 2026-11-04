#!/usr/bin/env python3
"""Inspect/normalize three independent Pathak aircraft cases (stdlib only).

Source WAV, KML, ridge CSV, and PDF bytes remain unchanged. Normalized aircraft
heights are EGM96 MSL, not ellipsoidal. No microphone truth is inferred.
"""
import argparse
import csv
from datetime import datetime
import hashlib
import json
from pathlib import Path
import tempfile
import urllib.request
import wave
import xml.etree.ElementTree as ET
import zipfile

ROOT = Path(__file__).resolve().parent
CASES = ["N373DF_R06", "N784MB_R04", "N788BM_R03"]
ZIP_URL = "https://zenodo.org/api/records/23424278/files/Raw%20data_V_constant_H_constant.zip/content"
ZIP_MD5 = "d34b32ae57477074d6e8563300a7fc19"
NS = {"k": "http://www.opengis.net/kml/2.2", "g": "http://www.google.com/kml/ext/2.2"}


def unix(value):
    return datetime.fromisoformat(value.replace("Z", "+00:00")).timestamp()


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def acquire():
    with urllib.request.urlopen(ZIP_URL, timeout=60) as stream:
        content = stream.read(24_000_001)
    if len(content) != 23_096_453 or hashlib.md5(content).hexdigest() != ZIP_MD5:
        raise SystemExit("Source ZIP size/checksum differs from published record")
    with tempfile.TemporaryFile() as temp:
        temp.write(content)
        temp.seek(0)
        with zipfile.ZipFile(temp) as archive:
            for case in CASES:
                prefix = "V_constant_H_constant/" + case + "/"
                for member in archive.namelist():
                    tail = member.removeprefix(prefix)
                    if member.startswith(prefix) and tail and "/" not in tail:
                        dest = ROOT / case / tail
                        dest.parent.mkdir(parents=True, exist_ok=True)
                        dest.write_bytes(archive.read(member))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--download", action="store_true", help="Restore original selected source files (23.1 MB ZIP)")
    parser.add_argument("--verify", action="store_true", help="Verify manifest file hashes only, offline")
    args = parser.parse_args()
    if args.verify:
        manifest = json.loads((ROOT / "manifest.json").read_text())
        for case in manifest["cases"]:
            for entry in case["files"]:
                p = ROOT / entry["path"]
                if not p.exists() or sha(p) != entry["sha256"]:
                    raise SystemExit(f"Mismatch or missing: {p}")
            print(case["id"] + ": all file hashes verified")
        return
    if args.download:
        acquire()
    summary = {r["case"]: r for r in csv.DictReader((ROOT / "final_results_40_cases.csv").open())}
    cases = []
    for case in CASES:
        folder = ROOT / case
        kml = next(folder.glob("*.kml"))
        tree = ET.parse(kml).getroot()
        rows = []
        diagnostics = []
        for index, track in enumerate(tree.findall(".//g:Track", NS)):
            times = [e.text for e in track.findall("k:when", NS)]
            coords = [list(map(float, e.text.split())) for e in track.findall("g:coord", NS)]
            if len(times) != len(coords):
                raise ValueError("gx:Track time/coordinate count mismatch")
            unique = {}
            conflicts = 0
            for value, xyz in zip(times, coords):
                t = unix(value)
                if t in unique and unique[t][1] != xyz:
                    conflicts += 1
                unique.setdefault(t, (value, xyz))
            for t, (value, xyz) in sorted(unique.items()):
                rows.append([index, value, f"{t:.3f}", xyz[1], xyz[0], xyz[2]])
            diagnostics.append({"segment_index": index, "input_points": len(times), "unique_points": len(unique), "duplicates": len(times) - len(unique), "conflicting_duplicates": conflicts})
        norm = folder / "track.normalized.csv"
        with norm.open("w", newline="") as output:
            writer = csv.writer(output)
            writer.writerow(["segment_index", "time_utc", "time_unix_s", "lat", "lon", "height_egm96_msl_m"])
            writer.writerows(rows)
        ridge = list(csv.DictReader((folder / "frequency_vs_time_used.csv").open()))
        origin = unix(ridge[0]["time_utc"])
        points = [{"time_s": unix(r["time_utc"]) - origin, "frequency_hz": float(r["frequency_hz"]), "strength_db": 0.0} for r in ridge]
        reference = {"source": "author supplied ridge; not automatic detector output", "time_origin_unix_s": origin, "time_origin_is_wav_start": False, "event": {"id": 0, "start_s": 0.0, "end_s": points[-1]["time_s"], "center_s": points[-1]["time_s"] / 2, "drop_hz": points[0]["frequency_hz"] - points[-1]["frequency_hz"], "harmonics": 1, "points": points}}
        (folder / "ridge.reference.json").write_text(json.dumps(reference, indent=2) + "\n")
        with wave.open(str(folder / "audio.wav"), "rb") as wav:
            audio = {"sample_rate_hz": wav.getframerate(), "channels": wav.getnchannels(), "sample_width_bytes": wav.getsampwidth(), "frames": wav.getnframes(), "duration_seconds": wav.getnframes() / wav.getframerate()}
        entry = {"id": case, "aircraft_registration": case.split("_")[0], "category": "V_constant_H_constant", "audio": audio, "first_ridge_utc": ridge[0]["time_utc"], "last_ridge_utc": ridge[-1]["time_utc"], "ridge_point_count": len(ridge), "waveform_first_sample_utc": None, "microphone_coordinate": None, "altitude_reference": "EGM96 orthometric mean sea level", "normalization": diagnostics, "author_summary": summary[case], "validation": {"role": "reference ridge and known-flight association; raw-audio event discovery comparison", "independent_location_error_eligible": False, "known_missing": ["surveyed microphone coordinate and height", "verified mapping of first WAV sample to UTC", "recording-clock and atmospheric uncertainty"]}}
        entry["files"] = [{"path": str(p.relative_to(ROOT)), "bytes": p.stat().st_size, "sha256": sha(p), "derived": p.name in ["track.normalized.csv", "ridge.reference.json", "result_page.txt"]} for p in sorted(folder.iterdir()) if p.is_file()]
        cases.append(entry)
    manifest = {"schema_version": 1, "source": {"title": "Aeroplane data full analysis and raw files", "author": "Praveen Pathak", "url": "https://zenodo.org/records/23424278", "doi": "10.5281/zenodo.23424278", "published_date": "2026-10-16", "license": "CC-BY-4.0", "license_url": "https://creativecommons.org/licenses/by/4.0/", "archive_url": ZIP_URL, "archive_md5": ZIP_MD5, "source_metadata": "source_record.json"}, "selection": "Three cases selected by source archive category, before running the localization engine. Development cases, not a held-out accuracy benchmark.", "cases": cases}
    (ROOT / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n")
    print(f"Prepared {len(cases)} cases; source WAVs unchanged")


if __name__ == "__main__":
    main()
