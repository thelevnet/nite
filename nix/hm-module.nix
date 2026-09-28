{ config, lib, pkgs, inputs, ... }:

let
  cfg = config.modules.nite;

  # Render a single mod/resourcepack/shaderpack entry to TOML inline value.
  # Accepts either a string slug or { name, version } attrset.
  renderPackageEntry = entry:
    if builtins.isString entry
      then ''"${entry}"''
    else ''{ name = "${entry.name}", version = "${entry.version}" }'';

  renderPackageList = entries:
    "[\n" + lib.concatMapStrings (e: "    ${renderPackageEntry e},\n") entries + "  ]";

  # Render a single [instance.settings] or [instance.mods_config.<mod>] value.
  renderTomlValue = v:
    if builtins.isBool v        then (if v then "true" else "false")
    else if builtins.isInt v    then toString v
    else if builtins.isFloat v  then toString v
    else ''"${v}"'';

  renderKeyValues = kvs:
    lib.concatStringsSep "\n" (
      lib.mapAttrsToList (k: v: "${k} = ${renderTomlValue v}") kvs
    );

  # Render one [[instance]] block.
  renderInstance = inst: ''
    [[instance]]
    name     = "${inst.name}"
    username = "${inst.username}"
    version  = "${inst.version}"
    fabric   = ${if inst.fabric then "true" else "false"}
    ${lib.optionalString (inst.memory != null) ''memory = "${inst.memory}"''}
    ${lib.optionalString (inst.java != null)   ''java   = "${inst.java}"''}
    ${lib.optionalString (inst.modpack != null) ''modpack = "${inst.modpack}"''}
    mods          = ${renderPackageList inst.mods}
    resourcepacks = ${renderPackageList inst.resourcepacks}
    shaderpacks   = ${renderPackageList inst.shaderpacks}
    ${lib.optionalString (inst.settings != {}) ''
    [instance.settings]
    ${renderKeyValues inst.settings}
    ''}
    ${lib.concatStringsSep "\n" (
      lib.mapAttrsToList (mod: kvs: ''
    [instance.mods_config.${mod}]
    ${renderKeyValues kvs}
      '') inst.modsConfig
    )}
    ${lib.concatStringsSep "\n" (
      lib.mapAttrsToList (pack: kvs: ''
    [instance.shader_settings."${pack}"]
    ${renderKeyValues kvs}
      '') inst.shaderSettings
    )}
  '';

  instancesToml = lib.concatMapStrings renderInstance cfg.instances;

  # Instance submodule type
  instanceType = lib.types.submodule {
    options = {
      name = lib.mkOption {
        type = lib.types.str;
        description = "Instance identifier (used in `nite run <name>`).";
      };

      username = lib.mkOption {
        type = lib.types.str;
        default = "Player";
        description = "In-game player username (used in offline mode).";
      };

      version = lib.mkOption {
        type = lib.types.str;
        description = "Minecraft version (e.g. \"26.1.2\", \"1.21.1\").";
      };

      fabric = lib.mkOption {
        type = lib.types.bool;
        default = true;
        description = "Whether to install and launch with Fabric Loader.";
      };

      mods = lib.mkOption {
        type = lib.types.listOf (lib.types.either lib.types.str (lib.types.submodule {
          options = {
            name    = lib.mkOption { type = lib.types.str; };
            version = lib.mkOption { type = lib.types.str; };
          };
        }));
        default = [];
        description = "Modrinth mod slugs or { name, version } entries.";
        example = [ "sodium" "iris" { name = "lithium"; version = "0.13.1"; } ];
      };

      resourcepacks = lib.mkOption {
        type = lib.types.listOf (lib.types.either lib.types.str (lib.types.submodule {
          options = {
            name    = lib.mkOption { type = lib.types.str; };
            version = lib.mkOption { type = lib.types.str; };
          };
        }));
        default = [];
        description = "Modrinth resource pack slugs or { name, version } entries.";
      };

      shaderpacks = lib.mkOption {
        type = lib.types.listOf (lib.types.either lib.types.str (lib.types.submodule {
          options = {
            name    = lib.mkOption { type = lib.types.str; };
            version = lib.mkOption { type = lib.types.str; };
          };
        }));
        default = [];
        description = "Modrinth shader pack slugs or { name, version } entries.";
      };

      settings = lib.mkOption {
        type = lib.types.attrsOf (lib.types.oneOf [ lib.types.bool lib.types.int lib.types.float lib.types.str ]);
        default = {};
        description = "Vanilla options.txt overrides (requires a recipe for the version).";
        example = {
          vsync        = false;
          fullscreen   = false;
          guiScale     = 3;
          renderDistance = 16;
        };
      };

      modsConfig = lib.mkOption {
        type = lib.types.attrsOf (lib.types.attrsOf (lib.types.oneOf [ lib.types.bool lib.types.int lib.types.float lib.types.str ]));
        default = {};
        description = "Per-mod config overrides. Keys are mod slugs, values are key-value maps.";
        example = {
          iris = {
            shaders_enabled = true;
            shader_pack     = "ComplementaryUnbound_r5.7.1.zip";
          };
        };
      };

      shaderSettings = lib.mkOption {
        type = lib.types.attrsOf (lib.types.attrsOf (lib.types.oneOf [ lib.types.bool lib.types.int lib.types.float lib.types.str ]));
        default = {};
        description = "Per-shader pack settings overrides. Keys are shader pack zip names, values are key-value maps.";
        example = {
          "ComplementaryUnbound_r5.7.1.zip" = {
            SHADOW_RESOLUTION = 2048;
            WATER_QUALITY = "HIGH";
          };
        };
      };

      memory = lib.mkOption {
        type = lib.types.nullOr lib.types.str;
        default = null;
        description = "JVM memory allocation, e.g. \"4G\" or \"2048M\". Defaults to 4G.";
      };

      java = lib.mkOption {
        type = lib.types.nullOr lib.types.str;
        default = null;
        description = "Explicit path to a Java binary. Auto-detected when null.";
      };

      modpack = lib.mkOption {
        type = lib.types.nullOr lib.types.str;
        default = null;
        description = ''
          URL or local path to a .mrpack modpack file. When set, nite installs
          the modpack on first run and reinstalls if the URL changes.
        '';
        example = "https://cdn.modrinth.com/data/1KVo5zza/versions/jmTldfTf/Fabulously.Optimized-v13.5.0.mrpack";
      };
    };
  };

in
{
  options.modules.nite = {
    enable = lib.mkEnableOption "nite declarative Minecraft launcher";

    package = lib.mkOption {
      type = lib.types.package;
      default = inputs.nite.packages.${pkgs.system}.default;
      defaultText = "inputs.nite.packages.\${pkgs.system}.default";
      description = "The nite package to use.";
    };

    instances = lib.mkOption {
      type = lib.types.listOf instanceType;
      default = [];
      description = "List of Minecraft instances to declare.";
      example = [
        {
          name    = "default";
          version = "26.1.2";
          fabric  = true;
          mods    = [ "sodium" "iris" "lithium" ];
          settings = {
            vsync          = false;
            renderDistance = 16;
          };
        }
      ];
    };
  };

  config = lib.mkIf cfg.enable {
    home.packages = [ cfg.package ];

    xdg.configFile."nite/instances.toml" = lib.mkIf (cfg.instances != []) {
      text = instancesToml;
    };
  };
}
