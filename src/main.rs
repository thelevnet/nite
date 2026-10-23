//! Pure declarative Minecraft launcher for NixOS.

pub mod lock;
pub mod servers;

use clap::{Parser, Subcommand};
use serde::Deserialize;
use std::fs::{self, File};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use uuid::Uuid;

#[derive(Parser, Debug)]
#[command(
    name = "nite",
    version,
    about = "Pure declarative Minecraft instance launcher for NixOS",
    subcommand_required = false
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    /// Quick instance run shortcut (e.g. 'nite default')
    instance_name: Option<String>,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Launch a declared Minecraft instance inside bubblewrap sandbox
    Run {
        /// Name of the instance declared in Nix
        instance: String,
    },
    /// List configured instances
    List,
    /// Refresh and lock mod/version dependencies into nite.lock
    Lock {
        /// Output path for the lock file
        #[arg(short, long, default_value = "nite.lock")]
        output: String,
    },
    /// Internal helper to compile server definitions to binary servers.dat
    FormatServers {
        /// Input JSON file containing server definitions
        #[arg(short, long)]
        input: String,
        /// Output binary servers.dat path
        #[arg(short, long)]
        output: String,
    },
}

#[derive(Deserialize, Debug)]
pub struct InstanceManifest {
    pub name: String,
    pub version: String,
    pub username: String,
    #[serde(default)]
    pub uuid: Option<String>,
    pub java: PathBuf,
    pub main_class: String,
    pub classpath: Vec<PathBuf>,
    pub game_dir: PathBuf,
    pub assets_dir: PathBuf,
    pub asset_index: String,
    #[serde(default = "default_memory")]
    pub memory: String,
    #[serde(default)]
    pub jvm_flags: Vec<String>,
    #[serde(default)]
    pub bwrap_bin: Option<PathBuf>,
    #[serde(default)]
    pub sandbox_lib: Option<PathBuf>,
}

fn default_memory() -> String {
    "4G".to_string()
}

fn main() {
    let cli = Cli::parse();

    let result = match cli.command {
        Some(Commands::Run { instance }) => launch_instance(&instance),
        Some(Commands::List) => list_instances(),
        Some(Commands::Lock { output }) => run_lock(&output),
        Some(Commands::FormatServers { input, output }) => run_format_servers(&input, &output),
        None => {
            if let Some(name) = cli.instance_name {
                launch_instance(&name)
            } else {
                list_instances()
            }
        }
    };

    if let Err(e) = result {
        eprintln!("\n[nite] ERROR: {e}\n");
        std::process::exit(1);
    }
}

fn find_manifest(instance: &str) -> Result<PathBuf, Box<dyn std::error::Error>> {
    if let Ok(explicit) = std::env::var("NITE_INSTANCE_MANIFEST") {
        let p = PathBuf::from(explicit);
        if p.exists() {
            return Ok(p);
        }
    }

    let xdg = xdg::BaseDirectories::with_prefix("nite");
    let mut candidates = Vec::new();

    if let Some(data) = xdg.get_data_home() {
        candidates.push(data.join("instances").join(format!("{instance}.json")));
    }
    candidates.push(PathBuf::from("/etc/nite/instances").join(format!("{instance}.json")));
    candidates.push(PathBuf::from("instances").join(format!("{instance}.json")));

    for c in candidates {
        if c.exists() {
            return Ok(c);
        }
    }

    Err(format!(
        "instance '{instance}' manifest not found. Make sure it is declared in your Nix configuration."
    ).into())
}

fn generate_offline_uuid(username: &str) -> String {
    let ns = Uuid::nil();
    let name_bytes = format!("OfflinePlayer:{username}");
    Uuid::new_v3(&ns, name_bytes.as_bytes()).to_string()
}

fn which_bwrap() -> Option<PathBuf> {
    if let Ok(path) = std::env::var("PATH") {
        for dir in path.split(':') {
            let p = PathBuf::from(dir).join("bwrap");
            if p.is_file() {
                return Some(p);
            }
        }
    }
    None
}

