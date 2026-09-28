//! Mojang version manifests, asset indexes, binaries, and vanilla library installations.

use md5::{Digest, Md5};
use rayon::prelude::*;
use serde::Deserialize;
use std::collections::HashMap;
use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::process::Command;
use uuid::Uuid;

const MOJANG_MANIFEST_URL: &str = "https://launchermeta.mojang.com/mc/game/version_manifest_v2.json";
const MOJANG_RESOURCES_URL: &str = "https://resources.download.minecraft.net";

#[derive(Deserialize)]
struct VersionManifest {
    versions: Vec<VersionEntry>,
}

#[derive(Deserialize)]
struct VersionEntry {
    id: String,
    url: String,
}

#[derive(Deserialize)]
struct AssetIndexReference {
    id: String,
    url: String,
}

#[derive(Deserialize)]
struct AssetObject {
    hash: String,
}

#[derive(Deserialize)]
struct AssetIndex {
    objects: HashMap<String, AssetObject>,
}

pub fn generate_offline_uuid(username: &str) -> Uuid {
    let mut hasher = Md5::new();
    hasher.update(format!("OfflinePlayer:{username}").as_bytes());
    let mut hash: [u8; 16] = hasher.finalize().into();
    // RFC 4122 UUID v3 (name-based MD5) variant & version bits
    hash[6] = (hash[6] & 0x0f) | 0x30; // Version 3
    hash[8] = (hash[8] & 0x3f) | 0x80; // IETF variant
    uuid::Builder::from_bytes(hash).into_uuid()
}

pub fn fetch_version_metadata(
    client: &reqwest::blocking::Client,
    version_json: &Path,
    version_id: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    if version_json.exists() {
        return Ok(());
    }

    let manifest: VersionManifest = client.get(MOJANG_MANIFEST_URL).send()?.json()?;
    let entry = manifest
        .versions
        .into_iter()
        .find(|v| v.id == version_id)
        .ok_or_else(|| format!("Minecraft version '{version_id}' not found in Mojang manifest"))?;

    let mut response = client.get(&entry.url).send()?;
    if let Some(parent) = version_json.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut out = File::create(version_json)?;
    std::io::copy(&mut response, &mut out)?;

    Ok(())
}

pub fn fetch_client_binary(
    client: &reqwest::blocking::Client,
    version_json: &Path,
    client_jar: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    if client_jar.exists() {
        return Ok(());
    }

    let file = File::open(version_json)?;
    let root: serde_json::Value = serde_json::from_reader(file)?;
    let url = root["downloads"]["client"]["url"]
        .as_str()
        .ok_or("Missing downloads.client.url in version metadata")?;

    let mut response = client.get(url).send()?;
    if let Some(parent) = client_jar.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut out = File::create(client_jar)?;
    std::io::copy(&mut response, &mut out)?;

    Ok(())
}

fn is_library_supported(lib: &serde_json::Value) -> bool {
    let rules = match lib.get("rules").and_then(|r| r.as_array()) {
        Some(rules) => rules,
        None => return true,
    };

    let mut allowed = false;
    for rule in rules {
        let is_allow = rule.get("action").and_then(|a| a.as_str()) == Some("allow");
        if let Some(os) = rule.get("os") {
            if os.get("name").and_then(|n| n.as_str()) == Some("linux") {
                allowed = is_allow;
            }
        } else {
            allowed = is_allow;
        }
    }
    allowed
}

fn extract_natives_archive(archive_path: &Path, destination: &Path) {
    let _ = Command::new("unzip")
        .arg("-q")
        .arg("-o")
        .arg("-j")
        .arg(archive_path)
        .arg("*.so")
        .arg("-d")
        .arg(destination)
        .status();
}

