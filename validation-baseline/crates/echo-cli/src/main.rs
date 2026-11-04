use clap::{Parser, Subcommand};
use echo_core::{model::*, Request};
use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{SystemTime, UNIX_EPOCH},
};
use tiny_http::{Header, Method, Response, Server, StatusCode};
mod ingest;
mod publish;

#[derive(Parser)]
#[command(
    version,
    about = "Independent aircraft Doppler audio analysis, native and Wasm"
)]
struct Args {
    #[command(subcommand)]
    command: Command,
}
#[derive(Subcommand)]
enum Command {
    RerankSpectral {
        audio: PathBuf,
        analysis: PathBuf,
        #[arg(long)]
        package: PathBuf,
        #[arg(long)]
        output: PathBuf,
        #[arg(long, default_value_t = 512)]
        max_cells: usize,
    },
    SearchEvents {
        analysis: PathBuf,
        #[arg(long)]
        package: PathBuf,
        #[arg(long)]
        search_config: Option<PathBuf>,
        #[arg(long)]
        output: PathBuf,
    },
    PublishData {
        #[arg(required = true)]
        inputs: Vec<PathBuf>,
        #[arg(long)]
        output: PathBuf,
        #[arg(long, default_value_t = 262144)]
        shard_bytes: usize,
    },
    Analyze {
        audio: PathBuf,
        #[arg(long)]
        config: Option<PathBuf>,
        #[arg(long = "package")]
        package: Option<PathBuf>,
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    Demo {
        #[arg(short, long, default_value = "demo")]
        output: PathBuf,
    },
    Serve {
        #[arg(long, default_value = "127.0.0.1")]
        host: String,
        #[arg(long, default_value_t = 8765)]
        port: u16,
        #[arg(long, default_value = "web")]
        web: PathBuf,
        #[arg(long, default_value = "validation-runs/jobs")]
        jobs: PathBuf,
    },
    Resume {
        job: PathBuf,
    },
    ValidatePackage {
        package: PathBuf,
    },
    ImportTraces {
        input: PathBuf,
        #[arg(long)]
        output: PathBuf,
        #[arg(long, allow_hyphen_values = true)]
        bounds: String,
        #[arg(long)]
        start_unix_s: Option<f64>,
        #[arg(long)]
        end_unix_s: Option<f64>,
    },
    ImportCsv {
        input: PathBuf,
        #[arg(long)]
        output: PathBuf,
        #[arg(long, allow_hyphen_values = true)]
        bounds: String,
        #[arg(long)]
        source: String,
        #[arg(long)]
        license: String,
        #[arg(long)]
        geoid_undulation_m: Option<f64>,
    },
}
fn main() {
    if let Err(e) = run() {
        eprintln!("Error: {e}");
        std::process::exit(1);
    }
}
fn read_json<T: serde::de::DeserializeOwned>(p: &Path) -> Result<T, String> {
    serde_json::from_slice(&fs::read(p).map_err(|e| e.to_string())?)
        .map_err(|e| format!("{}: {e}", p.display()))
}
fn save(p: &Path, v: &impl serde::Serialize) -> Result<(), String> {
    if let Some(parent) = p.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    fs::write(p, serde_json::to_vec_pretty(v).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())
}
fn run() -> Result<(), String> {
    match Args::parse().command {
        Command::RerankSpectral {
            audio,
            analysis,
            package,
            output,
            max_cells,
        } => {
            use sha2::{Digest, Sha256};
            let bytes = fs::read(&audio).map_err(|e| e.to_string())?;
            let saved: serde_json::Value = read_json(&analysis)?;
            let mut result: echo_core::model::Analysis =
                serde_json::from_value(saved["analysis"].clone()).map_err(|e| e.to_string())?;
            if format!("{:x}", Sha256::digest(&bytes)) != result.input_audio_sha256 {
                return Err("Audio fingerprint differs from cached analysis".into());
            }
            let pack: DataPackage = read_json(&package)?;
            pack.validate()?;
            result.config.spectral_refine = true;
            result.config.spectral_max_cells = max_cells;
            result.config.validate()?;
            let decoded = echo_core::audio::decode(&bytes, result.config.max_audio_s)?;
            let (_, spec, _) = echo_core::audio::spectrogram(&decoded, &result.config)?;
            let input_count = result.heatmap.len();
            let (ranked, heat) = echo_core::spectral::rerank(
                &result.heatmap,
                &spec,
                &pack,
                &result.config,
                &|| false,
            )?;
            result.candidates = ranked;
            result.heatmap = heat;
            result.score_kind = "profiled_spectrogram_contrast_db_not_probability".into();
            result.status = if result.candidates.is_empty() {
                "no_match"
            } else if result.search.as_ref().is_some_and(|s| s.budget_exhausted) {
                "search_incomplete"
            } else {
                "candidates_uncalibrated"
            }
            .into();
            result.warnings.push(format!("Experimental spectral reranking scored {} of {input_count} retained ridge cells; its profiled scores are not probabilities.",input_count.min(max_cells)));
            result.data_source = Some(pack.source);
            result.synthetic = pack.synthetic;
            result.coordinate_frame = pack.coordinate_frame;
            result.clock_domain = pack.clock_domain;
            result.warnings.extend(pack.coverage_notes);
            result.flights = pack.flights;
            save(&output, &serde_json::json!({"ok":true,"analysis":result}))?;
        }
        Command::SearchEvents {
            analysis,
            package,
            search_config,
            output,
        } => {
            let saved: serde_json::Value = read_json(&analysis)?;
            let mut result: echo_core::model::Analysis =
                serde_json::from_value(saved["analysis"].clone()).map_err(|e| e.to_string())?;
            let pack: DataPackage = read_json(&package)?;
            pack.validate()?;
            let mut config = serde_json::to_value(&result.config).map_err(|e| e.to_string())?;
            if let Some(path) = search_config {
                let overrides: serde_json::Value = read_json(&path)?;
                for (key, value) in overrides
                    .as_object()
                    .ok_or("Search overrides must be an object")?
                {
                    let allowed = [
                        "bounds",
                        "time_prior",
                        "grid_spacing_m",
                        "mode_separation_m",
                        "refinement_levels",
                        "max_cells",
                        "max_hypotheses",
                        "clock_step_s",
                        "clock_radius_s",
                        "sound_speed_m_s",
                        "wind_east_m_s",
                        "wind_north_m_s",
                        "wind_up_m_s",
                        "observer_height_ellipsoid_m",
                        "max_range_m",
                        "frequency_sigma_hz",
                        "max_track_gap_s",
                    ];
                    if !allowed.contains(&key.as_str()) {
                        return Err(format!("{key} affects discovery; rerun analyze rather than modifying cached events"));
                    }
                    config[key] = value.clone();
                }
            }
            let mut config: Config = serde_json::from_value(config).map_err(|e| e.to_string())?;
            // This command searches cached ridges; direct spectral scoring needs audio.
            config.spectral_refine = false;
            config.validate()?;
            if result.events.is_empty() {
                return Err("Cached analysis has no usable automatic events".into());
            }
            let (ranked, heat, summary) =
                echo_core::search::locate(&result.events, &pack, &config)?;
            result.status = if summary.budget_exhausted {
                "search_incomplete"
            } else if ranked.is_empty() {
                "no_match"
            } else if ranked[0].matched_events < 2 {
                "ambiguous"
            } else {
                "candidates_uncalibrated"
            }
            .into();
            result.config = config;
            result.score_kind = "profiled_ridge_shape_contrast_not_probability".into();
            result.candidates = ranked;
            result.heatmap = heat;
            result.search = Some(summary);
            result.data_source = Some(pack.source.clone());
            result.synthetic = pack.synthetic;
            result.coordinate_frame = pack.coordinate_frame;
            result.clock_domain = pack.clock_domain;
            result.flights = pack.flights;
            result.warnings.extend(pack.coverage_notes);
            result.warnings.push(format!("Reused automatic acoustic proposals from {} with audio SHA-256 {}. No hand-supplied ridge windows were used.",analysis.display(),result.input_audio_sha256));
            save(&output, &serde_json::json!({"ok":true,"analysis":result}))?;
        }
        Command::PublishData {
            inputs,
            output,
            shard_bytes,
        } => {
            let c = publish::export(&inputs, &output, shard_bytes)?;
            println!(
                "Published {} lossless packages to {}",
                c["packages"].as_array().unwrap().len(),
                output.display()
            );
        }
        Command::Analyze {
            audio,
            config,
            package,
            output,
        } => {
            let request = Request {
                config: config
                    .map(|p| read_json(&p))
                    .transpose()?
                    .unwrap_or_default(),
                package: package.map(|p| read_json(&p)).transpose()?,
            };
            let bytes = fs::read(audio).map_err(|e| e.to_string())?;
            let result = echo_core::analyze(&bytes, &request)?;
            eprintln!(
                "{}: {} events, {} candidate regions",
                result.status,
                result.events.len(),
                result.candidates.len()
            );
            let envelope = serde_json::json!({"ok":true,"analysis":result});
            if let Some(p) = output {
                save(&p, &envelope)?;
            } else {
                println!("{}", serde_json::to_string(&envelope).unwrap());
            }
        }
        Command::Demo { output } => {
            fs::create_dir_all(&output).map_err(|e| e.to_string())?;
            fs::write(output.join("demo.wav"), echo_core::demo::audio())
                .map_err(|e| e.to_string())?;
            save(
                &output.join("demo.package.json"),
                &echo_core::demo::package(),
            )?;
            save(&output.join("demo.config.json"), &echo_core::demo::config())?;
            save(
                &output.join("demo.truth.json"),
                &serde_json::json!({"synthetic":true,"lat":echo_core::demo::LAT,"lon":echo_core::demo::LON,"start_unix_s":echo_core::demo::START}),
            )?;
            println!("Synthetic fixture written to {}", output.display());
        }
        Command::Serve {
            host,
            port,
            web,
            jobs,
        } => serve(&host, port, web, jobs)?,
        Command::Resume { job } => {
            let request: Request = read_json(&job.join("request.json"))?;
            let _ = fs::remove_file(job.join("cancel"));
            let bytes = fs::read(job.join("audio.bin")).map_err(|e| e.to_string())?;
            run_job(&job, &request, &bytes)?;
        }
        Command::ValidatePackage { package } => {
            let p: DataPackage = read_json(&package)?;
            p.validate()?;
            println!("Valid package: {} ({} flights)", p.name, p.flights.len());
        }
        Command::ImportTraces {
            input,
            output,
            bounds,
            start_unix_s,
            end_unix_s,
        } => {
            let p = ingest::readsb(&input, ingest::bounds(&bounds)?, start_unix_s, end_unix_s)?;
            save(&output, &p)?;
            println!(
                "Imported {} flight segments into {}",
                p.flights.len(),
                output.display()
            );
        }
        Command::ImportCsv {
            input,
            output,
            bounds,
            source,
            license,
            geoid_undulation_m,
        } => {
            let p = ingest::csv(
                &input,
                ingest::bounds(&bounds)?,
                source,
                license,
                geoid_undulation_m,
            )?;
            save(&output, &p)?;
            println!(
                "Imported {} flight segments into {}",
                p.flights.len(),
                output.display()
            );
        }
    }
    Ok(())
}
fn header(n: &str, v: &str) -> Header {
    Header::from_bytes(n.as_bytes(), v.as_bytes()).unwrap()
}
fn json_response(v: serde_json::Value, status: u16) -> Response<std::io::Cursor<Vec<u8>>> {
    Response::from_data(serde_json::to_vec(&v).unwrap())
        .with_status_code(StatusCode(status))
        .with_header(header("Content-Type", "application/json"))
}
fn run_job(dir: &Path, request: &Request, bytes: &[u8]) -> Result<(), String> {
    save(
        &dir.join("state.json"),
        &serde_json::json!({"state":"running","stage":"Rust audio analysis and joint search"}),
    )?;
    let result = echo_core::analyze_with_control(bytes, request, &|| dir.join("cancel").exists());
    match result {
        Ok(a) => {
            save(
                &dir.join("result.json"),
                &serde_json::json!({"ok":true,"analysis":a}),
            )?;
            save(
                &dir.join("state.json"),
                &serde_json::json!({"state":"complete"}),
            )?;
        }
        Err(error) => {
            save(
                &dir.join("result.json"),
                &serde_json::json!({"ok":false,"error":error}),
            )?;
            save(
                &dir.join("state.json"),
                &serde_json::json!({"state":if error.contains("cancelled"){ "cancelled" }else{"failed"},"error":error}),
            )?;
        }
    }
    Ok(())
}
fn serve(host: &str, port: u16, web: PathBuf, jobs: PathBuf) -> Result<(), String> {
    fs::create_dir_all(&jobs).map_err(|e| e.to_string())?;
    for dir in fs::read_dir(&jobs).map_err(|e| e.to_string())?.flatten() {
        let p = dir.path().join("state.json");
        if let Ok(v) = read_json::<serde_json::Value>(&p) {
            if v["state"] == "running" {
                save(
                    &p,
                    &serde_json::json!({"state":"interrupted","message":"Resume reruns the saved inputs"}),
                )?;
            }
        }
    }
    let server = Server::http(format!("{host}:{port}")).map_err(|e| e.to_string())?;
    let busy = Arc::new(AtomicBool::new(false));
    println!("Echolocate: http://{host}:{port}");
    for mut req in server.incoming_requests() {
        // Local-only service: reject remote-origin writes and unexpected hostnames (DNS rebinding).
        let expected = format!("{host}:{port}");
        let valid_host = req
            .headers()
            .iter()
            .find(|h| h.field.equiv("Host"))
            .is_some_and(|h| {
                h.value.as_str() == expected
                    || (host == "127.0.0.1" && h.value.as_str() == format!("localhost:{port}"))
            });
        if !valid_host {
            let _ = req.respond(json_response(
                serde_json::json!({"error":"Host rejected"}),
                403,
            ));
            continue;
        }
        if *req.method() == Method::Post
            && req
                .headers()
                .iter()
                .find(|h| h.field.equiv("Origin"))
                .is_some_and(|h| {
                    h.value.as_str() != format!("http://{host}:{port}")
                        && h.value.as_str() != format!("http://localhost:{port}")
                })
        {
            let _ = req.respond(json_response(
                serde_json::json!({"error":"Cross-origin write rejected"}),
                403,
            ));
            continue;
        }
        let url = req.url().split('?').next().unwrap_or("/").to_owned();
        if url == "/api/health" {
            let _=req.respond(json_response(serde_json::json!({"ok":true,"engine":"Rust native","busy":busy.load(Ordering::SeqCst)}),200));
            continue;
        }
        if url == "/api/jobs" && *req.method() == Method::Post {
            if busy.swap(true, Ordering::SeqCst) {
                let _ = req.respond(json_response(
                    serde_json::json!({"error":"One native analysis is already running"}),
                    409,
                ));
                continue;
            }
            let mut body = Vec::new();
            let read = req
                .as_reader()
                .take(80 * 1024 * 1024 + 1)
                .read_to_end(&mut body);
            let parsed = (|| -> Result<(Request, Vec<u8>), String> {
                read.map_err(|e| e.to_string())?;
                if body.len() < 4 || body.len() > 80 * 1024 * 1024 {
                    return Err("Invalid request size".into());
                }
                let n = u32::from_le_bytes(body[..4].try_into().unwrap()) as usize;
                if n > 16 * 1024 * 1024 || n + 4 > body.len() {
                    return Err("Invalid metadata length".into());
                }
                let request: Request =
                    serde_json::from_slice(&body[4..4 + n]).map_err(|e| e.to_string())?;
                request.config.validate()?;
                if let Some(p) = &request.package {
                    p.validate()?;
                }
                Ok((request, body[4 + n..].to_vec()))
            })();
            match parsed {
                Err(error) => {
                    busy.store(false, Ordering::SeqCst);
                    let _ = req.respond(json_response(serde_json::json!({"error":error}), 400));
                }
                Ok((request, bytes)) => {
                    let id = SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap()
                        .as_nanos()
                        .to_string();
                    let dir = jobs.join(&id);
                    let saved = fs::create_dir_all(&dir)
                        .map_err(|e| e.to_string())
                        .and_then(|_| save(&dir.join("request.json"), &request))
                        .and_then(|_| {
                            fs::write(dir.join("audio.bin"), &bytes).map_err(|e| e.to_string())
                        });
                    if let Err(error) = saved {
                        busy.store(false, Ordering::SeqCst);
                        let _ = req.respond(json_response(serde_json::json!({"error":error}), 500));
                        continue;
                    }
                    let _ = save(
                        &dir.join("state.json"),
                        &serde_json::json!({"state":"running"}),
                    );
                    let busy_clone = busy.clone();
                    std::thread::spawn(move || {
                        let _ = run_job(&dir, &request, &bytes);
                        busy_clone.store(false, Ordering::SeqCst);
                    });
                    let _ = req.respond(json_response(
                        serde_json::json!({"id":id,"state":"running"}),
                        202,
                    ));
                }
            }
            continue;
        }
        if let Some(rest) = url.strip_prefix("/api/jobs/") {
            let parts: Vec<_> = rest.split('/').collect();
            let id = parts[0];
            if id.is_empty() || !id.bytes().all(|c| c.is_ascii_digit()) || !jobs.join(id).exists() {
                let _ = req.respond(json_response(
                    serde_json::json!({"error":"Unknown job"}),
                    404,
                ));
                continue;
            }
            let dir = jobs.join(id);
            if parts.get(1) == Some(&"cancel") && *req.method() == Method::Post {
                let _ = fs::write(dir.join("cancel"), b"cancel");
                let _ = req.respond(json_response(
                    serde_json::json!({"state":"cancellation_requested"}),
                    200,
                ));
                continue;
            }
            if parts.get(1) == Some(&"result") {
                match fs::read(dir.join("result.json")) {
                    Ok(data) => {
                        let _ = req.respond(
                            Response::from_data(data)
                                .with_header(header("Content-Type", "application/json")),
                        );
                    }
                    Err(_) => {
                        let _ = req.respond(json_response(
                            serde_json::json!({"error":"Result not ready"}),
                            404,
                        ));
                    }
                }
                continue;
            }
            let state = read_json::<serde_json::Value>(&dir.join("state.json"))
                .unwrap_or(serde_json::json!({"state":"unknown"}));
            let _ = req.respond(json_response(state, 200));
            continue;
        }
        if *req.method() != Method::Get {
            let _ = req.respond(json_response(
                serde_json::json!({"error":"Method not allowed"}),
                405,
            ));
            continue;
        }
        let path = if url == "/" {
            "index.html"
        } else {
            url.trim_start_matches('/')
        };
        if path.contains("..") || path.contains('\\') || path.contains('%') {
            let _ = req.respond(json_response(
                serde_json::json!({"error":"Invalid asset path"}),
                400,
            ));
            continue;
        }
        match fs::read(web.join(path)) {
            Ok(data) => {
                let mime = if path.ends_with(".html") {
                    "text/html"
                } else if path.ends_with(".js") {
                    "text/javascript"
                } else if path.ends_with(".css") {
                    "text/css"
                } else if path.ends_with(".wasm") {
                    "application/wasm"
                } else if path.ends_with(".json") {
                    "application/json"
                } else if path.ends_with(".wav") {
                    "audio/wav"
                } else {
                    "application/octet-stream"
                };
                let _ = req.respond(
                    Response::from_data(data)
                        .with_header(header("Content-Type", mime))
                        .with_header(header("X-Content-Type-Options", "nosniff")),
                );
            }
            Err(_) => {
                let _ = req.respond(json_response(serde_json::json!({"error":"Not found"}), 404));
            }
        }
    }
    Ok(())
}
