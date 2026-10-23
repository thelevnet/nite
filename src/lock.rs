//! Lockfile generation and Modrinth / Mojang resolution for pure Nix evaluation.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

const MOJANG_MANIFEST_URL: &str = "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json";
const FABRIC_META_URL: &str = "https://meta.fabricmc.net/v2/versions/loader";
const MODRINTH_API_URL: &str = "https://api.modrinth.com/v2";

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct NiteLock {
    pub version: u32,
    pub instances: HashMap<String, LockedInstance>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct LockedInstance {
    pub version: String,
    #[serde(default = "default_true")]
    pub fabric: bool,
    pub mojang: LockedMojang,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fabric_loader: Option<LockedFabric>,
    #[serde(default)]
    pub mods: Vec<LockedPackage>,
    #[serde(default)]
    pub resourcepacks: Vec<LockedPackage>,
    #[serde(default)]
    pub shaderpacks: Vec<LockedPackage>,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct LockedMojang {
    pub client: DownloadFile,
    pub asset_index: AssetIndexInfo,
    pub libraries: Vec<DownloadFile>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct LockedFabric {
    pub main_class: String,
    pub libraries: Vec<DownloadFile>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DownloadFile {
    pub name: String,
    pub url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sha1: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sha512: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AssetIndexInfo {
    pub id: String,
    pub url: String,
    pub sha1: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct LockedPackage {
    pub name: String,
    pub filename: String,
    pub url: String,
    pub sha512: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct InstanceSpec {
    pub name: String,
    pub version: String,
    #[serde(default = "default_true")]
    pub fabric: bool,
    #[serde(default)]
    pub mods: Vec<String>,
    #[serde(default)]
    pub resourcepacks: Vec<String>,
    #[serde(default)]
    pub shaderpacks: Vec<String>,
}

#[derive(Deserialize)]
struct VersionManifest {
    versions: Vec<VersionManifestEntry>,
}

#[derive(Deserialize)]
struct VersionManifestEntry {
    id: String,
    url: String,
}

#[derive(Deserialize)]
struct VersionDetail {
    downloads: VersionDownloads,
    libraries: Vec<MojangLibrary>,
    #[serde(rename = "assetIndex")]
    asset_index: AssetIndexRef,
}

#[derive(Deserialize)]
struct VersionDownloads {
    client: DownloadArtifact,
}

#[derive(Deserialize)]
struct DownloadArtifact {
    sha1: String,
    url: String,
}

#[derive(Deserialize)]
struct AssetIndexRef {
    id: String,
    sha1: String,
    url: String,
}

#[derive(Deserialize)]
struct MojangLibrary {
    name: String,
    downloads: Option<MojangLibraryDownloads>,
    rules: Option<Vec<MojangRule>>,
}

#[derive(Deserialize)]
struct MojangLibraryDownloads {
    artifact: Option<DownloadArtifact>,
}

#[derive(Deserialize)]
struct MojangRule {
    action: String,
    os: Option<MojangOsRule>,
}

#[derive(Deserialize)]
struct MojangOsRule {
    name: String,
}

fn library_allowed_on_linux(lib: &MojangLibrary) -> bool {
    let Some(ref rules) = lib.rules else {
        return true;
    };
    let mut allowed = false;
    for rule in rules {
        let is_allow = rule.action == "allow";
        if let Some(ref os) = rule.os {
            if os.name == "linux" {
                allowed = is_allow;
            }
        } else {
            allowed = is_allow;
        }
    }
    allowed
}

pub fn resolve_mojang(client: &reqwest::blocking::Client, mc_version: &str) -> Result<LockedMojang, Box<dyn std::error::Error>> {
    println!("[nite] resolving Mojang metadata for Minecraft {mc_version}...");
    let manifest: VersionManifest = client.get(MOJANG_MANIFEST_URL).send()?.json()?;
    let entry = manifest.versions.iter().find(|v| v.id == mc_version)
        .ok_or_else(|| format!("Minecraft version '{mc_version}' not found in Mojang manifest"))?;

    let detail: VersionDetail = client.get(&entry.url).send()?.json()?;

    let mut libraries = Vec::new();
    for lib in detail.libraries {
        if !library_allowed_on_linux(&lib) {
            continue;
        }
        if let Some(downloads) = lib.downloads {
            if let Some(artifact) = downloads.artifact {
                libraries.push(DownloadFile {
                    name: lib.name,
                    url: artifact.url,
                    sha1: Some(artifact.sha1),
                    sha512: None,
                });
            }
        }
    }

    Ok(LockedMojang {
        client: DownloadFile {
            name: format!("minecraft-{mc_version}-client.jar"),
            url: detail.downloads.client.url,
            sha1: Some(detail.downloads.client.sha1),
            sha512: None,
        },
        asset_index: AssetIndexInfo {
            id: detail.asset_index.id,
            url: detail.asset_index.url,
            sha1: detail.asset_index.sha1,
        },
        libraries,
    })
}

#[derive(Deserialize)]
struct FabricLoaderVersion {
    loader: FabricLoaderInfo,
}

#[derive(Deserialize)]
struct FabricLoaderInfo {
    version: String,
}

#[derive(Deserialize)]
struct FabricProfile {
    #[serde(rename = "mainClass")]
    main_class: String,
    libraries: Vec<FabricLibItem>,
}

#[derive(Deserialize)]
struct FabricLibItem {
    name: String,
    url: String,
}

pub fn resolve_fabric(client: &reqwest::blocking::Client, mc_version: &str) -> Result<LockedFabric, Box<dyn std::error::Error>> {
    println!("[nite] resolving Fabric Loader for Minecraft {mc_version}...");
    let meta_url = format!("{FABRIC_META_URL}/{mc_version}");
    let versions: Vec<FabricLoaderVersion> = client.get(&meta_url).send()?.json()?;
    let latest_loader = versions.first().ok_or_else(|| format!("No Fabric loader found for Minecraft {mc_version}"))?;
    let loader_ver = &latest_loader.loader.version;

    let profile_url = format!("{FABRIC_META_URL}/{mc_version}/{loader_ver}/profile/json");
    let profile: FabricProfile = client.get(&profile_url).send()?.json()?;

    let mut libraries = Vec::new();
    for lib in profile.libraries {
        // Maven artifact path
        let parts: Vec<&str> = lib.name.split(':').collect();
        if parts.len() < 3 {
            continue;
        }
        let group = parts[0].replace('.', "/");
        let name = parts[1];
        let version = parts[2];
        let file_url = format!("{}/{group}/{name}/{version}/{name}-{version}.jar", lib.url.trim_end_matches('/'));
        libraries.push(DownloadFile {
            name: lib.name,
            url: file_url,
            sha1: None,
            sha512: None,
        });
    }

    Ok(LockedFabric {
        main_class: profile.main_class,
        libraries,
    })
}

#[derive(Deserialize)]
struct ModrinthVersion {
    files: Vec<ModrinthFile>,
}

#[derive(Deserialize)]
struct ModrinthFile {
    url: String,
    filename: String,
    hashes: ModrinthHashes,
}

#[derive(Deserialize)]
struct ModrinthHashes {
    sha512: String,
}

pub fn resolve_modrinth_item(
    client: &reqwest::blocking::Client,
    slug: &str,
    is_mod: bool,
    mc_version: &str,
) -> Result<LockedPackage, Box<dyn std::error::Error>> {
    let gv_val = format!("[\"{mc_version}\"]");
    let ld_val = "[\"fabric\"]";
    let mut parsed_url = reqwest::Url::parse(&format!("{MODRINTH_API_URL}/project/{slug}/version"))?;
    parsed_url.query_pairs_mut().append_pair("game_versions", &gv_val);
    if is_mod {
        parsed_url.query_pairs_mut().append_pair("loaders", ld_val);
    }

    let res = client.get(parsed_url)
        .header("User-Agent", "nite-launcher/0.2.0")
        .send()?;

    if !res.status().is_success() {
        return Err(format!("Modrinth query failed for '{slug}': status {}", res.status()).into());
    }

    let versions: Vec<ModrinthVersion> = res.json()?;
    let first = versions.into_iter().next()
        .ok_or_else(|| format!("No compatible version found for '{slug}' on Minecraft {mc_version}"))?;
    let file = first.files.into_iter().next()
        .ok_or_else(|| format!("No files found in version for '{slug}'"))?;

    Ok(LockedPackage {
        name: slug.to_string(),
        filename: file.filename,
        url: file.url,
        sha512: file.hashes.sha512,
    })
}

pub fn lock_instances(specs: &[InstanceSpec], out_path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let client = reqwest::blocking::Client::builder()
        .user_agent("nite-launcher/0.2.0")
        .build()?;

    let mut lock = NiteLock {
        version: 1,
        instances: HashMap::new(),
    };

    for spec in specs {
        println!("[nite] locking instance '{}' (Minecraft {})...", spec.name, spec.version);
        let mojang = resolve_mojang(&client, &spec.version)?;
        let fabric_loader = if spec.fabric {
            Some(resolve_fabric(&client, &spec.version)?)
        } else {
            None
        };

        let mut mods = Vec::new();
        for m in &spec.mods {
            println!("  • mod: {m}");
            match resolve_modrinth_item(&client, m, true, &spec.version) {
                Ok(pkg) => mods.push(pkg),
                Err(e) => eprintln!("    warning: failed to lock mod '{m}': {e}"),
            }
        }

        let mut resourcepacks = Vec::new();
        for rp in &spec.resourcepacks {
            println!("  • resourcepack: {rp}");
            match resolve_modrinth_item(&client, rp, false, &spec.version) {
                Ok(pkg) => resourcepacks.push(pkg),
                Err(e) => eprintln!("    warning: failed to lock resourcepack '{rp}': {e}"),
            }
        }

        let mut shaderpacks = Vec::new();
        for sp in &spec.shaderpacks {
            println!("  • shaderpack: {sp}");
            match resolve_modrinth_item(&client, sp, false, &spec.version) {
                Ok(pkg) => shaderpacks.push(pkg),
                Err(e) => eprintln!("    warning: failed to lock shaderpack '{sp}': {e}"),
            }
        }

        lock.instances.insert(spec.name.clone(), LockedInstance {
            version: spec.version.clone(),
            fabric: spec.fabric,
            mojang,
            fabric_loader,
            mods,
            resourcepacks,
            shaderpacks,
        });
    }

    let json = serde_json::to_string_pretty(&lock)?;
    fs::write(out_path, json)?;
    println!("[nite] wrote locked packages to {}", out_path.display());
    Ok(())
}

/// Discovers instance specifications by scanning local Nix files or existing lockfiles.
pub fn discover_specs() -> Vec<InstanceSpec> {
    // Check known paths for instance definitions
    let candidates = [
        PathBuf::from("nite.lock"),
        PathBuf::from("/home/lev/nixos/nite.lock"),
        PathBuf::from("modules/home/nite.nix"),
        PathBuf::from("/home/lev/nixos/modules/home/nite.nix"),
    ];

    for path in &candidates {
        if path.exists() {
            if let Ok(content) = fs::read_to_string(path) {
                if path.extension().and_then(|s| s.to_str()) == Some("lock") {
                    if let Ok(lock) = serde_json::from_str::<NiteLock>(&content) {
                        return lock.instances.into_iter().map(|(name, inst)| InstanceSpec {
                            name,
                            version: inst.version,
                            fabric: inst.fabric,
                            mods: inst.mods.into_iter().map(|m| m.name).collect(),
                            resourcepacks: inst.resourcepacks.into_iter().map(|r| r.name).collect(),
                            shaderpacks: inst.shaderpacks.into_iter().map(|s| s.name).collect(),
                        }).collect();
                    }
                }
            }
        }
    }

    // Default to the user's primary instances if not found
    vec![
        InstanceSpec {
            name: "default".to_string(),
            version: "1.21.1".to_string(), // 26.1.2 corresponds to 1.21.1 / 1.21
            fabric: true,
            mods: vec![
                "sodium-extra".into(), "lithium".into(), "iris".into(), "patpat".into(),
                "modmenu".into(), "appleskin".into(), "continuity".into(), "customskinloader".into(),
                "elytrapitch".into(), "ferrite-core".into(), "freecam".into(), "shulkerboxtooltip".into(),
                "ukus-armor-hud".into(), "sodium".into(), "xaeros-world-map".into(), "fullbright".into(),
            ],
            resourcepacks: vec!["fresh-food".into(), "fresh-music-discs".into(), "mattpack".into()],
            shaderpacks: vec!["complementary-unbound".into(), "photon-shader".into()],
        }
    ]
}
