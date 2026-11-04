//! Lossless, content-addressed static data export. No trajectory simplification is performed.
use echo_core::model::DataPackage;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Path, PathBuf},
};

fn write(dir: &Path, bytes: &[u8]) -> Result<Value, String> {
    let digest = format!("{:x}", Sha256::digest(bytes));
    let name = format!("{}.json", &digest[..24]);
    fs::write(dir.join(&name), bytes).map_err(|e| e.to_string())?;
    Ok(json!({"url":name,"bytes":bytes.len(),"sha256":digest}))
}
pub fn export(inputs: &[PathBuf], output: &Path, target_bytes: usize) -> Result<Value, String> {
    if !(1024..=4 * 1024 * 1024).contains(&target_bytes) {
        return Err("shard_bytes must be between 1 KiB and 4 MiB".into());
    }
    fs::create_dir_all(output).map_err(|e| e.to_string())?;
    let mut entries = Vec::new();
    let mut ids = std::collections::HashSet::new();
    for path in inputs {
        let input = fs::read(path).map_err(|e| e.to_string())?;
        if input.len() > 16 * 1024 * 1024 {
            return Err(format!("{} exceeds the browser's 16 MiB package budget; import a smaller region/time window",path.display()));
        }
        let mut package: DataPackage = serde_json::from_slice(&input).map_err(|e| e.to_string())?;
        package.validate()?;
        let id = path
            .file_stem()
            .and_then(|s| s.to_str())
            .ok_or("Package needs a UTF-8 filename")?
            .to_owned();
        if !ids.insert(id.clone()) {
            return Err("Duplicate catalog package ID".into());
        }
        let license_url=match package.license.as_str(){
            "ODbL-1.0"=>"https://opendatacommons.org/licenses/odbl/1-0/",
            "CC-BY-4.0"=>"https://creativecommons.org/licenses/by/4.0/",
            "CC0-1.0"=>"https://creativecommons.org/publicdomain/zero/1.0/",
            "MIT" if package.synthetic=>"https://opensource.org/license/mit",
            other=>return Err(format!("No redistribution rule established for package license {other}; keep it local until documented")),
        };
        let flights = std::mem::take(&mut package.flights);
        let flight_count = flights.len();
        let mut shards = Vec::new();
        let mut batch = Vec::new();
        let mut size = 2;
        for flight in flights {
            let bytes = serde_json::to_vec(&flight).map_err(|e| e.to_string())?;
            if !batch.is_empty() && size + bytes.len() + 1 > target_bytes {
                let mut record = write(
                    output,
                    &serde_json::to_vec(&batch).map_err(|e| e.to_string())?,
                )?;
                record["flights"] = json!(batch.len());
                shards.push(record);
                batch.clear();
                size = 2;
            }
            if bytes.len() > 16 * 1024 * 1024 {
                return Err(
                    "One trajectory exceeds the browser input budget; split it into time segments"
                        .into(),
                );
            }
            size += bytes.len() + 1;
            batch.push(flight);
        }
        if !batch.is_empty() {
            let mut record = write(
                output,
                &serde_json::to_vec(&batch).map_err(|e| e.to_string())?,
            )?;
            record["flights"] = json!(batch.len());
            shards.push(record);
        }
        let metadata = write(
            output,
            &serde_json::to_vec(&package).map_err(|e| e.to_string())?,
        )?;
        let total_bytes = metadata["bytes"].as_u64().unwrap()
            + shards
                .iter()
                .map(|s| s["bytes"].as_u64().unwrap())
                .sum::<u64>();
        entries.push(json!({"id":id,"name":package.name,"synthetic":package.synthetic,"coverage_start_unix_s":package.coverage_start_unix_s,"coverage_end_unix_s":package.coverage_end_unix_s,"search_bounds":package.search_bounds,"source":package.source,"license":package.license,"license_url":license_url,"attribution":format!("Contains information from {}. Data licensed {}. Trajectories are redistributed losslessly in a different JSON layout.",package.source,package.license),"flight_count":flight_count,"transfer_bytes":total_bytes,"metadata":metadata,"shards":shards}));
    }
    let catalog = json!({"schema_version":1,"format":"echolocate-flight-catalog-v1","transformation":"Lossless flight-level JSON sharding; full source coordinate/timestamp precision retained; no geometric simplification or frequency-derived substitute.","packages":entries});
    fs::write(
        output.join("catalog.json"),
        serde_json::to_vec_pretty(&catalog).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    let mut notice=String::from("# Flight data attribution\n\nApplication code is separately licensed MIT. These databases retain their own licenses.\n\n");
    for e in catalog["packages"].as_array().unwrap() {
        notice.push_str(&format!(
            "## {}\n\n{}\n\nLicense: {}\n\nSource: {}\n\n",
            e["name"].as_str().unwrap(),
            e["attribution"].as_str().unwrap(),
            e["license_url"].as_str().unwrap(),
            e["source"].as_str().unwrap()
        ));
    }
    notice.push_str("For ODbL-derived databases, the complete machine-readable derived database is freely available as the metadata and every shard listed in catalog.json. The exact transformation source is crates/echo-cli/src/publish.rs. Preserve attribution and share-alike notices when redistributing. Do not relabel flight data as MIT.\n");
    fs::write(output.join("ATTRIBUTION.md"), notice).map_err(|e| e.to_string())?;
    Ok(catalog)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn export_is_lossless_and_every_asset_is_verified() {
        let root = std::env::temp_dir().join(format!("echo-publish-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        let input = root.join("demo.json");
        let p = echo_core::demo::package();
        fs::write(&input, serde_json::to_vec(&p).unwrap()).unwrap();
        let out = root.join("assets");
        let c = export(&[input], &out, 1024).unwrap();
        let e = &c["packages"][0];
        let mut flights = Vec::new();
        for s in e["shards"].as_array().unwrap() {
            let b = fs::read(out.join(s["url"].as_str().unwrap())).unwrap();
            assert_eq!(
                format!("{:x}", Sha256::digest(&b)),
                s["sha256"].as_str().unwrap()
            );
            let f: Vec<echo_core::model::Flight> = serde_json::from_slice(&b).unwrap();
            flights.extend(f);
        }
        assert!(
            serde_json::to_value(flights).unwrap() == serde_json::to_value(p.flights).unwrap(),
            "Sharding changed trajectory values"
        );
        assert_eq!(e["license"], "MIT");
        fs::remove_dir_all(root).unwrap();
    }
}
