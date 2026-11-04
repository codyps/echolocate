# Hosting real flight data with the static application

The shipped catalog contains actual historical regional data, not synthetic flight paths. Five regional windows and two explicitly labeled local-frame research trajectories are exported as seven logical packages. The regional source traces come from adsb.lol archives; the local-frame source is DLR RECORD139. See the catalog and acquisition manifests for exact counts and coverage.

## Format and integrity

`web/data/catalog.json` lists package coverage, aircraft counts, source attribution, license URI, metadata and content-addressed JSON shards. Metadata retains coordinate-frame and clock-domain labels; DLR internal seconds are not relabeled as UTC. Every downloaded metadata/shard file has a declared byte size and SHA-256. The browser checks both before accepting data. Asset URLs resolve relative to the catalog, including GitHub Pages project subdirectories.

Sharding is lossless after the documented import cleaning: timestamps and geometric coordinates are not rounded, downsampled or approximated for hosting. Rust's JSON float-roundtrip support is enabled and tested. Raw-source altitude exclusions, invalid points, conflicting duplicate timestamps, and observation gaps remain documented in each package. The exporter does not claim complete receiver coverage or verified audibility.

Current export target is 256 KiB per shard. Package reconstruction is limited to 16 MiB. A larger corpus should be divided into more geographic/time windows; changing compression must not silently alter the geometry. The full raw archive is unnecessary for browser inference once a finite regional package has been prepared.

```bash
target/release/echo-locate publish-data \
  data/flight-packs/*.json \
  data/location-validation/dlr/*.package.json \
  --output web/data
bash scripts/build.sh
python scripts/check_static_data.py dist
```

The complete resulting `dist/` is a static HTTP deployment. Its hosted-data picker downloads the selected window; audio stays in the browser. HTTPS or localhost is required for Web Crypto integrity checks. Same-origin catalog/shard hosting avoids CORS problems. The default build includes no collected news clips, original creator audio, screenshots, or non-commercial AeroSonicDB recordings.

## GitHub Pages

The public repository is [codyps/echolocate](https://github.com/codyps/echolocate), with the application at [codyps.github.io/echolocate](https://codyps.github.io/echolocate/). GitHub Pages uses **GitHub Actions** as its publishing source. `.github/workflows/ci.yml` runs on pushes to `main`, pull requests, and manual dispatch. It checks public-source boundaries, tests Rust and native/Wasm parity, verifies actual data hashes and budgets, tests the native service, and exercises Chromium including a project subdirectory. Only successful `main` runs upload a Pages artifact and deploy it. Pull requests have read-only repository permissions and cannot deploy. No extra repository secrets are needed.

For another repository, enable **Settings → Pages → Source: GitHub Actions**. All app/data URLs are relative, so the same build works at `owner.github.io/repository/`. Keep the build and deployment in the same workflow so deployment depends on that run’s completed verification job.

GitHub documents a 1 GB published Pages site limit and a soft 100 GB/month bandwidth limit. Regular Git files are limited to 100 MiB, and Git LFS is not supported for Pages assets. Our checked deployment is roughly 10 MB, with the flight database approximately 5.6 MB and every shard below 256 KiB. The build verifies actual sizes; the per-file and complete-site budgets are comfortably below the hosting limits. Sources: [Pages limits](https://docs.github.com/en/pages/getting-started-with-github-pages/github-pages-limits), [large files](https://docs.github.com/en/repositories/working-with-files/managing-large-files/about-large-files-on-github), [Git LFS limitations](https://docs.github.com/en/repositories/working-with-files/managing-large-files/about-git-large-file-storage).

Use ordinary versioned data files for these small windows. For a multi-year or global collection, keep only the catalog and common windows on Pages, and put versioned windows on an HTTPS object host/CDN with its catalog and shards on the same origin (or extend the client with an explicit CORS policy). Avoid placing multi-gigabyte raw day archives into Git history. GitHub Releases can archive acquisition artifacts, but browser data delivery still needs verified CORS behavior; no direct Release-download browser compatibility is assumed.

## Redistribution terms

The actual source archive contains `LICENSE-ODbL.txt`; its verified acquisition receipt is under `corpus/news-tracks/license-receipt.json`. The file is included in `web/data/`. adsb.lol also identifies the historical archive as ODbL 1.0 in its [historical-data documentation](https://www.adsb.lol/docs/open-data/historical).

Our regional extraction/reformatting is distributed as an ODbL-derived database. Preserve the source attribution and license links, distribute the derived database under ODbL, and keep the complete machine-readable derived data available. Here, every file necessary to reconstruct each derivative is already publicly distributable alongside the catalog; the transformation code is included at `crates/echo-cli/src/ingest.rs` and `publish.rs`. `ATTRIBUTION.md` identifies each source, license and transformation. This follows the source's [ODbL conditions](https://opendatacommons.org/licenses/odbl/1-0/), including notices, share-alike and access to derivatives. The MIT license for the application does not replace the data license.

DLR trajectories retain CC BY 4.0 and creator attribution. AeroSonicDB's CC BY-NC audio and publisher-owned news/creator excerpts remain local research inputs, excluded from the public static assets. The exporter's license allowlist is a technical guard; the source receipts and actual source terms establish the redistribution basis.

## Packaged handoff

`echolocate-static.zip` contains the deployable site; `echolocate-flight-data.zip` contains just the same public `data/` catalog and its shards. `echolocate-project.zip` contains the implementation, portable Linux executable, licensed raw flight traces, source receipts and validation results. Publisher-owned news/creator audio, creator screenshots, AeroSonicDB non-commercial audio, raw regional replays and build caches are excluded from that public project archive. Collected inputs remain in the local workspace. Source manifests and acquisition tools preserve their provenance.

Recreate the handoff with `python scripts/package.py` after the native, portable and Wasm builds. `artifact-manifest.json` beside the archives records exact sizes and SHA-256 digests.
