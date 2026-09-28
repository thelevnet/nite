//! Java detection and Minecraft client process execution.

use std::fs::File;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Extracts the required major Java version from Mojang's version.json (`javaVersion.majorVersion`).
pub fn extract_required_java_version(version_json: &Path) -> Option<u32> {
    let file = File::open(version_json).ok()?;
    let root: serde_json::Value = serde_json::from_reader(file).ok()?;
    root.get("javaVersion")
        .and_then(|jv| jv.get("majorVersion"))
        .and_then(|mv| mv.as_u64())
        .map(|v| v as u32)
}

/// Resolves the optimal Java binary for the required major version.
pub fn resolve_java_binary(
    java_override: Option<&str>,
    required_major: Option<u32>,
) -> Result<String, Box<dyn std::error::Error>> {
    // 1. Explicit override from instance configuration
    if let Some(explicit) = java_override {
        return Ok(explicit.to_string());
    }

    // 2. Explicit environment variables
    if let Ok(bin) = std::env::var("JAVA_BIN") {
        return Ok(bin);
    }
    if let Ok(home) = std::env::var("JAVA_HOME") {
        let candidate = PathBuf::from(home).join("bin").join("java");
        if candidate.is_file() {
            return Ok(candidate.to_string_lossy().into_owned());
        }
    }

    // 3. If version-specific Java is needed, check for standard versioned paths
    if let Some(major) = required_major {
        let version_candidates = [
            format!("/run/current-system/sw/lib/openjdk-{major}/bin/java"),
            format!("/usr/lib/jvm/java-{major}-openjdk/bin/java"),
            format!("/usr/lib/jvm/temurin-{major}-jdk/bin/java"),
        ];
        for candidate in version_candidates {
            if Path::new(&candidate).exists() {
                return Ok(candidate);
            }
        }
    }

    // 4. PATH search
    let path_env = std::env::var("PATH").unwrap_or_default();
    for dir in path_env.split(':') {
        let candidate = PathBuf::from(dir).join("java");
        if candidate.is_file() {
            return Ok(candidate.to_string_lossy().into_owned());
        }
    }

    // 5. NixOS system fallback
    let nix_standard = "/run/current-system/sw/bin/java";
    if Path::new(nix_standard).exists() {
        return Ok(nix_standard.to_string());
    }

    Err("Java executable not found. Ensure Java is installed or set 'java' in instances.toml or JAVA_BIN/JAVA_HOME.".into())
}

#[allow(clippy::too_many_arguments)]
pub fn spawn_game_process(
    version_json: &Path,
    version_id: &str,
    client_jar: &Path,
    classpath_jars: &[PathBuf],
    instance_dir: &Path,
    assets_dir: &Path,
    natives_dir: &Path,
    username: &str,
    uuid: &str,
    access_token: &str,
    user_type: &str,
    memory_limit: &str,
    java_override: Option<&str>,
    main_class_override: Option<&str>,
    extra_jvm_flags: &[String],
) -> Result<(), Box<dyn std::error::Error>> {
    let file = File::open(version_json)?;
    let root: serde_json::Value = serde_json::from_reader(file)?;

    let default_main_class = root["mainClass"]
        .as_str()
        .ok_or("Missing mainClass in version metadata")?;
    let main_class = main_class_override.unwrap_or(default_main_class);

    let asset_index_id = root["assetIndex"]["id"]
        .as_str()
        .ok_or("Missing assetIndex.id in version metadata")?;

    let mut cp_strings: Vec<String> = vec![client_jar.to_string_lossy().into_owned()];
    for jar in classpath_jars {
        cp_strings.push(jar.to_string_lossy().into_owned());
    }
    let classpath = cp_strings.join(":");

    let required_java = extract_required_java_version(version_json);
    if let Some(req) = required_java {
        println!("[nite] Minecraft {version_id} requires Java {req}");
    }

    let java_executable = resolve_java_binary(java_override, required_java)?;

    let mut cmd = Command::new(&java_executable);
    cmd.arg(format!("-Xms{memory_limit}"))
        .arg(format!("-Xmx{memory_limit}"));

    for flag in extra_jvm_flags {
        cmd.arg(flag);
    }

    let mut native_paths = vec![natives_dir.to_string_lossy().into_owned()];
    if let Ok(ld) = std::env::var("LD_LIBRARY_PATH") {
        native_paths.push(ld);
    }
    cmd.arg(format!("-Djava.library.path={}", native_paths.join(":")));

    let nix_openal = std::env::var("NIX_OPENAL_LIB")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("/run/current-system/sw/lib/libopenal.so"));

    if nix_openal.exists() {
        cmd.arg(format!("-Dorg.lwjgl.openal.libname={}", nix_openal.display()));
    }

    cmd.arg("-cp").arg(classpath).arg(main_class)
        .arg("--version").arg(version_id)
        .arg("--gameDir").arg(instance_dir)
        .arg("--assetsDir").arg(assets_dir)
        .arg("--assetIndex").arg(asset_index_id)
        .arg("--username").arg(username)
        .args([
            "--uuid",
            uuid,
            "--accessToken",
            access_token,
            "--userType",
            user_type,
            "--versionType",
            "release",
        ]);

    let err = cmd.exec();
    Err(format!("failed to execute '{java_executable}': {err}").into())
}
