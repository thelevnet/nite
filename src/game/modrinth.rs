//! Modrinth mod and pack synchronization logic.

use crate::config::PackageEntry;
use serde::Deserialize;
use std::collections::HashSet;
use std::fs::{self, File};
use std::path::Path;

const MODRINTH_API_URL: &str = "https://api.modrinth.com/v2";
const HTTP_USER_AGENT: &str = "nite/0.1.0 (https://github.com/thelevnet/nite)";

#[derive(Deserialize, Clone, Debug)]
pub struct ModrinthVersion {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub project_id: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub version_number: Option<String>,
    pub files: Vec<ModrinthFile>,
    #[serde(default)]
    pub dependencies: Vec<ModrinthDependency>,
}

/// Helper to find a matching version from a list of Modrinth versions.
/// Checks exact version_number/id, substring match in version_number, then substring in name.
pub fn find_matching_version<'a>(versions: &'a [ModrinthVersion], req_ver: &str) -> Option<&'a ModrinthVersion> {
    if let Some(v) = versions.iter().find(|v| {
        v.version_number.as_deref() == Some(req_ver) || v.id.as_deref() == Some(req_ver)
    }) {
        return Some(v);
    }
    if let Some(v) = versions.iter().find(|v| {
        v.version_number.as_deref().map_or(false, |vn| vn.contains(req_ver))
    }) {
        return Some(v);
    }
    if let Some(v) = versions.iter().find(|v| {
        v.name.as_deref().map_or(false, |vn| vn.contains(req_ver))
    }) {
        return Some(v);
    }
    None
}

#[derive(Deserialize, Clone, Debug)]
pub struct ModrinthDependency {
    pub version_id: Option<String>,
    pub project_id: Option<String>,
    pub dependency_type: String,
}

#[derive(Deserialize, Clone, Debug)]
pub struct ModrinthFile {
    pub url: String,
    pub filename: String,
    pub primary: Option<bool>,
}

#[derive(Deserialize, Debug)]
pub struct ModSearchResult {
    pub slug: String,
    pub title: String,
    pub description: String,
    pub author: Option<String>,
    #[serde(default)]
    pub versions: Vec<String>,
    #[serde(default)]
    pub categories: Vec<String>,
}

#[derive(Deserialize, Debug)]
struct SearchResponse {
    hits: Vec<ModSearchResult>,
}

/// Queries Modrinth search API for mods matching the given query string.
pub fn search_mods(query: &str) -> Result<Vec<ModSearchResult>, Box<dyn std::error::Error>> {
    let client = reqwest::blocking::Client::builder()
        .tcp_nodelay(true)
        .build()?;
    let encoded_query = query.replace(' ', "%20");
    let url = format!("{MODRINTH_API_URL}/search?query={encoded_query}&facets=%5B%5B%22project_type:mod%22%5D%5D&limit=15");
    let resp = client
        .get(&url)
        .header("User-Agent", HTTP_USER_AGENT)
        .send()?;

    if !resp.status().is_success() {
        return Err(format!("Modrinth API search error: HTTP {}", resp.status()).into());
    }

    let search_res: SearchResponse = resp.json()?;
    Ok(search_res.hits)
}

