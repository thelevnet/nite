//! Configuration schema and loader for nite launcher.
//!
//! Loads declarative instance definitions from `$XDG_CONFIG_HOME/nite/instances.toml`.

use serde::Deserialize;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

/// Root configuration holding all instance definitions.
#[derive(Deserialize, Debug)]
pub struct Config {
    #[serde(default)]
    pub instance: Vec<Instance>,
}

/// A mod or package entry in instances.toml which can be either a simple slug `"sodium"`
/// or a detailed object with version pinning: `{ name = "sodium", version = "0.5.8" }`.
#[derive(Debug, Clone)]
pub struct PackageEntry {
    pub name: String,
    pub version: Option<String>,
}

impl PackageEntry {
    pub fn parse(val: &toml::Value) -> Option<Self> {
        match val {
            toml::Value::String(s) => Some(Self {
                name: s.clone(),
                version: None,
            }),
            toml::Value::Table(t) => {
                let name = t.get("name").and_then(|v| v.as_str())?.to_string();
                let version = t.get("version").and_then(|v| v.as_str()).map(|s| s.to_string());
                Some(Self { name, version })
            }
            _ => None,
        }
    }
}

/// An individual Minecraft instance definition.
#[derive(Deserialize, Debug, Clone)]
pub struct Instance {
    /// Friendly identifier for the instance (e.g. "default", "survival").
    pub name: String,

    /// In-game player username.
    pub username: String,

    /// Target Minecraft version (e.g. "26.1.2", "1.21.1").
    pub version: String,

    /// Whether to install and launch with Fabric Loader.
    #[serde(default)]
    pub fabric: bool,

    /// Raw list of Modrinth mods (slug strings or { name, version } tables).
    #[serde(default)]
    pub mods: Vec<toml::Value>,

    /// Raw list of Modrinth resource packs.
    #[serde(default)]
    pub resourcepacks: Vec<toml::Value>,

    /// Raw list of Modrinth shader packs.
    #[serde(default)]
    pub shaderpacks: Vec<toml::Value>,

    /// Key-value overrides written to the instance's `options.txt`.
    #[serde(default)]
    pub settings: HashMap<String, toml::Value>,

    /// Memory allocation (e.g. "4G", "8G", "2048M"). Defaults to "4G".
    pub memory: Option<String>,

    /// Optional explicit path to the java binary.
    pub java: Option<String>,

    /// Per-mod settings overrides matching `mods/<mod>/<version>.toml` schemas.
    #[serde(default)]
    pub mods_config: HashMap<String, HashMap<String, toml::Value>>,

    /// Shader pack specific settings written to `shaderpacks/<pack>.zip.txt`.
    #[serde(default)]
    pub shader_settings: HashMap<String, HashMap<String, toml::Value>>,

    /// Optional .mrpack modpack URL or local file path to install declaratively.
    /// nite will install (or re-install if changed) the modpack on every `nite run`.
    pub modpack: Option<String>,
}

impl Instance {
    pub fn parsed_mods(&self) -> Vec<PackageEntry> {
        self.mods.iter().filter_map(PackageEntry::parse).collect()
    }

    pub fn parsed_resourcepacks(&self) -> Vec<PackageEntry> {
        self.resourcepacks.iter().filter_map(PackageEntry::parse).collect()
    }

    pub fn parsed_shaderpacks(&self) -> Vec<PackageEntry> {
        self.shaderpacks.iter().filter_map(PackageEntry::parse).collect()
    }
}

/// Resolves the path to `nite/instances.toml` using XDG base directory specifications.
pub fn find_config_path() -> Result<PathBuf, Box<dyn std::error::Error>> {
    let xdg = xdg::BaseDirectories::with_prefix("nite");
    xdg.find_config_file("instances.toml")
        .ok_or_else(|| "[nite] config file 'nite/instances.toml' not found in XDG config directories".into())
}

/// Resolves or default-creates the path to `instances.toml`.
pub fn get_or_create_config_path() -> Result<PathBuf, Box<dyn std::error::Error>> {
    let xdg = xdg::BaseDirectories::with_prefix("nite");
    if let Some(existing) = xdg.find_config_file("instances.toml") {
        return Ok(existing);
    }
    let config_dir = xdg.get_config_home().unwrap_or_else(|| PathBuf::from("~/.config/nite"));
    fs::create_dir_all(&config_dir)?;
    let new_path = config_dir.join("instances.toml");
    if !new_path.exists() {
        fs::write(&new_path, "# nite instances configuration\n")?;
    }
    Ok(new_path)
}

