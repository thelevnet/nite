# Home Manager Module Reference

When using `nite` via Home Manager, configure options under `modules.nite`.

## `modules.nite.enable`
- **Type:** `boolean`
- **Default:** `false`
- **Description:** Enables the `nite` declarative Minecraft launcher. Adds `nite` to your path and generates `instances.toml`.

## `modules.nite.package`
- **Type:** `package`
- **Default:** `inputs.nite.packages.${pkgs.system}.default`
- **Description:** The `nite` derivation to install.

## `modules.nite.instances`
- **Type:** `listOf submodule`
- **Default:** `[]`
- **Description:** List of instance configurations.

### Instance Submodule Options

| Field | Type | Default | Description |
|---|---|---|---|
| `name` | `string` | None (Required) | Instance identifier (used in `nite run <name>`). |
| `version` | `string` | None (Required) | Minecraft version (e.g., `"26.1.2"`, `"1.21.1"`). |
| `username` | `string` | `"Player"` | In-game player username. |
| `fabric` | `boolean` | `true` | Whether to install and launch with Fabric Loader. |
| `memory` | `nullOr string` | `null` (defaults to `"4G"`) | JVM memory allocation. |
| `java` | `nullOr string` | `null` | Explicit path to a Java binary. Auto-detected when null. |
| `modpack` | `nullOr string` | `null` | URL or local path to a `.mrpack` modpack file. |
| `mods` | `listOf (str / { name, version })` | `[]` | Modrinth mod slugs or versioned objects. |
| `resourcepacks` | `listOf (str / { name, version })` | `[]` | Modrinth resource pack slugs or versioned objects. |
| `shaderpacks` | `listOf (str / { name, version })` | `[]` | Modrinth shader pack slugs or versioned objects. |
| `settings` | `attrsOf (bool/int/float/str)` | `{}` | Vanilla `options.txt` overrides. Requires a version recipe. |
| `modsConfig` | `attrsOf (attrsOf (bool/int/float/str))` | `{}` | Per-mod config overrides. Keys are mod slugs, values are key-value maps. Requires recipes. |
| `shaderSettings` | `attrsOf (attrsOf (bool/int/float/str))` | `{}` | Per-shader pack settings overrides. Keys are shader pack zip names, values are key-value maps. |

---

## Example Usage

```nix
modules.nite = {
  enable = true;
  instances = [
    {
      name    = "default";
      version = "26.1.2";
      fabric  = true;
      mods    = [ "sodium" "iris" ];
      settings = {
        vsync          = false;
        renderDistance = 16;
      };
      modsConfig = {
        iris = { shaders_enabled = true; };
      };
      shaderSettings = {
        "ComplementaryUnbound.zip" = { SHADOW_RESOLUTION = 2048; };
      };
    }
  ];
};
```