/// Resolves an item slug (mod, resourcepack, shaderpack) and optional specific version number.
pub fn resolve_item_version(
    slug: &str,
    is_mod: bool,
    target_mc_version: Option<&str>,
    requested_version: Option<&str>,
) -> Result<String, Box<dyn std::error::Error>> {
    let client = reqwest::blocking::Client::builder()
        .tcp_nodelay(true)
        .build()?;

    let url = if is_mod {
        if let Some(mc_ver) = target_mc_version {
            format!("{MODRINTH_API_URL}/project/{slug}/version?loaders=%5B%22fabric%22%5D&game_versions=%5B%22{mc_ver}%22%5D")
        } else {
            format!("{MODRINTH_API_URL}/project/{slug}/version")
        }
    } else if let Some(mc_ver) = target_mc_version {
        format!("{MODRINTH_API_URL}/project/{slug}/version?game_versions=%5B%22{mc_ver}%22%5D")
    } else {
        format!("{MODRINTH_API_URL}/project/{slug}/version")
    };

    let resp = client.get(&url).header("User-Agent", HTTP_USER_AGENT).send()?;

    let mut versions: Vec<ModrinthVersion> = if resp.status().is_success() {
        resp.json().unwrap_or_default()
    } else {
        Vec::new()
    };

    // For resourcepacks and shaderpacks, allow fallback if game_versions filter returned nothing.
    // Never fall back across Minecraft versions for mods!
    if versions.is_empty() && (!is_mod || target_mc_version.is_none()) {
        let fallback_url = format!("{MODRINTH_API_URL}/project/{slug}/version");
        let fallback_resp = client
            .get(&fallback_url)
            .header("User-Agent", HTTP_USER_AGENT)
            .send()?;
        if fallback_resp.status().is_success() {
            versions = fallback_resp.json().unwrap_or_default();
        }
    }

    if versions.is_empty() {
        if let Some(mc_ver) = target_mc_version {
            return Err(format!("item '{slug}' has no compatible releases for Minecraft {mc_ver}").into());
        } else {
            return Err(format!("item '{slug}' was not found on Modrinth or has no releases").into());
        }
    }

    if let Some(req_ver) = requested_version {
        let matched = find_matching_version(&versions, req_ver);
        if let Some(v) = matched {
            Ok(v.version_number.clone().unwrap_or_else(|| req_ver.to_string()))
        } else {
            Err(format!(
                "'{slug}' does not have version '{req_ver}'. Available versions include: {}",
                versions
                    .iter()
                    .filter_map(|v| v.version_number.as_deref())
                    .take(5)
                    .collect::<Vec<_>>()
                    .join(", ")
            ).into())
        }
    } else {
        let latest = versions.first().and_then(|v| v.version_number.clone())
            .unwrap_or_else(|| "latest".to_string());
        Ok(latest)
    }
}

/// Checks all packages in an instance and returns list of (name, current_or_pinned, latest_available).
pub fn check_package_updates(
    packages: &[PackageEntry],
    is_mod: bool,
    mc_version: &str,
) -> Vec<(String, String, String, bool)> {
    let client = match reqwest::blocking::Client::builder().tcp_nodelay(true).build() {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };

    let mut results = Vec::new();

    for pkg in packages {
        let url = if is_mod {
            format!("{MODRINTH_API_URL}/project/{}/version?loaders=%5B%22fabric%22%5D&game_versions=%5B%22{mc_version}%22%5D", pkg.name)
        } else {
            format!("{MODRINTH_API_URL}/project/{}/version?game_versions=%5B%22{mc_version}%22%5D", pkg.name)
        };

        let mut versions: Vec<ModrinthVersion> = client
            .get(&url)
            .header("User-Agent", HTTP_USER_AGENT)
            .send()
            .ok()
            .and_then(|r| r.json().ok())
            .unwrap_or_default();

        if versions.is_empty() && !is_mod {
            let fallback_url = format!("{MODRINTH_API_URL}/project/{}/version", pkg.name);
            versions = client
                .get(&fallback_url)
                .header("User-Agent", HTTP_USER_AGENT)
                .send()
                .ok()
                .and_then(|r| r.json().ok())
                .unwrap_or_default();
        }

        let latest = versions.first().and_then(|v| v.version_number.clone()).unwrap_or_else(|| "unknown".to_string());
        let current = pkg.version.clone().unwrap_or_else(|| "latest".to_string());
        let is_update = if let Some(ref pinned) = pkg.version {
            pinned != &latest
        } else {
            false
        };

        results.push((pkg.name.clone(), current, latest, is_update));
    }

    results
}

