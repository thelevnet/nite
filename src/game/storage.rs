//! Storage layout and filesystem path resolution for nite.

use crate::config::Instance;
use std::fs;
use std::path::PathBuf;

pub struct StoragePaths {
    pub instance_dir: PathBuf,
    pub libs_dir: PathBuf,
    pub natives_dir: PathBuf,
    pub assets_dir: PathBuf,
    pub version_json: PathBuf,
    pub client_jar: PathBuf,
}

impl StoragePaths {
    pub fn resolve(instance: &Instance) -> Result<Self, Box<dyn std::error::Error>> {
        let xdg = xdg::BaseDirectories::with_prefix("nite");
        let data_home = xdg.get_data_home().ok_or("Failed to resolve XDG data home")?;

        let instance_dir = data_home.join("instances").join(&instance.name);
        let shared_dir = data_home.join("shared");
        let libs_dir = shared_dir.join("libraries");
        let version_dir = shared_dir.join("versions").join(&instance.version);
        let natives_dir = version_dir.join("natives");
        let assets_dir = shared_dir.join("assets");
        let client_jar = version_dir.join("client.jar");
        let version_json = version_dir.join(format!("{}.json", instance.version));

        let required_dirs = [
            &instance_dir,
            &shared_dir,
            &libs_dir,
            &version_dir,
            &natives_dir,
            &assets_dir.join("indexes"),
            &assets_dir.join("objects"),
        ];

        for dir in required_dirs {
            fs::create_dir_all(dir)?;
        }

        Ok(Self {
            instance_dir,
            libs_dir,
            natives_dir,
            assets_dir,
            version_json,
            client_jar,
        })
    }
}
