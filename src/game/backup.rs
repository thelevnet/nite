//! Backup and restore utilities for Minecraft instances (worlds/saves and screenshots).

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Creates a tar.gz backup archive of saves/ and/or screenshots/ for an instance.
pub fn create_backup(
    instance_name: &str,
    include_saves: bool,
    include_screenshots: bool,
    output_path: Option<&str>,
) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let xdg = xdg::BaseDirectories::with_prefix("nite");
    let data_home = xdg.get_data_home().ok_or("Failed to resolve XDG data home")?;
    let instance_dir = data_home.join("instances").join(instance_name);

    if !instance_dir.exists() {
        return Err(format!("instance directory '{}' does not exist", instance_dir.display()).into());
    }

    let mut items_to_archive = Vec::new();
    if include_saves && instance_dir.join("saves").exists() {
        items_to_archive.push("saves");
    }
    if include_screenshots && instance_dir.join("screenshots").exists() {
        items_to_archive.push("screenshots");
    }

    if items_to_archive.is_empty() {
        return Err(format!("no saves or screenshots found in instance '{instance_name}' to backup").into());
    }

    let dest_archive = if let Some(out) = output_path {
        PathBuf::from(out)
    } else {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_secs();
        let backup_dir = data_home.join("backups");
        fs::create_dir_all(&backup_dir)?;
        backup_dir.join(format!("{instance_name}-backup-{timestamp}.tar.gz"))
    };

    if let Some(parent) = dest_archive.parent() {
        fs::create_dir_all(parent)?;
    }

    let mut cmd = Command::new("tar");
    cmd.arg("-czf")
        .arg(&dest_archive)
        .arg("-C")
        .arg(&instance_dir);

    for item in &items_to_archive {
        cmd.arg(item);
    }

    let status = cmd.status()?;
    if !status.success() {
        return Err(format!("tar command failed with status {status}").into());
    }

    Ok(dest_archive)
}

/// Restores/extracts worlds and screenshots from a backup archive into an instance.
pub fn restore_backup(
    instance_name: &str,
    archive_path: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let archive = Path::new(archive_path);
    if !archive.exists() {
        return Err(format!("backup archive file '{}' does not exist", archive.display()).into());
    }

    let xdg = xdg::BaseDirectories::with_prefix("nite");
    let data_home = xdg.get_data_home().ok_or("Failed to resolve XDG data home")?;
    let instance_dir = data_home.join("instances").join(instance_name);
    fs::create_dir_all(&instance_dir)?;

    let status = Command::new("tar")
        .arg("-xzf")
        .arg(archive)
        .arg("-C")
        .arg(&instance_dir)
        .status()?;

    if !status.success() {
        return Err(format!("tar extraction failed with status {status}").into());
    }

    Ok(())
}
