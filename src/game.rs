//! Main facade for Minecraft instance launching, listing, and cache cleanup.

pub mod backup;
pub mod fabric;
pub mod launcher;
pub mod modpack;
pub mod mojang;
pub mod modrinth;
pub mod settings;
pub mod storage;

use crate::auth;
use crate::config::load_config;
use std::collections::HashSet;
use std::fs;
use storage::StoragePaths;

/// Resolves, synchronizes, and executes the specified Minecraft instance.
pub fn launch_instance(instance_name: &str) -> Result<(), Box<dyn std::error::Error>> {
    let config = load_config()?;
    let instance = config
        .instance
        .iter()
        .find(|i| i.name == instance_name)
        .ok_or_else(|| format!("[nite] instance '{instance_name}' not found in configuration"))?;

    let storage = StoragePaths::resolve(instance)?;

    // Check for authenticated Microsoft session or fallback to offline mode
    let session = auth::get_valid_session();
    let (username, uuid, access_token, user_type) = if let Some(ref s) = session {
        println!("[nite] authenticated as '{}'", s.username);
        (
            s.username.clone(),
            s.uuid.clone(),
            s.access_token.clone(),
            "msa".to_string(),
        )
    } else {
        let offline_uuid = mojang::generate_offline_uuid(&instance.username).to_string();
        (
            instance.username.clone(),
            offline_uuid,
            "0".to_string(),
            "legacy".to_string(),
        )
    };

    let http_client = reqwest::blocking::Client::builder()
        .tcp_nodelay(true)
        .build()?;

    let loader_label = if instance.fabric { "Fabric" } else { "Vanilla" };
    println!("[nite] launching {} instance '{}' (Minecraft {})", loader_label, instance.name, instance.version);

    // Sync declared modpack if set
    if let Some(ref modpack_source) = instance.modpack {
        modpack::sync_modpack(modpack_source, &storage.instance_dir)?;
    }

    if let Err(e) = mojang::fetch_version_metadata(&http_client, &storage.version_json, &instance.version) {
        if storage.version_json.exists() {
            println!("[nite] offline: using local version metadata ({e})");
        } else {
            return Err(e);
        }
    }

    if let Err(e) = mojang::fetch_client_binary(&http_client, &storage.version_json, &storage.client_jar) {
        if storage.client_jar.exists() {
            println!("[nite] offline: using local client jar ({e})");
        } else {
            return Err(e);
        }
    }

    let mut classpath = mojang::install_vanilla_libraries(
        &http_client,
        &storage.version_json,
        &storage.libs_dir,
        &storage.natives_dir,
    )?;

    let mut main_class_override = None;
    let mut extra_jvm_flags = Vec::new();

    let parsed_mods = instance.parsed_mods();
    if instance.fabric {
        let (fabric_main, jvm_flags, fabric_libs) =
            fabric::install_fabric_loader(&http_client, &instance.version, &storage.libs_dir)?;
        main_class_override = Some(fabric_main);
        extra_jvm_flags = jvm_flags;
        classpath.extend(fabric_libs);

        if !parsed_mods.is_empty() {
            let mods_dir = storage.instance_dir.join("mods");
            if let Err(e) = modrinth::sync_mods_directory(&http_client, &parsed_mods, &instance.version, &mods_dir) {
                println!("[nite] warning: failed to sync mods online, using local mods: {e}");
            }
        }
    }

    let parsed_rps = instance.parsed_resourcepacks();
    if !parsed_rps.is_empty() {
        let packs_dir = storage.instance_dir.join("resourcepacks");
        if let Err(e) = modrinth::sync_pack_directory(
            &http_client,
            &parsed_rps,
            &packs_dir,
            "resource pack",
            &instance.version,
        ) {
            println!("[nite] warning: failed to sync resource packs: {e}");
        }
    }

    let parsed_shaders = instance.parsed_shaderpacks();
    if !parsed_shaders.is_empty() {
        let shaders_dir = storage.instance_dir.join("shaderpacks");
        if let Err(e) = modrinth::sync_pack_directory(
            &http_client,
            &parsed_shaders,
            &shaders_dir,
            "shader pack",
            &instance.version,
        ) {
            println!("[nite] warning: failed to sync shader packs: {e}");
        }
    }

    if let Err(e) = mojang::sync_vanilla_assets(&http_client, &storage.version_json, &storage.assets_dir) {
        println!("[nite] warning: asset synchronization incomplete: {e}");
    }

    settings::apply_vanilla_settings(&storage.instance_dir, &instance.version, &instance.settings)?;
    settings::apply_modular_mod_configs(&storage.instance_dir, &instance.version, &instance.mods_config)?;
    settings::apply_shader_settings(&storage.instance_dir, &instance.shader_settings)?;

    let memory = instance.memory.as_deref().unwrap_or("4G");
    let java_override = instance.java.as_deref();

    println!("[nite] launching Minecraft...");
    launcher::spawn_game_process(
        &storage.version_json,
        &instance.version,
        &storage.client_jar,
        &classpath,
        &storage.instance_dir,
        &storage.assets_dir,
        &storage.natives_dir,
        &username,
        &uuid,
        &access_token,
        &user_type,
        memory,
        java_override,
        main_class_override.as_deref(),
        &extra_jvm_flags,
    )
}

/// Cleans up obsolete version files and instance folders not referenced in the config.
pub fn clean_unused_data() -> Result<(), Box<dyn std::error::Error>> {
    let config = load_config()?;
    let active_versions: HashSet<&str> = config.instance.iter().map(|i| i.version.as_str()).collect();
    let active_instances: HashSet<&str> = config.instance.iter().map(|i| i.name.as_str()).collect();

    let xdg = xdg::BaseDirectories::with_prefix("nite");
    let data_home = xdg.get_data_home().ok_or("Failed to resolve XDG data home")?;

    let shared_versions_dir = data_home.join("shared").join("versions");
    if let Ok(entries) = fs::read_dir(&shared_versions_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    if !active_versions.contains(name) {
                        println!("[nite] removing unused version binaries: {name}");
                        let _ = fs::remove_dir_all(&path);
                    }
                }
            }
        }
    }

    let instances_dir = data_home.join("instances");
    if let Ok(entries) = fs::read_dir(&instances_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    if !active_instances.contains(name) {
                        let saves_exist = path.join("saves").exists();
                        if saves_exist {
                            println!("[nite] preserving unreferenced instance '{name}' (contains singleplayer saves)");
                        } else {
                            println!("[nite] keeping instance folder '{name}' (use manual removal if no longer needed)");
                        }
                    }
                }
            }
        }
    }

    println!("[nite] cleanup completed");
    Ok(())
}

/// Lists configured instance names.
pub fn list_instances() -> Result<(), Box<dyn std::error::Error>> {
    let config = load_config()?;
    for instance in config.instance {
        println!("{}", instance.name);
    }
    Ok(())
}
