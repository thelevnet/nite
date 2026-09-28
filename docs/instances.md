# `instances.toml` Schema

The `instances.toml` file is located at `~/.config/nite/instances.toml`.

## Top-Level Array: `[[instance]]`

Each instance is defined as a table within the `[[instance]]` array.

| Field | Type | Default | Description |
|---|---|---|---|
| `name` | String (Required) | None | The friendly identifier used in CLI commands (e.g., `nite run survival`). |
| `version` | String (Required) | None | The Minecraft version (e.g., `"26.1.2"` or `"1.21.1"`). |
| `username` | String (Required) | `"Player"` | The in-game player name. |
| `fabric` | Boolean | `false` | Whether to launch with Fabric Loader. |
| `memory` | String | `"4G"` | Memory allocation string passed to the JVM (e.g., `"8G"`, `"2048M"`). |
| `java` | String | None | Explicit path to a Java binary. If null, automatically resolves based on version. |
| `modpack` | String | None | A URL or local path to a `.mrpack` modpack. If set, it will be kept synced. |
| `mods` | Array of Strings / Objects | `[]` | List of Modrinth mod slugs (e.g., `"sodium"`) or objects with version pinning (e.g., `{ name = "lithium", version = "0.12.0" }`). |
| `resourcepacks` | Array of Strings / Objects | `[]` | List of Modrinth resource pack slugs or version objects. |
| `shaderpacks` | Array of Strings / Objects | `[]` | List of Modrinth shader pack slugs or version objects. |

## Sub-Tables

### `[instance.settings]`
Key-value overrides written to the instance's `options.txt` before launch. Keys must map to properties in a valid version recipe.

### `[instance.mods_config.<mod>]`
Key-value overrides for specific mods. `mod` must be a valid mod slug (e.g., `iris`). Valid keys are defined in the specific mod's recipe. Example: `[instance.mods_config.iris]`. Keybinds defined here route directly to `options.txt`.

### `[instance.shader_settings.<pack>]`
Key-value overrides for a specific shader pack. The pack string must exactly match the shader pack zip or text name (e.g., `"ComplementaryUnbound.zip"`). These are written to `shaderpacks/<pack>.zip.txt`.