/// Checks if the given path is a symlink or located in read-only nix store.
pub fn is_symlink_or_nix(path: &PathBuf) -> bool {
    if let Ok(meta) = fs::symlink_metadata(path) {
        if meta.file_type().is_symlink() || meta.permissions().readonly() {
            return true;
        }
    }
    if let Ok(canon) = fs::canonicalize(path) {
        if canon.starts_with("/nix/store") {
            return true;
        }
    }
    false
}

/// Parses the user configuration file into a [`Config`] struct.
pub fn load_config() -> Result<Config, Box<dyn std::error::Error>> {
    let path = find_config_path()?;
    let content = fs::read_to_string(&path)
        .map_err(|e| format!("failed to read configuration at '{}': {e}", path.display()))?;
    let config: Config = toml::from_str(&content)
        .map_err(|e| format!("failed to parse configuration at '{}':\n{e}", path.display()))?;
    Ok(config)
}

/// Category of instance item to manage.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ItemCategory {
    Mods,
    ResourcePacks,
    Shaders,
}

impl ItemCategory {
    pub fn config_key(&self) -> &'static str {
        match self {
            ItemCategory::Mods => "mods",
            ItemCategory::ResourcePacks => "resourcepacks",
            ItemCategory::Shaders => "shaderpacks",
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            ItemCategory::Mods => "mod",
            ItemCategory::ResourcePacks => "resource pack",
            ItemCategory::Shaders => "shader pack",
        }
    }
}

/// Result of attempting to modify an item in the configuration.
pub enum ItemMutationResult {
    /// Config is managed by NixOS/Home Manager as a symlink or read-only store path.
    NixManaged {
        config_path: PathBuf,
        instance_name: String,
        category: ItemCategory,
        action: &'static str,
        name: String,
        version: Option<String>,
    },
    /// Config file was successfully mutated directly on disk.
    Mutated {
        config_path: PathBuf,
        instance_name: String,
        category: ItemCategory,
        action: &'static str,
        name: String,
        version: Option<String>,
    },
}

/// Installs or adds an item (mod, resourcepack, shaderpack) to an instance.
pub fn install_item(
    instance_name: &str,
    category: ItemCategory,
    item_name: &str,
    version: Option<String>,
) -> Result<ItemMutationResult, Box<dyn std::error::Error>> {
    let path = find_config_path()?;
    if is_symlink_or_nix(&path) {
        return Ok(ItemMutationResult::NixManaged {
            config_path: path,
            instance_name: instance_name.to_string(),
            category,
            action: "install",
            name: item_name.to_string(),
            version,
        });
    }

    let content = fs::read_to_string(&path)?;
    let mut doc: toml::Table = content.parse()?;

    let instances = doc
        .get_mut("instance")
        .and_then(|v| v.as_array_mut())
        .ok_or_else(|| "no [[instance]] found in configuration".to_string())?;

    let instance = instances
        .iter_mut()
        .find(|tbl| tbl.get("name").and_then(|n| n.as_str()) == Some(instance_name))
        .ok_or_else(|| format!("instance '{instance_name}' not found in configuration"))?;

    let instance_tbl = instance
        .as_table_mut()
        .ok_or_else(|| format!("instance '{instance_name}' is not a table"))?;

    let key = category.config_key();
    let array = instance_tbl
        .entry(key)
        .or_insert_with(|| toml::Value::Array(Vec::new()))
        .as_array_mut()
        .ok_or_else(|| format!("'{key}' in configuration is not an array"))?;

    // Build the value: either string or table with version
    let new_val = if let Some(ref ver) = version {
        let mut t = toml::map::Map::new();
        t.insert("name".to_string(), toml::Value::String(item_name.to_string()));
        t.insert("version".to_string(), toml::Value::String(ver.clone()));
        toml::Value::Table(t)
    } else {
        toml::Value::String(item_name.to_string())
    };

    // Remove old entry if matching name exists
    array.retain(|item| {
        match item {
            toml::Value::String(s) => s != item_name,
            toml::Value::Table(t) => t.get("name").and_then(|n| n.as_str()) != Some(item_name),
            _ => true,
        }
    });

    array.push(new_val);
    fs::write(&path, toml::to_string_pretty(&doc)?)?;

    Ok(ItemMutationResult::Mutated {
        config_path: path,
        instance_name: instance_name.to_string(),
        category,
        action: "install",
        name: item_name.to_string(),
        version,
    })
}

