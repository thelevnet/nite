//! Configuration and recipe synchronization (options.txt and modular mod configs).

use serde::Deserialize;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Deserialize)]
struct ModConfigFile {
    #[serde(alias = "target_file")]
    config_file: String,
    #[serde(default)]
    settings_map: HashMap<String, ModSettingMapping>,
    #[serde(default)]
    keybinds: HashMap<String, String>,
}

#[derive(Deserialize)]
struct ModSettingMapping {
    target: String,
    #[serde(rename = "type")]
    setting_type: String,
}

pub fn get_recipe_search_paths() -> Vec<PathBuf> {
    let mut search_paths = Vec::new();
    if let Ok(custom) = std::env::var("NITE_RECIPES_PATH") {
        search_paths.push(PathBuf::from(custom));
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(prefix) = exe.parent().and_then(|p| p.parent()) {
            search_paths.push(prefix.join("share").join("nite").join("recipes"));
        }
    }
    search_paths.push(PathBuf::from("recipes"));
    search_paths.push(PathBuf::from("/etc/nite/recipes"));
    let xdg = xdg::BaseDirectories::with_prefix("nite");
    if let Some(cfg) = xdg.get_config_home() {
        search_paths.push(cfg.join("recipes"));
    }
    search_paths
}

pub fn apply_vanilla_settings(
    instance_dir: &Path,
    version: &str,
    settings: &HashMap<String, toml::Value>,
) -> std::io::Result<()> {
    if settings.is_empty() {
        return Ok(());
    }

    let search_paths = get_recipe_search_paths();

    let mut recipe: Option<ModConfigFile> = None;
    for root in &search_paths {
        let candidate = root.join(version).join("options.toml");
        if candidate.exists() {
            if let Ok(content) = fs::read_to_string(&candidate) {
                if let Ok(parsed) = toml::from_str(&content) {
                    recipe = Some(parsed);
                    break;
                }
            }
        }
    }

    let options_filename = recipe
        .as_ref()
        .map(|r| r.config_file.as_str())
        .unwrap_or("options.txt");

    let options_file = instance_dir.join(options_filename);
    let mut options_map: HashMap<String, String> = HashMap::new();
    let mut key_order: Vec<String> = Vec::new();

    if options_file.exists() {
        let content = fs::read_to_string(&options_file)?;
        for line in content.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            if let Some((k, v)) = trimmed.split_once(':') {
                let key = k.trim().to_string();
                let val = v.trim().to_string();
                if !options_map.contains_key(&key) {
                    key_order.push(key.clone());
                }
                options_map.insert(key, val);
            }
        }
    }

    for (key, val) in settings {
        let (canonical_key, expected_type) = if let Some(ref r) = recipe {
            if let Some(m) = r.settings_map.get(key) {
                (m.target.as_str(), m.setting_type.as_str())
            } else if key.eq_ignore_ascii_case("vsync") {
                ("enableVsync", "bool")
            } else {
                (key.as_str(), "string")
            }
        } else if key.eq_ignore_ascii_case("vsync") {
            ("enableVsync", "bool")
        } else {
            (key.as_str(), "string")
        };

        let formatted_val = match (expected_type, val) {
            (_, toml::Value::Boolean(b)) => b.to_string(),
            (_, toml::Value::Integer(i)) => i.to_string(),
            (_, toml::Value::Float(f)) => f.to_string(),
            // options.txt never uses quoted strings — write raw value
            (_, toml::Value::String(s)) => s.clone(),
            _ => val.to_string(),
        };

        if !options_map.contains_key(canonical_key) {
            key_order.push(canonical_key.to_string());
        }
        options_map.insert(canonical_key.to_string(), formatted_val);
    }

    let mut output = String::new();
    for key in key_order {
        if let Some(val) = options_map.get(&key) {
            output.push_str(&format!("{key}:{val}\n"));
        }
    }

    fs::write(&options_file, output)?;
    println!("[nite] applied {} settings overrides to {options_filename}", settings.len());
    Ok(())
}

