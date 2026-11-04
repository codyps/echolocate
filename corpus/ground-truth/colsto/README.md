# Creator-linked original two-flyover recording

Source: colsto's [Unknown location — raw audio](https://www.youtube.com/watch?v=4CxVxuMVkMc), linked by the [original methodology video](https://www.youtube.com/watch?v=UytYrhYLOKI).

`35m00s-53m00s.wav` is one contiguous1080-second excerpt from35:00 through53:00 of the fullhour. It is mono16kHz PCM16, extracted from source AAC itag140. There are no internal cuts, denoising, time warps or inserted tones. Relative event timing remains intact. Set the engine's duration budget to at least1080seconds to analyze both passes; default900seconds truncates the second part.

This is original creator-published audio. No dopplerprint code or assets were accessed or used.

## Primary evidence and limits

Three creator-video frames are retained under `evidence/`:

- **00:30:** datecaption identifies filming on9/8/26 in the continentalUS.
- **25:30:** GoogleMaps shows a search reference at **40.876728,-73.526434**, OysterBay, NewYork.
- **26:08:** following a tree-line comparison, StreetView shows the pole-mounted camera at19BayviewAve, OysterBay. StreetView image dateSeptember2025 is not the recording date.

The displayed map reference is **not a surveyed microphone coordinate**. The separately supplied reconstruction reference40.876466,-73.524951 differs by roughly130meters. It is kept as a secondary reference with unknown uncertainty; it has not been silently promoted to primary ground truth. Reports should label distances to both references, rather than claiming exact microphone error.

The rawvideo's own description is empty. Its upload time2026-11-01 does not establish capture time. The often-used recording start20:17:30UTC on2026-09-08 is a reconstruction claim, not independently verified in the primary material inspected here. If used, report estimates as conditional on that prior. The creator date and NewYorksite instead support a broad local-calendar-day prior; `manifest.json` preserves the corresponding original and crop-shiftedUTC ranges.

A match would reproduce a known published demonstration, not establish a new blind or held-out location result. Precise time and coordinate accuracy remain unverified.

## Reproduction and rights

```bash
python prepare.py --verify
# If yt-dlp is installed in this interpreter:
python prepare.py --download
# Or from a previously acquired source AAC:
python prepare.py --source-m4a /path/to/original.m4a
```

Restore requires ffmpeg. The manifest records the source container and canonical excerpt hashes. The normalized WAV is34,560,044bytes; source licensing beyond standard publisher rights has not been established. Keep the audio and source-video frames out of the default public static package. Flight trajectories acquired independently retain their own redistributable-data terms.
