# Automatic detector versus independent published frequency ridges

This is an exploratory fitted curve comparison, with no location or clock-accuracy claim.
Both waveform offset and discrete harmonic ratio are selected using the reference curve.
The diagnostic 5 Hz threshold is not a calibrated false-positive or release criterion.

| Case | Events | Shared-offset covered reference | Fitted offset | Duration-weighted RMS at reference harmonic |
| --- | ---: | ---: | ---: | ---: |
| N373DF_R06 | 4 | 21.25 / 39.59 s (53.7%) | 3.83 s | 1.68 Hz |
| N784MB_R04 | 6 | 55.00 / 75.23 s (73.1%) | 5.07 s | 1.95 Hz |
| N788BM_R03 | 6 | 18.25 / 58.03 s (31.5%) | 4.99 s | 2.03 Hz |

Per-event results below fit offsets independently. These intervals must not be added together as shared-clock coverage.

| Case / event | Interval | Ratio observed/reference | Fitted offset | Paired points | Reference-equivalent RMS | Event overlap |
| --- | --- | --- | ---: | ---: | ---: | ---: |
| N373DF_R06 / 0 | 14.25–35.50 s | 1/2 | 3.825 s | 74 | 1.68 Hz | 100.0% |
| N373DF_R06 / 1 | 39.25–48.75 s | 3/5 | 7.660 s | 26 | 10.98 Hz | 84.2% |
| N373DF_R06 / 2 | 39.50–48.25 s | 1/2 | 7.910 s | 29 | 8.26 Hz | 91.4% |
| N373DF_R06 / 3 | 40.25–48.75 s | 1/2 | 10.344 s | 26 | 8.95 Hz | 100.0% |
| N784MB_R04 / 0 | 0.25–60.75 s | 1/2 | 5.070 s | 213 | 2.70 Hz | 90.9% |
| N784MB_R04 / 1 | 5.75–60.75 s | 1 | 5.015 s | 202 | 1.44 Hz | 100.0% |
| N784MB_R04 / 2 | 10.00–37.75 s | 2/3 | 5.085 s | 104 | 1.91 Hz | 100.0% |
| N784MB_R04 / 3 | 30.00–48.00 s | 5/4 | 4.945 s | 64 | 0.36 Hz | 100.0% |
| N784MB_R04 / 4 | 31.75–42.00 s | 5/2 | 10.001 s | 35 | 3.58 Hz | 100.0% |
| N784MB_R04 / 5 | 36.75–60.75 s | 2/3 | 5.020 s | 96 | 1.96 Hz | 100.0% |
| N788BM_R03 / 0 | 28.00–44.75 s | 1/2 | 9.530 s | 48 | 5.67 Hz | 100.0% |
| N788BM_R03 / 1 | 29.25–38.75 s | 1 | 8.375 s | 24 | 15.29 Hz | 100.0% |
| N788BM_R03 / 2 | 31.50–42.50 s | 5/3 | 3.235 s | 31 | 4.97 Hz | 100.0% |
| N788BM_R03 / 3 | 32.75–46.50 s | 6/5 | 0.000 s | 43 | 6.74 Hz | 100.0% |
| N788BM_R03 / 4 | 36.25–47.50 s | 3/2 | 9.465 s | 31 | 2.44 Hz | 100.0% |
| N788BM_R03 / 5 | 36.50–54.75 s | 4/5 | 4.990 s | 50 | 2.03 Hz | 100.0% |

The author identifies N373DF, N784MB and N788BM respectively; that identity was supplied by the source, not inferred by this comparison. The published endpoint drops are N373DF_R06: 113.28 Hz, N784MB_R04: 118.28 Hz, N788BM_R03: 134.60 Hz.

Fits require at least 8 seconds, 20 paired points and 80% of the detected event's span overlapping the reference. Piecewise-linear interpolation never extrapolates. Ratios p/q with numerator and denominator from 1 to 6 allow different harmonics, but broaden the chance-match search. RMS is reported at the published reference harmonic to avoid favoring low-frequency detections solely through their smaller residual scale.

Events outside the exploratory threshold are unexplained by this source ridge under tested alignments. Their true labels are unknown; some can be background tones, other sources or spurious tracks. Short good fits do not establish recovery of the complete pass. The shared offset is anchored to the longest event whose best individual fit meets the exploratory residual threshold. The other events are evaluated at that fixed offset. Union coverage avoids double-counting overlapping harmonics, but does not establish statistically significant detection. Full numeric results include input hashes, residuals, point counts, interpolation gaps and both individual/shared alignments.

Reproduce with `python corpus/pathak/compare_detector.py` after producing native analyses in `validation-runs/corpus/N*.json`. This reads automatic outputs and reference labels only; it changes no engine code or source recordings.