pub fn apply_modular_mod_configs(
    instance_dir: &Path,
    version: &str,
    mods_config: &HashMap<String, HashMap<String, toml::Value>>,
) -> Result<(), Box<dyn std::error::Error>> {
    if mods_config.is_empty() {
        return Ok(());
    }

    let search_paths = get_recipe_search_paths();

    for (mod_name, user_values) in mods_config {
        let mut recipe_file = None;
        for root in &search_paths {
            let candidate = root.join(version).join("mods").join(format!("{mod_name}.toml"));
            if candidate.exists() {
                recipe_file = Some(candidate);
                break;
            }
        }

        let recipe_path = match recipe_file {
            Some(p) => p,
            None => {
                eprintln!("[nite] warning: no configuration recipe found for '{mod_name}' (recipes/{version}/mods/{mod_name}.toml)");
                continue;
            }
        };

        let recipe_content = fs::read_to_string(&recipe_path)?;
        let recipe: ModConfigFile = toml::from_str(&recipe_content)?;

        // Handle keybinds that belong in options.txt
        let mut keybind_overrides: Vec<(String, String)> = Vec::new();
        for (user_key, val) in user_values {
            if let Some(target_keybind) = recipe.keybinds.get(user_key) {
                let bind_val = match val {
                    toml::Value::String(s) => s.clone(),
                    _ => val.to_string(),
                };
                keybind_overrides.push((target_keybind.clone(), bind_val));
            }
        }

        if !keybind_overrides.is_empty() {
            let options_file = instance_dir.join("options.txt");
            let mut options_map: HashMap<String, String> = HashMap::new();
            let mut key_order: Vec<String> = Vec::new();

            if options_file.exists() {
                if let Ok(content) = fs::read_to_string(&options_file) {
                    for line in content.lines() {
                        let trimmed = line.trim();
                        if trimmed.is_empty() {
                            continue;
                        }
                        if let Some((k, v)) = trimmed.split_once(':') {
                            let key = k.trim().to_string();
                            let val = v.trim().to_string();
                            if !options_map.contains_key(&key) {
                                key_order.push(key.clone());
                            }
                            options_map.insert(key, val);
                        }
                    }
                }
            }

            for (target_key, val) in keybind_overrides {
                if !options_map.contains_key(&target_key) {
                    key_order.push(target_key.clone());
                }
                options_map.insert(target_key, val);
            }

            let mut output = String::new();
            for key in key_order {
                if let Some(val) = options_map.get(&key) {
                    output.push_str(&format!("{key}:{val}\n"));
                }
            }
            let _ = fs::write(&options_file, output);
        }

        let target_file = instance_dir.join(&recipe.config_file);
        if let Some(parent) = target_file.parent() {
            fs::create_dir_all(parent)?;
        }

        let extension = target_file.extension().and_then(|e| e.to_str()).unwrap_or("");

        if extension == "properties" {
            let mut props = HashMap::new();
            let mut order = Vec::new();

            if target_file.exists() {
                let content = fs::read_to_string(&target_file)?;
                for line in content.lines() {
                    let trimmed = line.trim();
                    if trimmed.is_empty() || trimmed.starts_with('#') {
                        continue;
                    }
                    if let Some((k, v)) = trimmed.split_once('=') {
                        let key = k.trim().to_string();
                        let val = v.trim().to_string();
                        if !props.contains_key(&key) {
                            order.push(key.clone());
                        }
                        props.insert(key, val);
                    }
                }
            }

            for (user_key, val) in user_values {
                let (target_key, expected_type) = if let Some(m) = recipe.settings_map.get(user_key) {
                    (m.target.as_str(), m.setting_type.as_str())
                } else {
                    (user_key.as_str(), "string")
                };

                let formatted = match (expected_type, val) {
                    ("bool", toml::Value::Boolean(b)) => b.to_string(),
                    ("int", toml::Value::Integer(i)) => i.to_string(),
                    ("float", toml::Value::Float(f)) => f.to_string(),
                    ("string", toml::Value::String(s)) => s.clone(),
                    (_, toml::Value::String(s)) => s.clone(),
                    (_, toml::Value::Boolean(b)) => b.to_string(),
                    (_, toml::Value::Integer(i)) => i.to_string(),
                    (_, toml::Value::Float(f)) => f.to_string(),
                    _ => val.to_string(),
                };

                if !props.contains_key(target_key) {
                    order.push(target_key.to_string());
                }
                props.insert(target_key.to_string(), formatted);
            }

            let mut output = String::new();
            for key in order {
                if let Some(val) = props.get(&key) {
                    output.push_str(&format!("{key}={val}\n"));
                }
            }
            fs::write(&target_file, output)?;
            println!("[nite] updated {}", recipe.config_file);
        } else if extension == "json" || extension == "json5" {
            let mut root: serde_json::Value = if target_file.exists() {
                let content = fs::read_to_string(&target_file).unwrap_or_default();
                serde_json::from_str(&content).unwrap_or_else(|_| serde_json::json!({}))
            } else {
                serde_json::json!({})
            };

            for (user_key, val) in user_values {
                let target_key = recipe
                    .settings_map
                    .get(user_key)
                    .map(|m| m.target.as_str())
                    .unwrap_or(user_key.as_str());

                let json_val = match val {
                    toml::Value::String(s) => serde_json::Value::String(s.clone()),
                    toml::Value::Integer(i) => serde_json::Value::Number((*i).into()),
                    toml::Value::Float(f) => serde_json::Number::from_f64(*f)
                        .map(serde_json::Value::Number)
                        .unwrap_or(serde_json::Value::Null),
                    toml::Value::Boolean(b) => serde_json::Value::Bool(*b),
                    _ => serde_json::Value::Null,
                };

                let path_segments: Vec<&str> = target_key.split('.').collect();
                if path_segments.len() == 1 {
                    root[path_segments[0]] = json_val;
                } else if path_segments.len() == 2 {
                    if !root[path_segments[0]].is_object() {
                        root[path_segments[0]] = serde_json::json!({});
                    }
                    root[path_segments[0]][path_segments[1]] = json_val;
                } else if path_segments.len() == 3 {
                    if !root[path_segments[0]].is_object() {
                        root[path_segments[0]] = serde_json::json!({});
                    }
                    if !root[path_segments[0]][path_segments[1]].is_object() {
                        root[path_segments[0]][path_segments[1]] = serde_json::json!({});
                    }
                    root[path_segments[0]][path_segments[1]][path_segments[2]] = json_val;
                }
            }

            let output = serde_json::to_string_pretty(&root)?;
            fs::write(&target_file, output)?;
            println!("[nite] updated {}", recipe.config_file);
        } else if extension == "toml" {
            let mut root: toml::Table = if target_file.exists() {
                let content = fs::read_to_string(&target_file).unwrap_or_default();
                toml::from_str(&content).unwrap_or_default()
            } else {
                toml::Table::new()
            };

            for (user_key, val) in user_values {
                let target_key = recipe
                    .settings_map
                    .get(user_key)
                    .map(|m| m.target.as_str())
                    .unwrap_or(user_key.as_str());

                root.insert(target_key.to_string(), val.clone());
            }

            let output = toml::to_string_pretty(&root)?;
            fs::write(&target_file, output)?;
            println!("[nite] updated {}", recipe.config_file);
        }
    }

    Ok(())
}