fn sync_mod_tree_recursive(
    client: &reqwest::blocking::Client,
    version_data: &ModrinthVersion,
    mods_dir: &Path,
    mc_version: &str,
    tracked_files: &mut HashSet<String>,
    visited_projects: &mut HashSet<String>,
    visited_versions: &mut HashSet<String>,
) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(ref pid) = version_data.project_id {
        visited_projects.insert(pid.clone());
    }
    if let Some(ref vid) = version_data.id {
        visited_versions.insert(vid.clone());
    }

    let primary_file = version_data
        .files
        .iter()
        .find(|f| f.primary == Some(true))
        .or_else(|| version_data.files.first());

    if let Some(f) = primary_file {
        tracked_files.insert(f.filename.clone());
        let target_path = mods_dir.join(&f.filename);

        if !target_path.exists() {
            println!("[nite] downloading mod: {}", f.filename);
            let mut response = client.get(&f.url).send()
                .map_err(|e| format!("failed to request download for mod '{}': {e}", f.filename))?;
            if !response.status().is_success() {
                return Err(format!("failed to download mod '{}': HTTP {}", f.filename, response.status()).into());
            }
            let mut out = File::create(&target_path)
                .map_err(|e| format!("failed to create file for mod '{}': {e}", target_path.display()))?;
            std::io::copy(&mut response, &mut out)
                .map_err(|e| format!("failed to write mod file '{}': {e}", target_path.display()))?;
        }
    }

    for dependency in &version_data.dependencies {
        if dependency.dependency_type != "required" {
            continue;
        }

        if let Some(ref pid) = dependency.project_id {
            if visited_projects.contains(pid) {
                continue;
            }
        }

        if let Some(ref vid) = dependency.version_id {
            if visited_versions.insert(vid.clone()) {
                if let Some(ref pid) = dependency.project_id {
                    visited_projects.insert(pid.clone());
                }
                let url = format!("{MODRINTH_API_URL}/version/{vid}");
                if let Ok(resp) = client.get(&url).header("User-Agent", HTTP_USER_AGENT).send() {
                    if resp.status().is_success() {
                        if let Ok(dep_version) = resp.json::<ModrinthVersion>() {
                            if let Some(ref pid) = dep_version.project_id {
                                visited_projects.insert(pid.clone());
                            }
                            let _ = sync_mod_tree_recursive(
                                client,
                                &dep_version,
                                mods_dir,
                                mc_version,
                                tracked_files,
                                visited_projects,
                                visited_versions,
                            );
                        }
                    }
                }
            }
        } else if let Some(ref pid) = dependency.project_id {
            if visited_projects.insert(pid.clone()) {
                let url = format!(
                    "{MODRINTH_API_URL}/project/{pid}/version?loaders=%5B%22fabric%22%5D&game_versions=%5B%22{mc_version}%22%5D"
                );
                if let Ok(resp) = client.get(&url).header("User-Agent", HTTP_USER_AGENT).send() {
                    if resp.status().is_success() {
                        if let Ok(versions) = resp.json::<Vec<ModrinthVersion>>() {
                            if let Some(first) = versions.first() {
                                let _ = sync_mod_tree_recursive(
                                    client,
                                    first,
                                    mods_dir,
                                    mc_version,
                                    tracked_files,
                                    visited_projects,
                                    visited_versions,
                                );
                            }
                        }
                    }
                }
            }
        }
    }

    Ok(())
}

pub fn sync_mods_directory(
    client: &reqwest::blocking::Client,
    entries: &[PackageEntry],
    mc_version: &str,
    mods_dir: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    fs::create_dir_all(mods_dir)?;

    let mut tracked_files = HashSet::new();
    let mut visited_projects = HashSet::new();
    let mut visited_versions = HashSet::new();

    for entry in entries {
        if visited_projects.contains(&entry.name) {
            continue;
        }

        let url = format!(
            "{MODRINTH_API_URL}/project/{}/version?loaders=%5B%22fabric%22%5D&game_versions=%5B%22{mc_version}%22%5D",
            entry.name
        );
        let resp = client.get(&url).header("User-Agent", HTTP_USER_AGENT).send()?;

        let versions: Vec<ModrinthVersion> = if resp.status().is_success() {
            resp.json().unwrap_or_default()
        } else {
            Vec::new()
        };

        if versions.is_empty() {
            eprintln!("[nite] warning: no compatible version found for mod '{}' on Minecraft {}", entry.name, mc_version);
            continue;
        }

        let target_version = if let Some(ref ver_str) = entry.version {
            find_matching_version(&versions, ver_str).or_else(|| {
                eprintln!("[nite] warning: pinned version '{ver_str}' not found for mod '{}' on Minecraft {}", entry.name, mc_version);
                versions.first()
            })
        } else {
            versions.first()
        };

        if let Some(v) = target_version {
            visited_projects.insert(entry.name.clone());
            if let Some(ref pid) = v.project_id {
                if !visited_projects.insert(pid.clone()) && entry.version.is_none() {
                    continue;
                }
            }

            sync_mod_tree_recursive(
                client,
                v,
                mods_dir,
                mc_version,
                &mut tracked_files,
                &mut visited_projects,
                &mut visited_versions,
            )?;
        }
    }

    if let Ok(dir_entries) = fs::read_dir(mods_dir) {
        for dir_entry in dir_entries.flatten() {
            let path = dir_entry.path();
            if path.is_file() && path.extension().is_some_and(|e| e == "jar") {
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    if !tracked_files.contains(name) {
                        println!("[nite] purging unlisted mod: {name}");
                        let _ = fs::remove_file(&path);
                    }
                }
            }
        }
    }

    Ok(())
}