pub fn install_vanilla_libraries(
    client: &reqwest::blocking::Client,
    version_json: &Path,
    libs_dir: &Path,
    natives_dir: &Path,
) -> Result<Vec<PathBuf>, Box<dyn std::error::Error>> {
    let file = File::open(version_json)?;
    let root: serde_json::Value = serde_json::from_reader(file)?;

    let libraries = root["libraries"]
        .as_array()
        .ok_or("Malformed libraries entry in version metadata")?;

    let mut classpath = Vec::new();

    for lib in libraries {
        if !is_library_supported(lib) {
            continue;
        }

        let mut targets: Vec<(&str, &str, bool)> = Vec::new();

        if let Some(artifact) = lib.pointer("/downloads/artifact") {
            if let (Some(url), Some(path)) = (
                artifact.get("url").and_then(|u| u.as_str()),
                artifact.get("path").and_then(|p| p.as_str()),
            ) {
                let is_native = path.contains("natives") || path.contains("-linux");
                targets.push((url, path, is_native));
            }
        }

        if let Some(classifier_key) = lib.pointer("/natives/linux").and_then(|k| k.as_str()) {
            let pointer = format!("/downloads/classifiers/{classifier_key}");
            if let Some(classifier) = lib.pointer(&pointer) {
                if let (Some(url), Some(path)) = (
                    classifier.get("url").and_then(|u| u.as_str()),
                    classifier.get("path").and_then(|p| p.as_str()),
                ) {
                    targets.push((url, path, true));
                }
            }
        }

        for (url, relative_path, is_native) in targets {
            let target_file = libs_dir.join(relative_path);

            if !target_file.exists() {
                if let Some(parent) = target_file.parent() {
                    fs::create_dir_all(parent)?;
                }
                let filename = target_file.file_name().unwrap_or_default().to_string_lossy();
                println!("[nite] downloading library: {filename}");

                let mut response = client.get(url).send()?;
                let mut out = File::create(&target_file)?;
                std::io::copy(&mut response, &mut out)?;

                if is_native {
                    fs::create_dir_all(natives_dir)?;
                    extract_natives_archive(&target_file, natives_dir);
                }
            } else if is_native {
                let has_shared_libraries = fs::read_dir(natives_dir)
                    .map(|mut d| {
                        d.any(|entry| {
                            entry
                                .map(|e| e.path().extension().is_some_and(|ext| ext == "so"))
                                .unwrap_or(false)
                        })
                    })
                    .unwrap_or(false);

                if !has_shared_libraries {
                    fs::create_dir_all(natives_dir)?;
                    extract_natives_archive(&target_file, natives_dir);
                }
            }

            classpath.push(target_file);
        }
    }

    Ok(classpath)
}

pub fn sync_vanilla_assets(
    client: &reqwest::blocking::Client,
    version_json: &Path,
    assets_dir: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let file = File::open(version_json)?;
    let root: serde_json::Value = serde_json::from_reader(file)?;

    let index_info: AssetIndexReference = serde_json::from_value(root["assetIndex"].clone())?;
    let indexes_dir = assets_dir.join("indexes");
    let index_file = indexes_dir.join(format!("{}.json", index_info.id));

    if !index_file.exists() {
        println!("[nite] fetching asset index {}", index_info.id);
        fs::create_dir_all(&indexes_dir)?;
        let mut response = client.get(&index_info.url).send()?;
        let mut out = File::create(&index_file)?;
        std::io::copy(&mut response, &mut out)?;
    }

    println!("[nite] verifying game assets");
    let index_data = File::open(&index_file)?;
    let index: AssetIndex = serde_json::from_reader(index_data)?;

    let objects_dir = assets_dir.join("objects");
    let objects: Vec<&AssetObject> = index.objects.values().collect();

    objects.par_iter().for_each(|object| {
        let hash = &object.hash;
        if hash.len() < 2 {
            return;
        }

        let prefix = &hash[..2];
        let target_dir = objects_dir.join(prefix);
        let target_file = target_dir.join(hash);

        if !target_file.exists() {
            let _ = fs::create_dir_all(&target_dir);
            let url = format!("{MOJANG_RESOURCES_URL}/{prefix}/{hash}");
            if let Ok(mut res) = client.get(&url).send() {
                if res.status().is_success() {
                    if let Ok(mut out) = File::create(&target_file) {
                        let _ = std::io::copy(&mut res, &mut out);
                    }
                }
            }
        }
    });

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_offline_uuid_deterministic() {
        let uuid1 = generate_offline_uuid("Steve");
        let uuid2 = generate_offline_uuid("Steve");
        assert_eq!(uuid1, uuid2);

        let alex = generate_offline_uuid("Alex");
        assert_ne!(uuid1, alex);

        // Standard Minecraft offline UUID for "OfflinePlayer:Steve"
        assert_eq!(uuid1.to_string(), "5627dd98-e6be-3c21-b8a8-e92344183641");
    }
}