pub fn apply_shader_settings(
    instance_dir: &Path,
    shader_settings: &HashMap<String, HashMap<String, toml::Value>>,
) -> Result<(), Box<dyn std::error::Error>> {
    let shaderpacks_dir = instance_dir.join("shaderpacks");
    if !shaderpacks_dir.exists() {
        fs::create_dir_all(&shaderpacks_dir)?;
    }

    for (pack_name, settings) in shader_settings {
        let txt_name = if pack_name.ends_with(".zip") {
            format!("{}.txt", pack_name)
        } else if pack_name.ends_with(".txt") {
            pack_name.clone()
        } else {
            format!("{}.zip.txt", pack_name)
        };

        let target_file = shaderpacks_dir.join(&txt_name);

        let mut props = HashMap::new();
        let mut order = Vec::new();

        if target_file.exists() {
            let content = fs::read_to_string(&target_file)?;
            for line in content.lines() {
                let trimmed = line.trim();
                if trimmed.is_empty() || trimmed.starts_with('#') {
                    continue;
                }
                if let Some((k, v)) = trimmed.split_once('=') {
                    let key = k.trim().to_string();
                    let val = v.trim().to_string();
                    if !props.contains_key(&key) {
                        order.push(key.clone());
                    }
                    props.insert(key, val);
                }
            }
        }

        for (key, val) in settings {
            let formatted = match val {
                toml::Value::String(s) => s.clone(),
                toml::Value::Boolean(b) => b.to_string(),
                toml::Value::Integer(i) => i.to_string(),
                toml::Value::Float(f) => f.to_string(),
                _ => val.to_string(),
            };

            if !props.contains_key(key) {
                order.push(key.clone());
            }
            props.insert(key.clone(), formatted);
        }

        let mut output = String::new();
        for key in order {
            if let Some(val) = props.get(&key) {
                output.push_str(&format!("{}={}\n", key, val));
            }
        }
        
        fs::write(&target_file, output)?;
        println!("[nite] updated shader settings for {}", pack_name);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_apply_vanilla_settings_unquoted_and_preserved() {
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let temp_dir = std::env::temp_dir().join(format!("nite_test_{now}"));
        let _ = fs::create_dir_all(&temp_dir);

        let initial_options = "fov:70.0\nkey_key.jump:key.keyboard.space\n";
        fs::write(temp_dir.join("options.txt"), initial_options).unwrap();

        let mut settings = HashMap::new();
        settings.insert("vsync".to_string(), toml::Value::Boolean(false));
        settings.insert("key_key.jump".to_string(), toml::Value::String("key.keyboard.space".to_string()));
        settings.insert("soundDevice".to_string(), toml::Value::String("default".to_string()));

        apply_vanilla_settings(&temp_dir, "nonexistent_version", &settings).unwrap();

        let updated = fs::read_to_string(temp_dir.join("options.txt")).unwrap();
        // Check that strings are unquoted
        assert!(updated.contains("soundDevice:default"));
        assert!(!updated.contains("soundDevice:\"default\""));
        // Check boolean
        assert!(updated.contains("enableVsync:false"));
        // Check original key preserved
        assert!(updated.contains("fov:70.0"));

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_apply_shader_settings() {
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let temp_dir = std::env::temp_dir().join(format!("nite_test_shader_{now}"));
        let _ = fs::create_dir_all(&temp_dir);

        let mut shader_settings = HashMap::new();
        let mut comp_settings = HashMap::new();
        comp_settings.insert("SHADOW_RESOLUTION".to_string(), toml::Value::Integer(2048));
        comp_settings.insert("WATER_QUALITY".to_string(), toml::Value::String("HIGH".to_string()));
        shader_settings.insert("ComplementaryUnbound.zip".to_string(), comp_settings);

        apply_shader_settings(&temp_dir, &shader_settings).unwrap();

        let updated = fs::read_to_string(temp_dir.join("shaderpacks/ComplementaryUnbound.zip.txt")).unwrap();
        assert!(updated.contains("SHADOW_RESOLUTION=2048"));
        assert!(updated.contains("WATER_QUALITY=HIGH"));

        let _ = fs::remove_dir_all(&temp_dir);
    }
}
