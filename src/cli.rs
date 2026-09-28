use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(
    name = "nite",
    version,
    about = "Declarative Minecraft instance launcher for NixOS",
    subcommand_required = true,
    arg_required_else_help = true
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Launch a Minecraft instance
    Run {
        /// Name of the instance declared in instances.toml
        instance: String,
    },
    /// Create a new Minecraft instance
    New {
        /// Name of the new instance
        name: String,
        /// Minecraft version (e.g. "1.21.1", "26.1.2")
        #[arg(short, long, default_value = "1.21.1")]
        version: String,
        /// In-game username
        #[arg(short, long)]
        username: Option<String>,
        /// Whether to install Fabric Loader
        #[arg(long, default_value_t = true)]
        fabric: bool,
    },
    /// Manage backups (worlds and screenshots)
    Backup {
        #[command(subcommand)]
        action: BackupCommands,
    },
    /// Search for mods on Modrinth
    Search {
        /// Mod query to search for
        query: String,
    },
    /// List configured instances
    List,
    /// Remove unused version binaries and unreferenced instance folders
    Clean,
    /// Authenticate a Microsoft / Xbox Minecraft account via Device Flow
    Auth,
    /// Manage a specific Minecraft instance (mods, resourcepacks, shaders, modpack, update)
    #[command(external_subcommand)]
    Instance(Vec<String>),
}

#[derive(Subcommand, Debug, Clone)]
pub enum BackupCommands {
    /// Create a backup of instance worlds and/or screenshots
    Create {
        /// Name of the instance
        instance: String,
        /// Destination archive file (.tar.gz)
        #[arg(short, long)]
        output: Option<String>,
        /// Backup only saves/worlds
        #[arg(long)]
        worlds_only: bool,
        /// Backup only screenshots
        #[arg(long)]
        screenshots_only: bool,
    },
    /// Restore worlds/screenshots from a backup archive (.tar.gz) into an instance
    Restore {
        /// Name of the target instance
        instance: String,
        /// Path to the backup archive file (.tar.gz)
        archive: String,
    },
}

#[derive(Parser, Debug)]
pub struct InstanceSubcommands {
    #[command(subcommand)]
    pub action: InstanceAction,
}

#[derive(Subcommand, Debug, Clone)]
pub enum InstanceAction {
    /// Manage mods for the instance
    Mods {
        #[command(subcommand)]
        action: ItemAction,
    },
    /// Manage resource packs for the instance
    Resourcepacks {
        #[command(subcommand)]
        action: ItemAction,
    },
    /// Manage shaders for the instance
    Shaders {
        #[command(subcommand)]
        action: ItemAction,
    },
    /// Declare a .mrpack modpack for this instance (URL or local file path).
    /// The modpack is written to instances.toml and installed automatically on `nite run`.
    /// Changing the URL on the next run will trigger a reinstall.
    Modpack {
        /// URL or local path to the .mrpack file
        source: String,
    },
    /// Check and batch update all mods, resource packs, and shaders
    Update,

    // Backward compatibility shortcuts directly under instance:
    // `nite <instance> install <mod>` etc.
    #[command(hide = true)]
    Install {
        mod_slug: String,
        #[arg(short, long)]
        version: Option<String>,
    },
    #[command(hide = true)]
    Remove {
        mod_slug: String,
    },
}

#[derive(Subcommand, Debug, Clone)]
pub enum ItemAction {
    /// Install an item to the instance
    Install {
        /// Slug on Modrinth
        name: String,
        /// Specific version to install or pin
        #[arg(short, long)]
        version: Option<String>,
    },
    /// Remove an item from the instance
    Remove {
        /// Slug on Modrinth
        name: String,
    },
    /// Update an item on the instance
    Update {
        /// Slug on Modrinth
        name: String,
        /// Target version to update or pin
        #[arg(short, long)]
        version: Option<String>,
    },
}

#[derive(Debug)]
pub enum Action {
    Run { instance: String },
    New {
        name: String,
        version: String,
        username: Option<String>,
        fabric: bool,
    },
    Backup(BackupCommands),
    Search { query: String },
    List,
    Clean,
    Auth,
    InstanceAction {
        instance: String,
        action: InstanceAction,
    },
}

impl Cli {
    pub fn parse_action() -> Result<Action, Box<dyn std::error::Error>> {
        let cli = Cli::parse();
        match cli.command {
            Commands::Run { instance } => Ok(Action::Run { instance }),
            Commands::New { name, version, username, fabric } => {
                Ok(Action::New { name, version, username, fabric })
            }
            Commands::Backup { action } => Ok(Action::Backup(action)),
            Commands::Search { query } => Ok(Action::Search { query }),
            Commands::List => Ok(Action::List),
            Commands::Clean => Ok(Action::Clean),
            Commands::Auth => Ok(Action::Auth),
            Commands::Instance(args) => {
                if args.is_empty() {
                    return Err("missing instance command. Run 'nite --help' for usage.".into());
                }
                let instance_name = args[0].clone();
                let sub_args = &args[1..];

                if sub_args.is_empty() {
                    let config_res = crate::config::load_config();
                    let is_known_instance = config_res
                        .as_ref()
                        .map(|c| c.instance.iter().any(|i| i.name == instance_name))
                        .unwrap_or(false);

                    if !is_known_instance {
                        if ["install", "remove", "update", "mods", "resourcepacks", "shaders", "modpack"].contains(&instance_name.as_str()) {
                            return Err(format!(
                                "syntax is 'nite <instance> {instance_name} ...'\nExample: nite default mods install <mod>"
                            ).into());
                        } else {
                            return Err(format!(
                                "unrecognized command or instance '{instance_name}'. Run 'nite --help' for usage."
                            ).into());
                        }
                    }

                    use clap::CommandFactory;
                    let mut cmd = InstanceSubcommands::command();
                    cmd.set_bin_name(format!("nite {instance_name}"));
                    cmd.print_help()?;
                    println!();
                    std::process::exit(0);
                }

                if sub_args[0] == "run" {
                    return Err(format!("'run' is a top-level command. Use 'nite run {instance_name}' instead.").into());
                }

                let mut full_sub_args = vec![format!("nite {instance_name}")];
                full_sub_args.extend_from_slice(sub_args);

                let parsed = InstanceSubcommands::try_parse_from(&full_sub_args)?;
                Ok(Action::InstanceAction {
                    instance: instance_name,
                    action: parsed.action,
                })
            }
        }
    }
}