/// Removes an item from the named instance.
pub fn remove_item(
    instance_name: &str,
    category: ItemCategory,
    item_name: &str,
) -> Result<ItemMutationResult, Box<dyn std::error::Error>> {
    let path = find_config_path()?;
    if is_symlink_or_nix(&path) {
        return Ok(ItemMutationResult::NixManaged {
            config_path: path,
            instance_name: instance_name.to_string(),
            category,
            action: "remove",
            name: item_name.to_string(),
            version: None,
        });
    }

    let content = fs::read_to_string(&path)?;
    let mut doc: toml::Table = content.parse()?;

    let instances = doc
        .get_mut("instance")
        .and_then(|v| v.as_array_mut())
        .ok_or_else(|| "no [[instance]] found in configuration".to_string())?;

    let instance = instances
        .iter_mut()
        .find(|tbl| tbl.get("name").and_then(|n| n.as_str()) == Some(instance_name))
        .ok_or_else(|| format!("instance '{instance_name}' not found in configuration"))?;

    let instance_tbl = instance
        .as_table_mut()
        .ok_or_else(|| format!("instance '{instance_name}' is not a table"))?;

    let key = category.config_key();
    if let Some(array) = instance_tbl.get_mut(key).and_then(|v| v.as_array_mut()) {
        array.retain(|item| {
            match item {
                toml::Value::String(s) => s != item_name,
                toml::Value::Table(t) => t.get("name").and_then(|n| n.as_str()) != Some(item_name),
                _ => true,
            }
        });
    }

    fs::write(&path, toml::to_string_pretty(&doc)?)?;

    Ok(ItemMutationResult::Mutated {
        config_path: path,
        instance_name: instance_name.to_string(),
        category,
        action: "remove",
        name: item_name.to_string(),
        version: None,
    })
}

/// Updates an item in the instance configuration.
pub fn update_item(
    instance_name: &str,
    category: ItemCategory,
    item_name: &str,
    version: Option<String>,
) -> Result<ItemMutationResult, Box<dyn std::error::Error>> {
    let path = find_config_path()?;
    if is_symlink_or_nix(&path) {
        return Ok(ItemMutationResult::NixManaged {
            config_path: path,
            instance_name: instance_name.to_string(),
            category,
            action: "update",
            name: item_name.to_string(),
            version,
        });
    }

    install_item(instance_name, category, item_name, version)
}

/// Creates a new instance entry in `instances.toml`.
pub fn create_instance(
    name: &str,
    version: &str,
    fabric: bool,
    username: Option<&str>,
) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let path = get_or_create_config_path()?;
    if is_symlink_or_nix(&path) {
        return Err(format!(
            "Configuration '{}' is managed by Nix. Declare instance '{name}' in your Nix flake/module instead.",
            path.display()
        ).into());
    }

    let content = fs::read_to_string(&path).unwrap_or_default();
    let mut doc: toml::Table = content.parse().unwrap_or_default();

    let instances = doc
        .entry("instance")
        .or_insert_with(|| toml::Value::Array(Vec::new()))
        .as_array_mut()
        .ok_or_else(|| "'instance' in configuration is not an array")?;

    if instances.iter().any(|i| i.get("name").and_then(|n| n.as_str()) == Some(name)) {
        return Err(format!("instance '{name}' already exists in {}", path.display()).into());
    }

    let mut new_inst = toml::map::Map::new();
    new_inst.insert("name".to_string(), toml::Value::String(name.to_string()));
    new_inst.insert("version".to_string(), toml::Value::String(version.to_string()));
    new_inst.insert("username".to_string(), toml::Value::String(username.unwrap_or("Player").to_string()));
    new_inst.insert("fabric".to_string(), toml::Value::Boolean(fabric));
    new_inst.insert("mods".to_string(), toml::Value::Array(Vec::new()));
    new_inst.insert("resourcepacks".to_string(), toml::Value::Array(Vec::new()));
    new_inst.insert("shaderpacks".to_string(), toml::Value::Array(Vec::new()));

    instances.push(toml::Value::Table(new_inst));

    fs::write(&path, toml::to_string_pretty(&doc)?)?;
    Ok(path)
}

