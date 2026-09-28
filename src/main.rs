//! Main entry point for the nite launcher.

pub mod auth;
pub mod cli;
pub mod config;
pub mod game;

use cli::{Action, BackupCommands, Cli, InstanceAction, ItemAction};
use config::{ItemCategory, ItemMutationResult};

fn main() {
    let action = match Cli::parse_action() {
        Ok(act) => act,
        Err(err) => {
            eprintln!();
            eprintln!("[nite] ERROR: {err}");
            eprintln!();
            std::process::exit(1);
        }
    };

    let result = match action {
        Action::Run { instance } => game::launch_instance(&instance),
        Action::New { name, version, username, fabric } => handle_new_instance(&name, &version, username.as_deref(), fabric),
        Action::Backup(backup_cmd) => handle_backup(backup_cmd),
        Action::List => game::list_instances(),
        Action::Clean => game::clean_unused_data(),
        Action::Auth => auth::login().map(|_| ()),
        Action::Search { query } => handle_search(&query),
        Action::InstanceAction { instance, action } => handle_instance_action(&instance, action),
    };

    if let Err(err) = result {
        eprintln!();
        eprintln!("[nite] ERROR: {err}");
        eprintln!();
        std::process::exit(1);
    }
}

fn handle_new_instance(
    name: &str,
    version: &str,
    username: Option<&str>,
    fabric: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let path = config::create_instance(name, version, fabric, username)?;
    println!("[nite] Created new instance '{name}' (Minecraft {version}, Fabric: {fabric}) in {}", path.display());
    println!("[nite] Run 'nite run {name}' to launch.");
    Ok(())
}

fn handle_backup(cmd: BackupCommands) -> Result<(), Box<dyn std::error::Error>> {
    match cmd {
        BackupCommands::Create { instance, output, worlds_only, screenshots_only } => {
            let include_saves = !screenshots_only;
            let include_screenshots = !worlds_only;
            let dest = game::backup::create_backup(&instance, include_saves, include_screenshots, output.as_deref())?;
            println!("[nite] Backup created at: {}", dest.display());
            Ok(())
        }
        BackupCommands::Restore { instance, archive } => {
            game::backup::restore_backup(&instance, &archive)?;
            println!("[nite] Restored backup '{}' into instance '{instance}'", archive);
            Ok(())
        }
    }
}

fn handle_search(query: &str) -> Result<(), Box<dyn std::error::Error>> {
    let hits = game::modrinth::search_mods(query)?;
    if hits.is_empty() {
        println!("[nite] No mods found matching '{query}'.");
        return Ok(());
    }

    println!("Found {} mod(s) matching '{query}':\n", hits.len());
    for hit in hits {
        let versions_str = if hit.versions.is_empty() {
            "none".to_string()
        } else {
            let sample: Vec<&str> = hit.versions.iter().rev().take(4).map(|s| s.as_str()).collect();
            sample.join(", ")
        };
        let author = hit.author.as_deref().unwrap_or("unknown");

        println!("  • {} ({}) by {}", hit.title, hit.slug, author);
        println!("    Description: {}", hit.description.trim());
        println!("    Latest versions: {versions_str}");
        println!();
    }
    Ok(())
}

fn handle_instance_action(instance: &str, action: InstanceAction) -> Result<(), Box<dyn std::error::Error>> {
    match action {
        InstanceAction::Mods { action } => handle_item_action(instance, ItemCategory::Mods, action),
        InstanceAction::Resourcepacks { action } => handle_item_action(instance, ItemCategory::ResourcePacks, action),
        InstanceAction::Shaders { action } => handle_item_action(instance, ItemCategory::Shaders, action),
        InstanceAction::Modpack { source } => {
            let (path, nix_managed) = config::set_modpack(instance, Some(&source))?;
            if nix_managed {
                println!("[nite] Notice: '{}' is managed by Nix (symlink / read-only store path).", path.display());
                println!("[nite] Add the modpack to instance '{instance}' in your Nix configuration:");
                println!();
                println!("    modpack = \"{source}\";");
            } else {
                println!("[nite] Set modpack for instance '{instance}' in {}", path.display());
                println!("[nite] Run 'nite run {instance}' to install and launch.");
            }
            Ok(())
        }
        InstanceAction::Update => handle_batch_update(instance),
        InstanceAction::Install { mod_slug, version } => {
            handle_item_action(instance, ItemCategory::Mods, ItemAction::Install { name: mod_slug, version })
        }
        InstanceAction::Remove { mod_slug } => {
            handle_item_action(instance, ItemCategory::Mods, ItemAction::Remove { name: mod_slug })
        }
    }
}

