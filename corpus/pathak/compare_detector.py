#!/usr/bin/env python3
"""Exploratory fitted agreement between automatic events and published ridges.

Fits a waveform offset and a discrete small-integer harmonic ratio. Neither
parameter is independent clock truth. This measures no geographic accuracy.
"""
import argparse
import bisect
from fractions import Fraction
import hashlib
import json
import math
from pathlib import Path
import statistics

ROOT = Path(__file__).resolve().parent
PROJECT = ROOT.parent.parent
MIN_SPAN = 8.0
MIN_POINTS = 20
MIN_EVENT_OVERLAP = 0.8
OFFSET_STEP = 0.05
MAX_RMS = 5.0
RATIOS = sorted({Fraction(p, q) for p in range(1, 7) for q in range(1, 7)})


def interpolate(points, times, time):
    i = bisect.bisect_right(times, time)
    if i == 0:
        return None
    if i == len(times):
        if abs(time - times[-1]) < 1e-7:
            return points[-1]["frequency_hz"], 0.0
        return None
    lo, hi = points[i - 1], points[i]
    gap = hi["time_s"] - lo["time_s"]
    fraction = (time - lo["time_s"]) / gap
    return lo["frequency_hz"] + fraction * (hi["frequency_hz"] - lo["frequency_hz"]), gap


def evaluate(event, reference, offset, ratios):
    points = reference["points"]
    times = [p["time_s"] for p in points]
    paired = []
    for p in event["points"]:
        value = interpolate(points, times, p["time_s"] - offset)
        if value is not None:
            paired.append((p["time_s"], p["frequency_hz"], value[0], value[1]))
    if len(paired) < MIN_POINTS:
        return None
    span = paired[-1][0] - paired[0][0]
    fullspan = event["points"][-1]["time_s"] - event["points"][0]["time_s"]
    coverage = span / fullspan if fullspan else 0.0
    if span + 1e-8 < MIN_SPAN or coverage + 1e-8 < MIN_EVENT_OVERLAP:
        return None
    best = None
    for ratio in ratios:
        r = float(ratio)
        errors = [obs / r - ref for _, obs, ref, _ in paired]
        rms = math.sqrt(sum(e * e for e in errors) / len(errors))
        if best is not None and rms >= best["rms_reference_hz"]:
            continue
        best = {
            "waveform_offset_s": offset,
            "observed_over_reference_ratio": str(ratio),
            "ratio_numerator": ratio.numerator,
            "ratio_denominator": ratio.denominator,
            "rms_reference_hz": rms,
            "rms_observed_hz": rms * r,
            "median_absolute_reference_error_hz": statistics.median(abs(e) for e in errors),
            "relative_rms_percent": 100 * rms / statistics.mean(x[2] for x in paired),
            "paired_points": len(paired),
            "observed_point_fraction": len(paired) / len(event["points"]),
            "overlap_span_s": span,
            "event_span_coverage_fraction": coverage,
            "reference_span_coverage_fraction": span / (times[-1] - times[0]),
            "reference_interval_s": [paired[0][0] - offset, paired[-1][0] - offset],
            "max_observed_gap_s": max(b[0] - a[0] for a, b in zip(paired, paired[1:])),
            "max_reference_interpolation_gap_s": max(x[3] for x in paired),
        }
    return best


