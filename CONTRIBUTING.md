# Contributing to nite

Thank you for contributing to `nite`! This guide explains how to add and maintain recipes for game settings and mods.

---

## Directory Layout

All recipes are organized under `recipes/<version>/`:

```
recipes/<version>/
├── options.toml       # Vanilla options.txt recipe mapping
└── mods/
    ├── <mod>.toml     # Mod configuration recipe
```

---

## 1. Game Options Recipes (`options.toml`)

Vanilla options are mapped via `recipes/<version>/options.toml`.

```toml
target_file = "options.txt"

[settings_map]
vsync = { target = "enableVsync", type = "bool" }
gui_scale = { target = "guiScale", type = "int" }
gamma = { target = "gamma", type = "float" }
```

Users can override these under `[instance.settings]` in `instances.toml`.

---

## 2. Mod Recipes (`recipes/<version>/mods/<mod>.toml`)

To add support for a mod in a specific Minecraft version:

1. **Recipe file**: `recipes/<version>/mods/<mod>.toml`
2. **Documentation**: `docs/recipes/<version>/mods/<mod>.md`

### Recipe Example

```toml
config_file = "config/example-mod.json" # or .properties / .toml

[settings_map]
fast_rendering = { target = "performance.fast_rendering", type = "bool" }
cache_size = { target = "cache_size", type = "int" }
theme = { target = "ui.theme", type = "string" }
```

Users configure these under `[instance.mods_config.<mod>]` in `instances.toml`.

#### Supported Formats:
- **`.properties`**: Key-value pairs (`key=value`)
- **`.json` / `.json5`**: Structured JSON objects with dot notation support (`quality.pixel_filtering_mode`)
- **`.toml`**: TOML format tables and values

---

## Development Workflow

1. Fork and clone the repository.
2. Enter the development environment:
   ```bash
   nix develop
   ```
3. Test your build:
   ```bash
   cargo build
   ```
4. Verify code formatting and lints:
   ```bash
   cargo clippy
   ```
5. Submit a Pull Request describing your changes.
