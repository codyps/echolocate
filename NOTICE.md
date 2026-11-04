# Attribution and implementation boundary

Echolocate code was written independently for this project. It contains no code copied from or dependency on dopplerprint, dopplertrack or DOPPLERGEIST. Those projects and the following works informed the problem framing and initial research plan.

- colsto, [original methodology video](https://www.youtube.com/watch?v=UytYrhYLOKI): matching aircraft Doppler curves with historical flight trajectories.
- Szczepanik, [Doppler localization survey](https://arxiv.org/abs/2604.14413): inference, ambiguity and practical impairments.
- Riabko et al., [acoustic Doppler localization with retardation correction](https://acnsci.org/journal/index.php/jec/article/view/1201): propagation-delay modeling and identifiability limitations.
- Yu et al., [aircraft-radiated sound parameter measurement](https://www.frontiersin.org/journals/marine-science/articles/10.3389/fmars.2022.996493/full).
- [readsb JSON documentation](https://github.com/wiedehopf/readsb/blob/dev/README-json.md): external trace interchange format, not vendored implementation code.
- Timo Schumacher and Henri Siller, [DLR RECORD139](https://zenodo.org/records/20494053), CC BY 4.0: measured flyover audio, microphone coordinates and source trajectory metadata.
- [AeroSonicDB v1.1.2](https://zenodo.org/records/10215080), CC BY-NC 4.0: local-only propeller recordings; deliberately displaced public coordinates are not microphone truth.
- [adsb.lol historical data](https://www.adsb.lol/docs/open-data/historical), ODbL 1.0.
- Praveen Pathak, [Aeroplane data full analysis and raw files](https://zenodo.org/records/23424278), CC BY 4.0: original audio, published ridge measurements, tracks and reports. Detailed attribution accompanies the corpus.

RustFFT, Symphonia, Hound, Serde, SHA-2, Clap, tiny_http, flate2 and csv retain their upstream licenses. Cargo.lock pins the selected dependency graph. Corpus items have independent rights and provenance in their manifests. Publisher-owned news excerpts are local research inputs and are excluded from the static site.
