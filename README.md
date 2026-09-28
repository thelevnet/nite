# nite

A Minecraft launcher for NixOS.

`nite` configures instances, mods, resource packs, shaders, and game settings using a TOML file or a Home Manager module. It resolves dependencies and links native libraries without requiring `nix-ld` or FHS environments.

## Table of Contents

- [Installation](#installation)
- [Usage](#usage)
  - [Home Manager](#home-manager)
  - [Standalone Configuration](#standalone-configuration)
- [Configuration Reference](#configuration-reference)
  - [Supported Versions](#supported-versions)
- [CLI Reference](#cli-reference)
- [Building from Source](#building-from-source)
- [Contributing](#contributing)

---

## Installation

Run `nite` directly:

```bash
nix run github:thelevnet/nite -- <command>
```

Add `nite` to your system packages in `flake.nix`:

```nix
{
  inputs.nite.url = "github:thelevnet/nite";

  outputs = { self, nixpkgs, nite, ... }: {
    nixosConfigurations.myHost = nixpkgs.lib.nixosSystem {
      system = "x86_64-linux";
      modules = [
        ({ pkgs, ... }: {
          environment.systemPackages = [ nite.packages.${pkgs.system}.default ];
        })
      ];
    };
  };
}
```

---

## Usage

You can configure `nite` using the Home Manager module or a standalone TOML file.

### Home Manager

When using the Home Manager module, `nite` reads the configuration from the Nix store. 

```nix
# flake.nix
inputs.nite.url = "github:thelevnet/nite";

# home.nix
{ inputs, pkgs, ... }: {
  imports = [ inputs.nite.homeManagerModules.nite ];

  modules.nite = {
    enable = true;

    instances = [
      {
        name    = "survival";
        version = "26.1.2";
        fabric  = true;
        
        mods          = [ "sodium" "iris" "lithium" ];
        resourcepacks = [ "fresh-food" ];
        shaderpacks   = [ "complementary-unbound" ];
        
        settings = {
          vsync          = false;
          guiScale       = 3;
          renderDistance = 16;
        };
        
        modsConfig = {
          iris = {
            shaders_enabled = true;
            shader_pack     = "ComplementaryUnbound_r5.7.1.zip";
          };
        };
        
        shaderSettings = {
          "ComplementaryUnbound_r5.7.1.zip" = {
            SHADOW_RESOLUTION = 2048;
            WATER_QUALITY = "HIGH";
          };
        };
      }
      {
        name     = "fabulously-optimized";
        version  = "26.1.2";
        fabric   = true;
        modpack  = "https://cdn.modrinth.com/data/1KVo5zza/versions/jmTldfTf/Fabulously.Optimized-v13.5.0.mrpack";
      }
    ];
  };
}
```

### Standalone Configuration

`nite` reads standalone configurations from `~/.config/nite/instances.toml`:

```toml
[[instance]]
name = "survival"
username = "steve"
version = "26.1.2"
fabric = true
mods = ["sodium", "iris", "lithium"]
resourcepacks = ["fresh-food"]
shaderpacks = ["complementary-unbound"]

[instance.settings]
vsync = false
fullscreen = true
guiScale = 4
renderDistance = 32

[instance.mods_config.iris]
shaders_enabled = true
shader_pack = "ComplementaryUnbound_r5.7.1.zip"

[instance.shader_settings."ComplementaryUnbound_r5.7.1.zip"]
SHADOW_RESOLUTION = 2048
WATER_QUALITY = "HIGH"
```

---

## Configuration Reference

### Supported Versions

`nite` uses recipes located in `recipes/<version>/mods/<mod>.toml` to map configuration keys to their specific files.

All Minecraft versions support downloading and running mods, resource packs, and shaders. However, modifying `settings` and `mods_config` requires a recipe for that specific version.

Versions with complete recipe support:
- `1.21.11`
- `1.21.4`
- `1.21.1`
- `1.21`
- `1.20.4`
- `1.20.1`
- `26.1.2`

---

## CLI Reference

### Core Commands

| Command | Description |
|---|---|
| `nite run <name>` | Launches the specified instance. |
| `nite new <name> [options]` | Creates a new instance in `instances.toml`. |
| `nite list` | Lists all configured instances. |
| `nite auth` | Authenticates with Microsoft. |

### Package Management

| Command | Description |
|---|---|
| `nite search <query>` | Searches Modrinth for packages. |
| `nite <instance> mods install <slug>` | Adds a mod to the instance and downloads it. |
| `nite <instance> mods remove <slug>` | Removes a mod from the instance. |
| `nite <instance> mods update <slug>` | Updates a specific mod. |
| `nite <instance> resourcepacks install <slug>` | Installs a resource pack. |
| `nite <instance> shaders install <slug>` | Installs a shader pack. |
| `nite <instance> modpack <url>` | Installs an `.mrpack` modpack. |
| `nite <instance> update` | Checks for and applies updates to all packages. |

### State Management

| Command | Description |
|---|---|
| `nite backup create <instance>` | Archives worlds and screenshots into a tarball. |
| `nite backup restore <instance> <file>` | Restores worlds and screenshots from an archive. |
| `nite clean` | Removes unused version binaries and assets. |

---

## Building from Source

```bash
nix develop
cargo build --release
```

To build using Nix:
```bash
nix build
./result/bin/nite run <instance>
```

---

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) to add support for new versions or recipes.