pub fn sync_pack_directory(
    client: &reqwest::blocking::Client,
    entries: &[PackageEntry],
    target_dir: &Path,
    category: &str,
    mc_version: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    fs::create_dir_all(target_dir)?;

    let mut tracked_files = HashSet::new();

    for entry in entries {
        let version_filtered_url = format!(
            "{MODRINTH_API_URL}/project/{}/version?game_versions=%5B%22{mc_version}%22%5D",
            entry.name
        );

        let resp = client
            .get(&version_filtered_url)
            .header("User-Agent", HTTP_USER_AGENT)
            .send()?;

        let mut versions: Vec<ModrinthVersion> = if resp.status().is_success() {
            resp.json().unwrap_or_default()
        } else {
            Vec::new()
        };

        if versions.is_empty() {
            let fallback_url = format!("{MODRINTH_API_URL}/project/{}/version", entry.name);
            let fallback_resp = client
                .get(&fallback_url)
                .header("User-Agent", HTTP_USER_AGENT)
                .send()?;
            if fallback_resp.status().is_success() {
                versions = fallback_resp.json().unwrap_or_default();
            }
        }

        let target_version = if let Some(ref ver_str) = entry.version {
            find_matching_version(&versions, ver_str).or_else(|| versions.first())
        } else {
            versions.first()
        };

        let chosen = match target_version {
            Some(v) => v,
            None => {
                eprintln!("[nite] warning: {category} '{}' has no downloadable files", entry.name);
                continue;
            }
        };

        let primary_file = chosen
            .files
            .iter()
            .find(|f| f.primary == Some(true))
            .or_else(|| chosen.files.first());

        if let Some(f) = primary_file {
            tracked_files.insert(f.filename.clone());
            let target_file = target_dir.join(&f.filename);

            if !target_file.exists() {
                println!("[nite] downloading {category}: {}", f.filename);
                let mut download_resp = client.get(&f.url).send()
                    .map_err(|e| format!("failed to request download for {category} '{}': {e}", f.filename))?;
                if !download_resp.status().is_success() {
                    return Err(format!("failed to download {category} '{}': HTTP {}", f.filename, download_resp.status()).into());
                }
                let mut out = File::create(&target_file)
                    .map_err(|e| format!("failed to create file for {category} '{}': {e}", target_file.display()))?;
                std::io::copy(&mut download_resp, &mut out)
                    .map_err(|e| format!("failed to write {category} file '{}': {e}", target_file.display()))?;
            }
        }
    }

    if let Ok(dir_entries) = fs::read_dir(target_dir) {
        for dir_entry in dir_entries.flatten() {
            let path = dir_entry.path();
            if path.is_file() {
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    if !tracked_files.contains(name) {
                        println!("[nite] purging unlisted {category}: {name}");
                        let _ = fs::remove_file(&path);
                    }
                }
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_matching_version() {
        let v1 = ModrinthVersion {
            id: Some("id1".to_string()),
            project_id: Some("proj1".to_string()),
            name: Some("[Fabric] Sodium Extra 0.9.4 - Minecraft 26.1.2".to_string()),
            version_number: Some("mc26.1.2-0.9.4+fabric".to_string()),
            files: vec![],
            dependencies: vec![],
        };
        let v2 = ModrinthVersion {
            id: Some("id2".to_string()),
            project_id: Some("proj1".to_string()),
            name: Some("[Fabric] Sodium Extra 0.9.3 for Minecraft 26.1.2".to_string()),
            version_number: Some("mc26.1.2-0.9.3+fabric".to_string()),
            files: vec![],
            dependencies: vec![],
        };
        let versions = vec![v1, v2];

        // Exact match
        assert_eq!(
            find_matching_version(&versions, "mc26.1.2-0.9.4+fabric").unwrap().id.as_deref(),
            Some("id1")
        );
        // By ID
        assert_eq!(
            find_matching_version(&versions, "id2").unwrap().version_number.as_deref(),
            Some("mc26.1.2-0.9.3+fabric")
        );
        // Substring match on version number
        assert_eq!(
            find_matching_version(&versions, "0.9.3").unwrap().id.as_deref(),
            Some("id2")
        );
        // Substring match on name
        assert_eq!(
            find_matching_version(&versions, "Sodium Extra 0.9.4").unwrap().id.as_deref(),
            Some("id1")
        );
        // Non-matching
        assert!(find_matching_version(&versions, "0.8.0").is_none());
    }
}