/// Sets (or clears) the `modpack` field for a named instance in `instances.toml`.
/// If `url` is `None`, the field is removed (reverting to no modpack).
/// Returns `true` if the config is Nix-managed (read-only), `false` on success.
pub fn set_modpack(
    instance_name: &str,
    url: Option<&str>,
) -> Result<(PathBuf, bool), Box<dyn std::error::Error>> {
    let path = find_config_path()?;
    if is_symlink_or_nix(&path) {
        return Ok((path, true));
    }

    let content = fs::read_to_string(&path)?;
    let mut doc: toml::Table = content.parse()?;

    let instances = doc
        .get_mut("instance")
        .and_then(|v| v.as_array_mut())
        .ok_or_else(|| "no [[instance]] found in configuration".to_string())?;

    let instance = instances
        .iter_mut()
        .find(|tbl| tbl.get("name").and_then(|n| n.as_str()) == Some(instance_name))
        .ok_or_else(|| format!("instance '{instance_name}' not found in configuration"))?;

    let instance_tbl = instance
        .as_table_mut()
        .ok_or_else(|| format!("instance '{instance_name}' is not a table"))?;

    match url {
        Some(u) => {
            instance_tbl.insert("modpack".to_string(), toml::Value::String(u.to_string()));
        }
        None => {
            instance_tbl.remove("modpack");
        }
    }

    fs::write(&path, toml::to_string_pretty(&doc)?)?;
    Ok((path, false))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_package_entry_string() {
        let val = toml::Value::String("sodium".to_string());
        let entry = PackageEntry::parse(&val).expect("failed to parse string entry");
        assert_eq!(entry.name, "sodium");
        assert_eq!(entry.version, None);
    }

    #[test]
    fn test_parse_package_entry_table() {
        let mut map = toml::map::Map::new();
        map.insert("name".to_string(), toml::Value::String("iris".to_string()));
        map.insert("version".to_string(), toml::Value::String("1.7.0".to_string()));
        let val = toml::Value::Table(map);

        let entry = PackageEntry::parse(&val).expect("failed to parse table entry");
        assert_eq!(entry.name, "iris");
        assert_eq!(entry.version.as_deref(), Some("1.7.0"));
    }

    #[test]
    fn test_config_deserialize() {
        let toml_str = r#"
        [[instance]]
        name = "test"
        username = "Steve"
        version = "1.21.1"
        fabric = true
        memory = "8G"
        modpack = "https://example.com/pack.mrpack"
        mods = [
            "sodium",
            { name = "lithium", version = "0.12.0" }
        ]

        [instance.settings]
        vsync = false
        render_distance = 12

        [instance.mods_config.iris]
        shaders_enabled = true
        "#;

        let conf: Config = toml::from_str(toml_str).expect("failed to deserialize config");
        assert_eq!(conf.instance.len(), 1);
        let inst = &conf.instance[0];
        assert_eq!(inst.name, "test");
        assert_eq!(inst.username, "Steve");
        assert_eq!(inst.version, "1.21.1");
        assert!(inst.fabric);
        assert_eq!(inst.memory.as_deref(), Some("8G"));
        assert_eq!(inst.modpack.as_deref(), Some("https://example.com/pack.mrpack"));

        let parsed_mods = inst.parsed_mods();
        assert_eq!(parsed_mods.len(), 2);
        assert_eq!(parsed_mods[0].name, "sodium");
        assert_eq!(parsed_mods[0].version, None);
        assert_eq!(parsed_mods[1].name, "lithium");
        assert_eq!(parsed_mods[1].version.as_deref(), Some("0.12.0"));

        assert_eq!(inst.settings.get("vsync"), Some(&toml::Value::Boolean(false)));
    }
}
