# Separate-recording validation corpus

These are all five previously unused passages from the constant-speed/height category of Praveen Pathak’s [published dataset](https://zenodo.org/records/23424278), CC BY 4.0. The earlier three cases were excluded from this new evaluation. Membership was fixed before running the new signals, and the algorithm/configuration/evaluation protocol were SHA-256 frozen before evaluation.

The full original 23,096,453-byte ZIP was checked against the publisher’s MD5, and source WAV/KML/frequency CSV/PDF files are unchanged. Derived trajectory CSV and source-ridge JSON are labeled. `python prepare.py --download` restores the selected cases; `--verify` checks the manifest offline.

The source does not directly publish surveyed microphone GPS or verified first-WAV-sample UTC. The metadata-only reconstruction in `data/independent-validation/reference.json` is an approximate comparison reference, with an unknown metrological error bound. Supplied KMLs and bounded time priors assist inference; source frequency curves are never supplied to inference.

All five cases were included in the shared-site test with separate clocks and equal weights; none was removed based on its result. See [results and limitations](../../docs/ALGORITHM_IMPROVEMENT.md). These signals have now been exposed during evaluation and cannot serve as a fresh test set for later tuning.