fn handle_item_action(
    instance: &str,
    category: ItemCategory,
    action: ItemAction,
) -> Result<(), Box<dyn std::error::Error>> {
    let conf = config::load_config()?;
    let inst = conf.instance.iter().find(|i| i.name == instance)
        .ok_or_else(|| format!("instance '{instance}' not found in configuration"))?;

    let is_mod = category == ItemCategory::Mods;

    match action {
        ItemAction::Install { name, version } => {
            let resolved_ver = game::modrinth::resolve_item_version(&name, is_mod, Some(&inst.version), version.as_deref())?;
            let res = config::install_item(instance, category, &name, Some(resolved_ver.clone()))?;
            display_mutation_result(res)
        }
        ItemAction::Remove { name } => {
            let res = config::remove_item(instance, category, &name)?;
            display_mutation_result(res)
        }
        ItemAction::Update { name, version } => {
            let resolved_ver = game::modrinth::resolve_item_version(&name, is_mod, Some(&inst.version), version.as_deref())?;
            let res = config::update_item(instance, category, &name, Some(resolved_ver.clone()))?;
            display_mutation_result(res)
        }
    }
}

fn handle_batch_update(instance: &str) -> Result<(), Box<dyn std::error::Error>> {
    let conf = config::load_config()?;
    let inst = conf.instance.iter().find(|i| i.name == instance)
        .ok_or_else(|| format!("instance '{instance}' not found in configuration"))?;

    println!("[nite] Checking updates for instance '{instance}' (Minecraft {})...", inst.version);

    let mod_updates = game::modrinth::check_package_updates(&inst.parsed_mods(), true, &inst.version);
    let rp_updates = game::modrinth::check_package_updates(&inst.parsed_resourcepacks(), false, &inst.version);
    let sp_updates = game::modrinth::check_package_updates(&inst.parsed_shaderpacks(), false, &inst.version);

    let mut has_updates = false;

    println!("\nMods:");
    for (name, current, latest, is_up) in &mod_updates {
        if *is_up {
            has_updates = true;
            println!("  ↑ {name}: {current} -> {latest}");
        } else {
            println!("  ✓ {name}: {latest}");
        }
    }

    if !rp_updates.is_empty() {
        println!("\nResource Packs:");
        for (name, current, latest, is_up) in &rp_updates {
            if *is_up {
                has_updates = true;
                println!("  ↑ {name}: {current} -> {latest}");
            } else {
                println!("  ✓ {name}: {latest}");
            }
        }
    }

    if !sp_updates.is_empty() {
        println!("\nShader Packs:");
        for (name, current, latest, is_up) in &sp_updates {
            if *is_up {
                has_updates = true;
                println!("  ↑ {name}: {current} -> {latest}");
            } else {
                println!("  ✓ {name}: {latest}");
            }
        }
    }

    if has_updates {
        println!("\nTo update pinned versions, run 'nite {instance} mods update <name> -v <version>'.");
    } else {
        println!("\nAll packages are up to date.");
    }

    Ok(())
}

fn display_mutation_result(res: ItemMutationResult) -> Result<(), Box<dyn std::error::Error>> {
    match res {
        ItemMutationResult::Mutated { config_path, instance_name, category, action, name, version } => {
            let cat_name = category.display_name();
            let action_verb = match action {
                "install" => "Installed",
                "remove" => "Removed",
                "update" => "Updated",
                _ => "Modified",
            };
            if let Some(ref ver) = version {
                println!("[nite] {action_verb} {cat_name} '{name}' (version {ver}) for instance '{instance_name}' in {}", config_path.display());
            } else {
                println!("[nite] {action_verb} {cat_name} '{name}' for instance '{instance_name}' in {}", config_path.display());
            }
            println!("[nite] Run 'nite run {instance_name}' to synchronize and launch.");
        }
        ItemMutationResult::NixManaged { config_path, instance_name, category, action, name, version } => {
            let cat_key = category.config_key();
            let cat_name = category.display_name();
            println!("[nite] Notice: '{}' is managed by Nix (symlink / read-only store path).", config_path.display());
            match action {
                "install" => {
                    println!("[nite] Add {cat_name} '{name}' to instance '{instance_name}' in your Nix configuration:");
                    println!();
                    println!("    {cat_key} = [");
                    println!("      # ...");
                    if let Some(ref ver) = version {
                        println!("      {{ name = \"{name}\"; version = \"{ver}\"; }},");
                    } else {
                        println!("      \"{name}\",");
                    }
                    println!("    ];");
                }
                "remove" => {
                    println!("[nite] Remove {cat_name} '{name}' from instance '{instance_name}' in your Nix configuration.");
                }
                "update" => {
                    println!("[nite] {cat_name} '{name}' is declared in instance '{instance_name}'.");
                    if let Some(ref ver) = version {
                        println!("       Target version resolved: {ver}");
                        println!("       Update {{ name = \"{name}\"; version = \"{ver}\"; }} in your Nix configuration.");
                    }
                }
                _ => {}
            }
        }
    }
    Ok(())
}
