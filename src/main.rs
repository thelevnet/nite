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
        /// Optional input JSON or Nix file containing instance specifications
        #[arg(short, long)]
        input: Option<String>,
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
        Some(Commands::Lock { input, output }) => run_lock(input.as_deref(), &output),
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

fn bind_tree_as_files(src: &Path, dst: &Path, cmd: &mut Command) {
    if let Ok(entries) = fs::read_dir(src) {
        for entry in entries.flatten() {
            let src_entry = entry.path();
            if let Some(file_name) = src_entry.file_name() {
                let dst_entry = dst.join(file_name);
                if src_entry.is_dir() {
                    cmd.arg("--dir").arg(&dst_entry);
                    bind_tree_as_files(&src_entry, &dst_entry, cmd);
                } else {
                    cmd.arg("--ro-bind").arg(&src_entry).arg(&dst_entry);
                }
            }
        }
    }
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
        .arg("--dev-bind-try").arg("/dev/dri").arg("/dev/dri")
        .arg("--dev-bind-try").arg("/dev/input").arg("/dev/input")
        .arg("--proc").arg("/proc")
        .arg("--tmpfs").arg("/tmp")
        .arg("--dir").arg(&target_game_dir)
        .arg("--chdir").arg(&target_game_dir);

    // Bind XDG_RUNTIME_DIR writable for native Wayland sockets, shm, and audio
    if let Ok(runtime_dir) = std::env::var("XDG_RUNTIME_DIR") {
        cmd.arg("--bind-try").arg(&runtime_dir).arg(&runtime_dir);
    } else {
        unsafe extern "C" {
            fn getuid() -> u32;
        }
        let uid = unsafe { getuid() };
        let run_user = format!("/run/user/{uid}");
        cmd.arg("--bind-try").arg(&run_user).arg(&run_user);
    }

    // Export pure Wayland native environment variables into sandbox
    if let Ok(wayland_display) = std::env::var("WAYLAND_DISPLAY") {
        cmd.env("WAYLAND_DISPLAY", wayland_display);
    }
    cmd.env("XDG_SESSION_TYPE", "wayland");
    cmd.env("GDK_BACKEND", "wayland");
    cmd.env("QT_QPA_PLATFORM", "wayland");
    cmd.env("SDL_VIDEODRIVER", "wayland");
    cmd.env("CLUTTER_BACKEND", "wayland");

    let shared_assets = xdg.get_data_home()
        .unwrap_or_else(|| PathBuf::from(std::env::var("HOME").unwrap_or_default()).join(".local/share/nite"))
        .join("shared")
        .join("assets");
    let _ = fs::create_dir_all(shared_assets.join("indexes"));
    let _ = fs::create_dir_all(shared_assets.join("objects"));

    let assets_dir = if manifest.assets_dir.exists() {
        manifest.assets_dir
    } else {
        shared_assets
    };

    if assets_dir.exists() {
        cmd.arg("--bind-try").arg(&assets_dir).arg(&assets_dir);
    }

    // Directories whose files should be individual ro-bind mounts to avoid exposing Nix store symlinks
    let individual_bind_dirs = ["config", "mods", "resourcepacks", "shaderpacks"];

    // Ro-bind all entries from the Nix store game_dir into the sandbox target_game_dir
    if let Ok(entries) = fs::read_dir(&manifest.game_dir) {
        for entry in entries.flatten() {
            let p = entry.path();
            if let Some(file_name) = p.file_name() {
                let name_str = file_name.to_string_lossy();
                if individual_bind_dirs.contains(&name_str.as_ref()) {
                    let guest_sub = target_game_dir.join(file_name);
                    cmd.arg("--dir").arg(&guest_sub);
                    bind_tree_as_files(&p, &guest_sub, &mut cmd);
                } else if mutable_subdirs.contains(&name_str.as_ref()) {
                    // Skip mutable directories as they will be bound from data_root below
                    continue;
                } else {
                    let guest_target = target_game_dir.join(file_name);
                    cmd.arg("--ro-bind").arg(&p).arg(&guest_target);
                }
            }
        }
    }

    // Bind all mutable directories onto the sandbox target_game_dir
    for sub in &mutable_subdirs {
        let host_p = data_root.join(sub);
        let guest_p = target_game_dir.join(sub);
        cmd.arg("--bind").arg(&host_p).arg(&guest_p);
    }

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

fn run_lock(input: Option<&str>, output: &str) -> Result<(), Box<dyn std::error::Error>> {
    let specs = match input {
        Some(inp) => lock::load_specs_from_path(Path::new(inp))?,
        None => lock::discover_specs(),
    };
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
