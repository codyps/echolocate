# Actual location-estimate validation: DLR RECORD139

These are actual engine estimates from measured audio, compared after inference with source receiver coordinates. No geographic origin or absolute UTC is asserted.

| Protocol | Channel | Status | Top-1 error | Origin-only baseline |
|---|---:|---|---:|---:|
| measured-knots | 0 | no_usable_events | No estimate | — |
| measured-knots | 46 | no_usable_events | No estimate | — |
| measured-knots | 92 | no_usable_events | No estimate | — |
| measured-knots | 138 | no_usable_events | No estimate | — |
| measured-knots | 184 | no_usable_events | No estimate | — |
| measured-knots | 230 | no_usable_events | No estimate | — |
| constant-velocity-extension | 0 | ambiguous | 336.9 m | 0.6 m |
| constant-velocity-extension | 46 | ambiguous | 334.5 m | 2.5 m |
| constant-velocity-extension | 92 | ambiguous | 59.8 m | 1.1 m |
| constant-velocity-extension | 138 | ambiguous | 14.1 m | 3.9 m |
| constant-velocity-extension | 184 | ambiguous | 204.2 m | 6.9 m |
| constant-velocity-extension | 230 | ambiguous | 20.7 m | 19.0 m |

## Interpretation

All six channels share one jet flyover. Source clock convention is explicitly assumed, not independently established. One protocol uses only supplied knots and a fixed common arrival slice; the other extrapolates a constant-velocity source model. Receiver coordinates were excluded from inference.

The origin-only baseline is included because the array is compact. A small absolute error without beating this baseline would not demonstrate useful acoustic localization. A returned uncalibrated candidate is not a validated answer.

Raw outputs, per-candidate errors, source fingerprints, data conventions and search budgets are retained. Missing estimates remain failures/abstentions, not zero error.