def union_length(intervals):
    end = -math.inf
    total = 0.0
    for lo, hi in sorted(intervals):
        total += max(0.0, hi - max(lo, end))
        end = max(end, hi)
    return total


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--analysis-directory", type=Path, default=PROJECT / "validation-runs/corpus")
    args = parser.parse_args()
    manifest = json.loads((ROOT / "manifest.json").read_text())
    results = []
    for item in manifest["cases"]:
        case = item["id"]
        analysis_file = args.analysis_directory / (case + ".json")
        raw = analysis_file.read_bytes()
        analysis = json.loads(raw)["analysis"]
        reference_file = ROOT / case / "ridge.reference.json"
        reference_raw = reference_file.read_bytes()
        ref = json.loads(reference_raw)["event"]
        span = ref["points"][-1]["time_s"] - ref["points"][0]["time_s"]
        limit = max(0.0, analysis["audio"]["duration_s"] - span)
        offsets = [i * OFFSET_STEP for i in range(int(limit / OFFSET_STEP) + 1)] + [limit]
        events = []
        for event in analysis["events"]:
            fits = [evaluate(event, ref, offset, RATIOS) for offset in offsets]
            fits = [f for f in fits if f is not None]
            fit = min(fits, key=lambda f: f["rms_reference_hz"]) if fits else None
            if fit is not None:
                coarse = fit["waveform_offset_s"]
                refined = [evaluate(event, ref, max(0.0, min(limit, coarse + i * 0.005)), RATIOS) for i in range(-10, 11)]
                fit = min([fit] + [f for f in refined if f], key=lambda f: f["rms_reference_hz"])
            events.append({"event_id": event["id"], "start_s": event["start_s"], "end_s": event["end_s"], "point_count": len(event["points"]), "reported_harmonics": event["harmonics"], "reported_drop_hz": event["drop_hz"], "best_individual_fit": fit, "diagnostic_status": "insufficient_overlap" if fit is None else "within_exploratory_5hz_threshold" if fit["rms_reference_hz"] <= MAX_RMS else "unexplained_by_reference_at_tested_ratios"})
        # Anchor the shared offset on the longest event that passes the exploratory
        # residual threshold; then evaluate every other event at that SAME offset.
        # Selecting the offset to maximize covered area would favor stretching the
        # alignment until marginal fragments happen to cross the threshold.
        eligible = [e for e in events if e["best_individual_fit"] and e["best_individual_fit"]["rms_reference_hz"] <= MAX_RMS]
        anchor = max(eligible, key=lambda e: e["best_individual_fit"]["overlap_span_s"]) if eligible else None
        offset = anchor["best_individual_fit"]["waveform_offset_s"] if anchor else 0.0
        fits = [evaluate(event, ref, offset, RATIOS) for event in analysis["events"]]
        explained = [(e, f) for e, f in zip(analysis["events"], fits) if f and f["rms_reference_hz"] <= MAX_RMS]
        covered = union_length([f["reference_interval_s"] for _, f in explained])
        rms = sum(f["rms_reference_hz"] * f["overlap_span_s"] for _, f in explained) / sum(f["overlap_span_s"] for _, f in explained) if explained else None
        shared = {"anchor_event_id": anchor["event_id"] if anchor else None, "waveform_offset_s": offset if anchor else None, "reference_covered_union_s": covered, "reference_covered_fraction": covered / span, "duration_weighted_rms_reference_hz": rms, "matched_event_ids": [e["id"] for e, _ in explained], "fits": [{"event_id": e["id"], **f} for e, f in explained]}
        results.append({"case": case, "analysis_sha256": hashlib.sha256(raw).hexdigest(), "reference_sha256": hashlib.sha256(reference_raw).hexdigest(), "engine_version": analysis["engine_version"], "reference_span_s": span, "audio_duration_s": analysis["audio"]["duration_s"], "searched_waveform_offset_range_s": [0.0, limit], "reference_first_frequency_hz": ref["points"][0]["frequency_hz"], "reference_last_frequency_hz": ref["points"][-1]["frequency_hz"], "reference_endpoint_drop_hz": ref["points"][0]["frequency_hz"] - ref["points"][-1]["frequency_hz"], "author_identified_aircraft": item["aircraft_registration"], "events": events, "shared_offset_diagnostic": shared})
    report = {"schema_version": 1, "method": {"offset_definition": "reference CSV time zero occurs this many seconds after waveform time zero; fitted, not ground truth", "offset_grid_step_s": OFFSET_STEP, "individual_offset_refinement_step_s": 0.005, "harmonic_ratios": [str(r) for r in RATIOS], "harmonic_ratio_definition": "observed frequency / reference frequency = p/q, 1<=p,q<=6; no continuous frequency scaling", "minimum_overlap_span_s": MIN_SPAN, "minimum_paired_points": MIN_POINTS, "minimum_detected_event_span_overlap_fraction": MIN_EVENT_OVERLAP, "selection_metric": "RMS frequency residual after converting observed points to the reference harmonic", "exploratory_threshold_reference_hz": MAX_RMS, "threshold_is_release_acceptance_criterion": False, "reference_interpolation": "piecewise linear at observed point times; no extrapolation", "shared_offset_selection": "best individual offset of the longest event within the exploratory residual threshold; remaining events evaluated at that fixed offset", "limitations": ["Offsets and harmonic choices fitted on the same published curves being compared", "No verified absolute waveform clock or microphone position", "No false-match null distribution or held-out threshold calibration", "Unexplained events may be other aircraft/harmonics or spurious; labels not established", "Coverage is time-span union, not proof of uniformly dense/continuous detection", "All three cases are development data"]}, "cases": results}
    (ROOT / "detector-comparison.json").write_text(json.dumps(report, indent=2, allow_nan=False) + "\n")
    lines = ["# Automatic detector versus independent published frequency ridges", "", "This is an exploratory fitted curve comparison, with no location or clock-accuracy claim.", "Both waveform offset and discrete harmonic ratio are selected using the reference curve.", "The diagnostic 5 Hz threshold is not a calibrated false-positive or release criterion.", "", "| Case | Events | Shared-offset covered reference | Fitted offset | Duration-weighted RMS at reference harmonic |", "| --- | ---: | ---: | ---: | ---: |"]
    for r in results:
        s = r["shared_offset_diagnostic"]
        rms = f"{s['duration_weighted_rms_reference_hz']:.2f} Hz" if s["duration_weighted_rms_reference_hz"] is not None else "—"
        lines.append(f"| {r['case']} | {len(r['events'])} | {s['reference_covered_union_s']:.2f} / {r['reference_span_s']:.2f} s ({100*s['reference_covered_fraction']:.1f}%) | {s['waveform_offset_s']:.2f} s | {rms} |")
    lines += ["", "Per-event results below fit offsets independently. These intervals must not be added together as shared-clock coverage.", "", "| Case / event | Interval | Ratio observed/reference | Fitted offset | Paired points | Reference-equivalent RMS | Event overlap |", "| --- | --- | --- | ---: | ---: | ---: | ---: |"]
    for r in results:
        for e in r["events"]:
            f = e["best_individual_fit"]
            if f:
                lines.append(f"| {r['case']} / {e['event_id']} | {e['start_s']:.2f}–{e['end_s']:.2f} s | {f['observed_over_reference_ratio']} | {f['waveform_offset_s']:.3f} s | {f['paired_points']} | {f['rms_reference_hz']:.2f} Hz | {100*f['event_span_coverage_fraction']:.1f}% |")
            else:
                lines.append(f"| {r['case']} / {e['event_id']} | {e['start_s']:.2f}–{e['end_s']:.2f} s | insufficient overlap | — | — | — | — |")
    lines += ["", "The author identifies N373DF, N784MB and N788BM respectively; that identity was supplied by the source, not inferred by this comparison. The published endpoint drops are " + ", ".join(f"{r['case']}: {r['reference_endpoint_drop_hz']:.2f} Hz" for r in results) + ".", "", "Fits require at least 8 seconds, 20 paired points and 80% of the detected event's span overlapping the reference. Piecewise-linear interpolation never extrapolates. Ratios p/q with numerator and denominator from 1 to 6 allow different harmonics, but broaden the chance-match search. RMS is reported at the published reference harmonic to avoid favoring low-frequency detections solely through their smaller residual scale.", "", "Events outside the exploratory threshold are unexplained by this source ridge under tested alignments. Their true labels are unknown; some can be background tones, other sources or spurious tracks. Short good fits do not establish recovery of the complete pass. The shared offset is anchored to the longest event whose best individual fit meets the exploratory residual threshold. The other events are evaluated at that fixed offset. Union coverage avoids double-counting overlapping harmonics, but does not establish statistically significant detection. Full numeric results include input hashes, residuals, point counts, interpolation gaps and both individual/shared alignments.", "", "Reproduce with `python corpus/pathak/compare_detector.py` after producing native analyses in `validation-runs/corpus/N*.json`. This reads automatic outputs and reference labels only; it changes no engine code or source recordings."]
    (ROOT / "detector-comparison.md").write_text("\n".join(lines) + "\n")
    for r in results:
        print(r["case"], json.dumps(r["shared_offset_diagnostic"]))


if __name__ == "__main__":
    main()