fn launch_instance(instance: &str) -> Result<(), Box<dyn std::error::Error>> {
    let manifest_path = find_manifest(instance)?;
    let file = File::open(&manifest_path)?;
    let manifest: InstanceManifest = serde_json::from_reader(file)?;

    println!(
        "[nite] launching pure instance '{}' (Minecraft {}, user '{}')...",
        manifest.name, manifest.version, manifest.username
    );

    let uuid = manifest.uuid.unwrap_or_else(|| generate_offline_uuid(&manifest.username));

    // Resolve host mutable data storage directory
    let xdg = xdg::BaseDirectories::with_prefix("nite");
    let data_root = xdg.get_data_home()
        .unwrap_or_else(|| PathBuf::from(std::env::var("HOME").unwrap_or_default()).join(".local/share/nite"))
        .join("data")
        .join(&manifest.name);

    let mutable_subdirs = [
        "saves",
        "screenshots",
        "logs",
        "xaero",
        "XaeroWorldMap",
        "XaeroWaypoints",
        ".fabric",
        "CustomSkinLoader",
    ];

    for sub in &mutable_subdirs {
        let p = data_root.join(sub);
        fs::create_dir_all(&p)?;
    }

    let bwrap_executable = manifest.bwrap_bin
        .or_else(|| std::env::var("NITE_BWRAP_BIN").ok().map(PathBuf::from))
        .or_else(which_bwrap)
        .unwrap_or_else(|| PathBuf::from("/run/current-system/sw/bin/bwrap"));

    let sandbox_lib = manifest.sandbox_lib
        .or_else(|| std::env::var("NITE_SANDBOX_LIB").ok().map(PathBuf::from))
        .unwrap_or_else(|| PathBuf::from("/run/current-system/sw/lib/libnite_sandbox.so"));

    let cp_strings: Vec<String> = manifest.classpath.iter().map(|p| p.to_string_lossy().into_owned()).collect();
    let classpath_joined = cp_strings.join(":");

    let target_game_dir = PathBuf::from("/tmp/nite-game");

    let mut cmd = Command::new(&bwrap_executable);
    cmd.arg("--ro-bind").arg("/").arg("/")
        .arg("--dev").arg("/dev")
        .arg("--proc").arg("/proc")
        .arg("--tmpfs").arg("/tmp")
        .arg("--dir").arg(&target_game_dir);

    // Ro-bind all entries from the Nix store game_dir into the sandbox target_game_dir
    if let Ok(entries) = fs::read_dir(&manifest.game_dir) {
        for entry in entries.flatten() {
            let p = entry.path();
            if let Some(file_name) = p.file_name() {
                let guest_target = target_game_dir.join(file_name);
                cmd.arg("--ro-bind").arg(&p).arg(&guest_target);
            }
        }
    }

    // Bind all mutable directories onto the sandbox target_game_dir
    for sub in &mutable_subdirs {
        let host_p = data_root.join(sub);
        let guest_p = target_game_dir.join(sub);
        cmd.arg("--bind").arg(&host_p).arg(&guest_p);
    }

    let assets_dir = if manifest.assets_dir.exists() {
        manifest.assets_dir
    } else if let Some(data) = xdg.get_data_home() {
        let p = data.join("shared").join("assets");
        if p.exists() {
            p
        } else {
            manifest.assets_dir
        }
    } else {
        manifest.assets_dir
    };

    if sandbox_lib.exists() {
        cmd.env("LD_PRELOAD", &sandbox_lib);
    }
    cmd.env("NITE_GAME_DIR", &target_game_dir);
    cmd.env("NITE_MUTABLE_DIRS", "saves:screenshots:logs:xaero:XaeroWorldMap:XaeroWaypoints:.fabric:CustomSkinLoader");

    // Pass Java command line inside bubblewrap
    cmd.arg(&manifest.java)
        .arg(format!("-Xms{}", manifest.memory))
        .arg(format!("-Xmx{}", manifest.memory));

    for flag in &manifest.jvm_flags {
        cmd.arg(flag);
    }

    cmd.arg("-cp").arg(classpath_joined)
        .arg(&manifest.main_class)
        .arg("--version").arg(&manifest.version)
        .arg("--gameDir").arg(&target_game_dir)
        .arg("--assetsDir").arg(&assets_dir)
        .arg("--assetIndex").arg(&manifest.asset_index)
        .arg("--username").arg(&manifest.username)
        .args([
            "--uuid",
            &uuid,
            "--accessToken",
            "0",
            "--userType",
            "legacy",
            "--versionType",
            "release",
        ]);

    let err = cmd.exec();
    Err(format!("failed to execute bubblewrap '{:?}': {}", bwrap_executable, err).into())
}

fn list_instances() -> Result<(), Box<dyn std::error::Error>> {
    let xdg = xdg::BaseDirectories::with_prefix("nite");
    let mut found = Vec::new();

    let dirs = [
        xdg.get_data_home().map(|p| p.join("instances")),
        Some(PathBuf::from("/etc/nite/instances")),
    ];

    for d in dirs.into_iter().flatten() {
        if let Ok(entries) = fs::read_dir(d) {
            for entry in entries.flatten() {
                let p = entry.path();
                if p.extension().and_then(|e| e.to_str()) == Some("json") {
                    if let Some(stem) = p.file_stem().and_then(|s| s.to_str()) {
                        if !found.contains(&stem.to_string()) {
                            found.push(stem.to_string());
                        }
                    }
                }
            }
        }
    }

    if found.is_empty() {
        println!("No instances configured. Declare them in your Nix configuration.");
    } else {
        println!("Configured Minecraft instances:");
        for name in found {
            println!("  • {name}");
        }
    }
    Ok(())
}

fn run_lock(output: &str) -> Result<(), Box<dyn std::error::Error>> {
    let specs = lock::discover_specs();
    lock::lock_instances(&specs, Path::new(output))
}

fn run_format_servers(input: &str, output: &str) -> Result<(), Box<dyn std::error::Error>> {
    let content = fs::read_to_string(input)?;
    let servers: Vec<servers::ServerEntry> = serde_json::from_str(&content)?;
    let bytes = servers::encode_servers_dat(&servers);
    fs::write(output, bytes)?;
    println!("[nite] wrote binary servers.dat to {output}");
    Ok(())
}
