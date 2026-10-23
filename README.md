# nite

A 100% pure declarative Minecraft launcher and sandbox for NixOS.

All instances, mods, resource packs, shader packs, game settings, and server definitions are declared purely in Nix and built directly into `/nix/store` derivations at system rebuild time.

Minecraft launches inside an isolated **Bubblewrap (`bwrap`)** sandbox with an **LD_PRELOAD write sink (`libnite_sandbox.so`)** that makes all instance files immutable Nix store symlinks while silently sinking writes outside mutable directories (`saves`, `screenshots`, `logs`, `xaero`) to `/dev/null` ("sure thing buddy").

---

## Features

- **Pure Nix derivations**: Zero runtime downloads in Rust. Mods, packs, libraries, and client JARs are fetched at rebuild time by Nix.
- **Flake-locked packages**: Dependencies are locked in `nite.lock` and globally updated via `nix flake update` using the bundled `libnix_nite.so` Nix plugin.
- **Bubblewrap + `/dev/null` sandbox**: Game settings, mod configs, and server definitions are immutable. Only `saves`, `screenshots`, `logs`, and `xaero` persist to `~/.local/share/nite/data/<instance>/`.
- **Zero imperative bloat**: No runtime `install`, `update`, `remove`, `auth`, or `clean` commands. The launcher only launches declared instances.
- **Declarative `servers.dat`**: Define multiplayer servers in Nix; `nite` formats them into an immutable binary NBT file.
- **Deterministic offline UUID**: Computed deterministically from player username.

---

## Usage (Home Manager)

Import `nite.homeManagerModules.nite` into your Home Manager configuration:

```nix
{ inputs, pkgs, ... }: {
  imports = [ inputs.nite.homeManagerModules.nite ];

  modules.nite = {
    enable = true;

    instances = [
      {
        name     = "default";
        version  = "1.21.1";
        username = "steve";
        fabric   = true;

        mods = [
          "sodium"
          "iris"
          "lithium"
          "modmenu"
          "appleskin"
        ];

        resourcepacks = [ "fresh-food" ];
        shaderpacks   = [ "complementary-unbound" ];

        servers = [
          {
            name = "Main Server";
            ip   = "100.127.101.226:25565";
          }
        ];

        settings = {
          guiScale       = 4;
          renderDistance = 32;
          vsync          = false;
        };

        modsConfig = {
          iris = {
            shaders_enabled = true;
            shader_pack     = "ComplementaryUnbound_r5.9.3.zip";
          };
        };
      }
    ];
  };
}
```

To automatically update `nite.lock` whenever you run `nix flake update`, add the NixOS module to your system configuration:

```nix
{ inputs, ... }: {
  imports = [ inputs.nite.nixosModules.nite ];
}
```

---

## CLI

The `nite` CLI is minimal and has no imperative package management commands:

| Command | Description |
|---|---|
| `nite run <instance>` (or `nite <instance>`) | Launches the instance inside the Bubblewrap sandbox. |
| `nite list` | Lists all configured instances. |
| `nite lock` | Refreshes and updates `nite.lock`. |

---

## Building from Source

```bash
nix build
./result/bin/nite list
```
