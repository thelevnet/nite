//! Fabric meta and loader installation.

use serde::Deserialize;
use std::fs::{self, File};
use std::path::{Path, PathBuf};

const FABRIC_META_URL: &str = "https://meta.fabricmc.net/v2/versions/loader";
const HTTP_USER_AGENT: &str = "nite/0.1.0 (https://github.com/thelevnet/nite)";

#[derive(Deserialize)]
struct FabricLoaderEntry {
    loader: FabricLoaderVersion,
}

#[derive(Deserialize)]
struct FabricLoaderVersion {
    version: String,
}

fn maven_coordinate_to_path(coordinate: &str) -> Option<String> {
    let parts: Vec<&str> = coordinate.split(':').collect();
    if parts.len() < 3 {
        return None;
    }
    let group = parts[0].replace('.', "/");
    let artifact = parts[1];
    let version = parts[2];
    Some(format!("{group}/{artifact}/{version}/{artifact}-{version}.jar"))
}

pub fn install_fabric_loader(
    client: &reqwest::blocking::Client,
    mc_version: &str,
    libs_dir: &Path,
) -> Result<(String, Vec<String>, Vec<PathBuf>), Box<dyn std::error::Error>> {
    let cached_profile_path = libs_dir
        .parent()
        .map(|p| p.join("versions").join(mc_version).join("fabric_profile.json"))
        .unwrap_or_else(|| libs_dir.join("fabric_profile.json"));

    let profile: serde_json::Value = match (|| -> Result<serde_json::Value, Box<dyn std::error::Error>> {
        let meta_url = format!("{FABRIC_META_URL}/{mc_version}");
        let entries: Vec<FabricLoaderEntry> = client
            .get(&meta_url)
            .header("User-Agent", HTTP_USER_AGENT)
            .send()?
            .json()?;

        let loader_version = entries
            .first()
            .map(|e| e.loader.version.as_str())
            .ok_or_else(|| format!("No compatible Fabric loader found for {mc_version}"))?;

        println!("[nite] using Fabric loader {loader_version}");
        let profile_url = format!("{FABRIC_META_URL}/{mc_version}/{loader_version}/profile/json");
        let fetched_profile: serde_json::Value = client
            .get(&profile_url)
            .header("User-Agent", HTTP_USER_AGENT)
            .send()?
            .json()?;

        if let Some(parent) = cached_profile_path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let _ = fs::write(&cached_profile_path, serde_json::to_string_pretty(&fetched_profile)?);
        Ok(fetched_profile)
    })() {
        Ok(p) => p,
        Err(err) => {
            if cached_profile_path.exists() {
                println!("[nite] offline mode: using cached Fabric profile");
                let file = File::open(&cached_profile_path)?;
                serde_json::from_reader(file)?
            } else {
                return Err(err);
            }
        }
    };

    let main_class = profile["mainClass"]
        .as_str()
        .unwrap_or("net.fabricmc.loader.impl.launch.knot.KnotClient")
        .to_string();

    let mut jvm_args = Vec::new();
    if let Some(args) = profile.pointer("/arguments/jvm").and_then(|j| j.as_array()) {
        for arg in args {
            if let Some(s) = arg.as_str() {
                jvm_args.push(s.to_string());
            }
        }
    }

    let mut libraries = Vec::new();
    if let Some(libs) = profile["libraries"].as_array() {
        for lib in libs {
            let coordinate = match lib["name"].as_str() {
                Some(name) => name,
                None => continue,
            };
            let relative_path = match maven_coordinate_to_path(coordinate) {
                Some(p) => p,
                None => continue,
            };

            let base_url = lib["url"].as_str().unwrap_or("https://maven.fabricmc.net/");
            let separator = if base_url.ends_with('/') { "" } else { "/" };
            let download_url = format!("{base_url}{separator}{relative_path}");
            let target_file = libs_dir.join(&relative_path);

            if !target_file.exists() {
                if let Some(parent) = target_file.parent() {
                    fs::create_dir_all(parent)?;
                }
                println!("[nite] downloading Fabric dependency: {coordinate}");
                let mut resp = client.get(&download_url).send()?;
                let mut out = File::create(&target_file)?;
                std::io::copy(&mut resp, &mut out)?;
            }
            libraries.push(target_file);
        }
    }

    Ok((main_class, jvm_args, libraries))
}
