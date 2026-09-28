//! Modpack installation logic for Modrinth .mrpack packages.

use rayon::prelude::*;
use serde::Deserialize;
use std::collections::HashMap;
use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::process::Command;

const HTTP_USER_AGENT: &str = "nite/0.1.0 (https://github.com/thelevnet/nite)";
const LOCK_FILE_NAME: &str = ".nite-modpack-lock";

/// Syncs a declared modpack into the instance directory.
///
/// Reads the `.nite-modpack-lock` file in the instance dir to determine if the
/// current `source` URL/path is already installed. If the source matches the
/// lock, installation is skipped. Otherwise, the modpack is (re-)installed and
/// the lock file is updated.
pub fn sync_modpack(
    source: &str,
    instance_dir: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let lock_path = instance_dir.join(LOCK_FILE_NAME);

    // Check if already installed for this source
    if let Ok(locked) = fs::read_to_string(&lock_path) {
        if locked.trim() == source.trim() {
            println!("[nite] modpack already installed (lock matches), skipping.");
            return Ok(());
        }
        println!("[nite] modpack source changed, reinstalling...");
    }

    install_mrpack(source, instance_dir)?;

    // Write lock file with the source URL/path
    fs::write(&lock_path, source)?;

    Ok(())
}


#[derive(Deserialize, Debug)]
pub struct MrpackIndex {
    #[serde(rename = "formatVersion")]
    pub format_version: u32,
    pub game: String,
    #[serde(rename = "versionId")]
    pub version_id: String,
    pub name: String,
    pub summary: Option<String>,
    pub files: Vec<MrpackFile>,
    pub dependencies: HashMap<String, String>,
}

#[derive(Deserialize, Debug)]
pub struct MrpackFile {
    pub path: String,
    pub downloads: Vec<String>,
    #[serde(default)]
    pub env: Option<MrpackEnv>,
}

#[derive(Deserialize, Debug)]
pub struct MrpackEnv {
    pub client: Option<String>,
}

/// Downloads and extracts a .mrpack (file path or HTTP URL) into an instance directory.
/// Returns (minecraft_version, is_fabric).
pub fn install_mrpack(
    source: &str,
    instance_dir: &Path,
) -> Result<(String, bool), Box<dyn std::error::Error>> {
    let client = reqwest::blocking::Client::builder()
        .tcp_nodelay(true)
        .build()?;

    let xdg = xdg::BaseDirectories::with_prefix("nite");
    let cache_home = xdg.get_cache_home().unwrap_or_else(|| PathBuf::from("/tmp"));
    let temp_pack = cache_home.join("temp_pack.mrpack");
    let extract_dir = cache_home.join("mrpack_extracted");

    if source.starts_with("http://") || source.starts_with("https://") {
        println!("[nite] downloading modpack from {source}...");
        let mut resp = client.get(source).header("User-Agent", HTTP_USER_AGENT).send()?;
        if !resp.status().is_success() {
            return Err(format!("failed to download modpack: HTTP {}", resp.status()).into());
        }
        let mut out = File::create(&temp_pack)?;
        std::io::copy(&mut resp, &mut out)?;
    } else {
        let src_path = Path::new(source);
        if !src_path.exists() {
            return Err(format!("modpack file '{}' does not exist", src_path.display()).into());
        }
        fs::copy(src_path, &temp_pack)?;
    }

    let _ = fs::remove_dir_all(&extract_dir);
    fs::create_dir_all(&extract_dir)?;

    let status = Command::new("unzip")
        .arg("-q")
        .arg("-o")
        .arg(&temp_pack)
        .arg("-d")
        .arg(&extract_dir)
        .status()?;

    if !status.success() {
        return Err(format!("unzip failed on modpack file with status {status}").into());
    }

    let index_file = extract_dir.join("modrinth.index.json");
    if !index_file.exists() {
        return Err("modpack is missing modrinth.index.json".into());
    }

    let index_content = fs::read_to_string(&index_file)?;
    let index: MrpackIndex = serde_json::from_str(&index_content)?;

    let mc_version = index
        .dependencies
        .get("minecraft")
        .cloned()
        .unwrap_or_else(|| "1.21.1".to_string());
    let is_fabric = index.dependencies.contains_key("fabric-loader");

    println!("[nite] installing modpack '{}' (Minecraft {}, Fabric: {})", index.name, mc_version, is_fabric);

    // Copy overrides folder into instance directory if present
    let overrides_dir = extract_dir.join("overrides");
    if overrides_dir.exists() {
        copy_dir_recursive(&overrides_dir, instance_dir)?;
    }
    let client_overrides_dir = extract_dir.join("client-overrides");
    if client_overrides_dir.exists() {
        copy_dir_recursive(&client_overrides_dir, instance_dir)?;
    }

    // Download files specified in index
    let files_to_download: Vec<&MrpackFile> = index
        .files
        .iter()
        .filter(|f| {
            if let Some(ref env) = f.env {
                if let Some(ref c) = env.client {
                    if c == "unsupported" {
                        return false;
                    }
                }
            }
            true
        })
        .collect();

    println!("[nite] downloading {} modpack files...", files_to_download.len());

    files_to_download.par_iter().for_each(|f| {
        let dest = instance_dir.join(&f.path);
        if dest.exists() {
            return;
        }

        if let Some(parent) = dest.parent() {
            let _ = fs::create_dir_all(parent);
        }

        for url in &f.downloads {
            let thread_client = match reqwest::blocking::Client::builder().tcp_nodelay(true).build() {
                Ok(c) => c,
                Err(_) => continue,
            };

            if let Ok(mut resp) = thread_client.get(url).header("User-Agent", HTTP_USER_AGENT).send() {
                if resp.status().is_success() {
                    if let Ok(mut out) = File::create(&dest) {
                        if std::io::copy(&mut resp, &mut out).is_ok() {
                            break;
                        }
                    }
                }
            }
        }
    });

    let _ = fs::remove_dir_all(&extract_dir);
    let _ = fs::remove_file(&temp_pack);

    println!("[nite] modpack '{}' installed successfully", index.name);
    Ok((mc_version, is_fabric))
}

fn copy_dir_recursive(src: &Path, dst: &Path) -> std::io::Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        let target = dst.join(entry.file_name());
        if ty.is_dir() {
            copy_dir_recursive(&entry.path(), &target)?;
        } else {
            let _ = fs::copy(entry.path(), &target);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mrpack_index_deserialization() {
        let manifest_json = r#"{
            "formatVersion": 1,
            "game": "minecraft",
            "versionId": "1.0.0",
            "name": "Test Pack",
            "summary": "A test pack",
            "files": [
                {
                    "path": "mods/sodium.jar",
                    "downloads": ["https://example.com/sodium.jar"],
                    "env": { "client": "required" }
                }
            ],
            "dependencies": {
                "minecraft": "1.21.1",
                "fabric-loader": "0.16.5"
            }
        }"#;

        let index: MrpackIndex = serde_json::from_str(manifest_json).expect("failed to parse mrpack manifest");
        assert_eq!(index.format_version, 1);
        assert_eq!(index.name, "Test Pack");
        assert_eq!(index.dependencies.get("minecraft").unwrap(), "1.21.1");
        assert!(index.dependencies.contains_key("fabric-loader"));
        assert_eq!(index.files.len(), 1);
        assert_eq!(index.files[0].path, "mods/sodium.jar");
    }

    #[test]
    fn test_sync_modpack_skips_when_lock_matches() {
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let temp_dir = std::env::temp_dir().join(format!("nite_modpack_test_{now}"));
        let _ = fs::create_dir_all(&temp_dir);

        let lock_file = temp_dir.join(LOCK_FILE_NAME);
        fs::write(&lock_file, "https://example.com/pack.mrpack").unwrap();

        // Calling sync_modpack with the same URL should return Ok without trying to download anything
        let res = sync_modpack("https://example.com/pack.mrpack", &temp_dir);
        assert!(res.is_ok());

        let _ = fs::remove_dir_all(&temp_dir);
    }
}
