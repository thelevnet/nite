{ config, lib, pkgs, inputs, ... }:

let
  cfg = config.modules.nite;

  jsonFormat = pkgs.formats.json {};
  tomlFormat = pkgs.formats.toml {};

  renderOptionValue = v:
    if builtins.isBool v then (if v then "true" else "false")
    else if builtins.isFloat v then toString v
    else if builtins.isInt v then toString v
    else ''"${toString v}"'';

  renderPropertyValue = v:
    if builtins.isBool v then (if v then "true" else "false")
    else if builtins.isList v then builtins.toJSON v
    else toString v;

  generateProperties = name: attrs:
    pkgs.writeText name (
      lib.concatStringsSep "\n" (
        lib.mapAttrsToList (k: v: "${k}=${renderPropertyValue v}") attrs
      ) + "\n"
    );

  generateOptionsTxt = name: attrs:
    pkgs.writeText name (
      lib.concatStringsSep "\n" (
        lib.mapAttrsToList (k: v: "${k}:${renderOptionValue v}") attrs
      ) + "\n"
    );

  # Uniform Arbitrary File Submodule
  fileSubmodule = { name, ... }: {
    options = {
      target = lib.mkOption {
        type = lib.types.str;
        default = name;
        description = "Path relative to instance game directory.";
      };
      source = lib.mkOption {
        type = lib.types.nullOr lib.types.path;
        default = null;
        description = "Path to source file.";
      };
      text = lib.mkOption {
        type = lib.types.nullOr lib.types.lines;
        default = null;
        description = "Raw text content.";
      };
      lines = lib.mkOption {
        type = lib.types.nullOr (lib.types.listOf lib.types.str);
        default = null;
        description = "List of text lines.";
      };
      json = lib.mkOption {
        type = lib.types.nullOr lib.types.attrs;
        default = null;
        description = "Attribute set formatted as JSON.";
      };
      toml = lib.mkOption {
        type = lib.types.nullOr lib.types.attrs;
        default = null;
        description = "Attribute set formatted as TOML.";
      };
      properties = lib.mkOption {
        type = lib.types.nullOr lib.types.attrs;
        default = null;
        description = "Attribute set formatted as Java properties / key-value.";
      };
    };
  };

  # Builds file submodule to a store derivation
  buildFileDerivation = f:
    let
      fileName = baseNameOf f.target;
    in
      if f.source != null then f.source
      else if f.text != null then pkgs.writeText fileName f.text
      else if f.lines != null then pkgs.writeText fileName (lib.concatStringsSep "\n" f.lines + "\n")
      else if f.json != null then jsonFormat.generate fileName f.json
      else if f.toml != null then tomlFormat.generate fileName f.toml
      else if f.properties != null then generateProperties fileName f.properties
      else throw "nite: file '${f.target}' has no content declared";

  # Mod Config Submodule
  modConfigSubmodule = { name, ... }: {
    options = {
      target = lib.mkOption {
        type = lib.types.nullOr lib.types.str;
        default = null;
        description = "Custom relative target path. If omitted, defaults to config/<name>.<format>.";
      };
      source = lib.mkOption { type = lib.types.nullOr lib.types.path; default = null; };
      text = lib.mkOption { type = lib.types.nullOr lib.types.lines; default = null; };
      lines = lib.mkOption { type = lib.types.nullOr (lib.types.listOf lib.types.str); default = null; };
      json = lib.mkOption { type = lib.types.nullOr lib.types.attrs; default = null; };
      toml = lib.mkOption { type = lib.types.nullOr lib.types.attrs; default = null; };
      properties = lib.mkOption {
        type = lib.types.nullOr (lib.types.either lib.types.attrs (lib.types.submodule {
          options = {
            target = lib.mkOption { type = lib.types.nullOr lib.types.str; default = null; };
            settings = lib.mkOption { type = lib.types.attrs; };
          };
        }));
        default = null;
      };
    };
  };

  # Server Submodule
  serverSubmodule = { name, ... }: {
    options = {
      name = lib.mkOption { type = lib.types.str; default = name; };
      displayName = lib.mkOption { type = lib.types.str; default = name; };
      ip = lib.mkOption { type = lib.types.nullOr lib.types.str; default = null; };
      address = lib.mkOption { type = lib.types.nullOr lib.types.str; default = null; };
      acceptTextures = lib.mkOption { type = lib.types.bool; default = true; };
    };
  };

  # Load nite.lock
  lockData =
    if cfg.lockFile != null && builtins.pathExists cfg.lockFile then
      builtins.fromJSON (builtins.readFile cfg.lockFile)
    else if builtins.pathExists ../nite.lock then
      builtins.fromJSON (builtins.readFile ../nite.lock)
    else
      { instances = {}; };

  resolveMod = instName: item:
    if lib.isDerivation item then { name = item.name; path = item; }
    else if builtins.isPath item then { name = baseNameOf item; path = item; }
    else if builtins.isString item then
      let
        entries = lockData.instances.${instName}.mods or [];
        match = lib.findFirst (e: e.name == item) null entries;
      in
        if match != null then {
          name = match.filename;
          path = pkgs.fetchurl {
            name = match.filename;
            url = match.url;
            sha512 = match.sha512;
          };
        }
        else throw "nite: mod '${item}' for instance '${instName}' not found in lockfile."
    else throw "nite: invalid mod definition";

  resolveResourcepack = instName: item:
    if lib.isDerivation item then { name = item.name; path = item; }
    else if builtins.isPath item then { name = baseNameOf item; path = item; }
    else if builtins.isString item then
      let
        entries = lockData.instances.${instName}.resourcepacks or [];
        match = lib.findFirst (e: e.name == item) null entries;
      in
        if match != null then {
          name = match.filename;
          path = pkgs.fetchurl {
            name = match.filename;
            url = match.url;
            sha512 = match.sha512;
          };
        }
        else throw "nite: resourcepack '${item}' for instance '${instName}' not found in lockfile."
    else throw "nite: invalid resourcepack definition";

  resolveShaderpack = instName: item:
    if lib.isDerivation item then { name = item.name; path = item; }
    else if builtins.isPath item then { name = baseNameOf item; path = item; }
    else if builtins.isString item then
      let
        entries = lockData.instances.${instName}.shaderpacks or [];
        match = lib.findFirst (e: e.name == item) null entries;
      in
        if match != null then {
          name = match.filename;
          path = pkgs.fetchurl {
            name = match.filename;
            url = match.url;
            sha512 = match.sha512;
          };
        }
        else throw "nite: shaderpack '${item}' for instance '${instName}' not found in lockfile."
    else throw "nite: invalid shaderpack definition";

  instanceSubmodule = { name, ... }: {
    options = {
      name = lib.mkOption {
        type = lib.types.str;
        default = name;
        description = "Instance identifier.";
      };

      version = lib.mkOption {
        type = lib.types.str;
        description = "Minecraft version (e.g. \"26.1.2\").";
      };

      fabric = lib.mkOption {
        type = lib.types.bool;
        default = true;
        description = "Enable Fabric Loader.";
      };

      username = lib.mkOption {
        type = lib.types.str;
        default = "Player";
        description = "In-game player username.";
      };

      meta = {
        displayName = lib.mkOption {
          type = lib.types.str;
          default = "Minecraft (${name})";
          description = "Human-readable label for desktop launchers and menus.";
        };
        description = lib.mkOption {
          type = lib.types.str;
          default = "Nite declarative Minecraft instance";
          description = "Comment / subtitle for app launcher entries.";
        };
        icon = lib.mkOption {
          type = lib.types.str;
          default = "minecraft";
          description = "Desktop icon name or absolute image path.";
        };
        categories = lib.mkOption {
          type = lib.types.listOf lib.types.str;
          default = [ "Game" ];
          description = "Desktop categories.";
        };
      };

      jvm = {
        package = lib.mkOption {
          type = lib.types.package;
          default = pkgs.temurin-bin-25;
          defaultText = "pkgs.temurin-bin-25";
          description = "JDK package to execute Minecraft with.";
        };
        minMemory = lib.mkOption {
          type = lib.types.str;
          default = "512M";
          description = "Initial JVM heap size (-Xms).";
        };
        maxMemory = lib.mkOption {
          type = lib.types.str;
          default = "4G";
          description = "Maximum JVM heap size (-Xmx).";
        };
        extraArgs = lib.mkOption {
          type = lib.types.listOf lib.types.str;
          default = [];
          description = "Additional JVM arguments.";
        };
      };

      # Backward-compatible shortcuts
      memory = lib.mkOption {
        type = lib.types.nullOr lib.types.str;
        default = null;
        description = "Legacy shortcut for jvm.maxMemory.";
      };

      java = lib.mkOption {
        type = lib.types.nullOr lib.types.str;
        default = null;
        description = "Legacy shortcut for path to java binary.";
      };

      mods = lib.mkOption {
        type = lib.types.listOf (lib.types.either lib.types.str lib.types.package);
        default = [];
        description = "List of mod slugs (from lockfile) or direct jar packages.";
      };

      resourcepacks = lib.mkOption {
        type = lib.types.listOf (lib.types.either lib.types.str lib.types.package);
        default = [];
        description = "List of resource pack slugs (from lockfile) or direct zip packages.";
      };

      shaderpacks = lib.mkOption {
        type = lib.types.listOf (lib.types.either lib.types.str lib.types.package);
        default = [];
        description = "List of shader pack slugs (from lockfile) or direct zip packages.";
      };

      # Uniform mod configurations
      # Accepts { json = ...; toml = ...; properties = ...; lines = ...; text = ...; source = ...; }
      # Automatically supports legacy format: { continuity = { connected_textures = true; }; }
      modsConfig = lib.mkOption {
        type = lib.types.attrsOf (lib.types.either modConfigSubmodule (lib.types.attrsOf (lib.types.oneOf [ lib.types.bool lib.types.int lib.types.float lib.types.str ])));
        default = {};
        description = "Declarative mod configurations placed in config/.";
      };

      # Backward-compatible shader settings
      shaderSettings = lib.mkOption {
        type = lib.types.attrsOf (lib.types.attrsOf (lib.types.oneOf [ lib.types.bool lib.types.int lib.types.float lib.types.str ]));
        default = {};
        description = "Declarative shader pack settings in shaderpacks/<pack>.txt.";
      };

      # Uniform Arbitrary Files
      files = lib.mkOption {
        type = lib.types.attrsOf (lib.types.submodule fileSubmodule);
        default = {};
        description = "Arbitrary declarative files placed in instance directory.";
      };

      # options.txt settings
      settings = lib.mkOption {
        type = lib.types.attrsOf (lib.types.oneOf [ lib.types.bool lib.types.int lib.types.float lib.types.str ]);
        default = {};
        description = "Declarative options.txt settings.";
      };

      # Multiplayer servers
      servers = lib.mkOption {
        type = lib.types.coercedTo
          (lib.types.listOf (lib.types.submodule serverSubmodule))
          (list: lib.listToAttrs (map (s: lib.nameValuePair s.name s) list))
          (lib.types.attrsOf (lib.types.submodule serverSubmodule));
        default = {};
        description = "Multiplayer servers list.";
      };
    };
  };

  compileInstanceFiles = inst:
    let
      # Auto-include mods declared in modsConfig
      allModKeys = lib.unique (
        (map (m: if builtins.isString m then m else m.name) inst.mods)
        ++ (lib.attrNames inst.modsConfig)
      );

      # Mods
      modEntries = map (m:
        let
          resolved = resolveMod inst.name m;
        in {
          target = "mods/${resolved.name}";
          source = resolved.path;
        }
      ) allModKeys;

      # Resource packs
      rpEntries = map (r:
        let
          resolved = resolveResourcepack inst.name r;
        in {
          target = "resourcepacks/${resolved.name}";
          source = resolved.path;
        }
      ) inst.resourcepacks;

      # Shader packs
      spEntries = map (s:
        let
          resolved = resolveShaderpack inst.name s;
        in {
          target = "shaderpacks/${resolved.name}";
          source = resolved.path;
        }
      ) inst.shaderpacks;

      # Mod configs
      modConfigEntries = lib.concatLists (lib.mapAttrsToList (modName: modVal:
        if modVal ? json || modVal ? toml || modVal ? properties || modVal ? text || modVal ? lines || modVal ? source then
          let
            propsSettings =
              if modVal.properties != null then
                if modVal.properties ? settings then modVal.properties.settings
                else modVal.properties
              else null;
            propsTarget =
              if modVal.properties ? target && modVal.properties.target != null then modVal.properties.target
              else if modVal.target != null then modVal.target
              else "config/${modName}.properties";
          in
            lib.optional (modVal.json != null) {
              target = if modVal.target != null then modVal.target else "config/${modName}.json";
              source = jsonFormat.generate "${modName}.json" modVal.json;
            } ++
            lib.optional (modVal.toml != null) {
              target = if modVal.target != null then modVal.target else "config/${modName}.toml";
              source = tomlFormat.generate "${modName}.toml" modVal.toml;
            } ++
            lib.optional (propsSettings != null) {
              target = propsTarget;
              source = generateProperties (baseNameOf propsTarget) propsSettings;
            } ++
            lib.optional (modVal.text != null) {
              target = if modVal.target != null then modVal.target else "config/${modName}.txt";
              source = pkgs.writeText "${modName}.txt" modVal.text;
            } ++
            lib.optional (modVal.lines != null) {
              target = if modVal.target != null then modVal.target else "config/${modName}.txt";
              source = pkgs.writeText "${modName}.txt" (lib.concatStringsSep "\n" modVal.lines + "\n");
            } ++
            lib.optional (modVal.source != null) {
              target = if modVal.target != null then modVal.target else "config/${modName}";
              source = modVal.source;
            }
        else
          # Legacy fallback: raw key-value attrset
          let
            jsonContent = jsonFormat.generate "${modName}.json" modVal;
            propsContent = generateProperties "${modName}.properties" modVal;
          in
            if modName == "iris" then [ { target = "config/iris.properties"; source = propsContent; } ]
            else if modName == "continuity" then [ { target = "config/continuity/continuity.properties"; source = propsContent; } ]
            else [ { target = "config/${modName}.json"; source = jsonContent; } ]
      ) inst.modsConfig);

      # Backward-compatible shader settings
      shaderSettingsEntries = lib.mapAttrsToList (pack: kvs: {
        target = "shaderpacks/${pack}.txt";
        source = generateOptionsTxt "${pack}.txt" kvs;
      }) inst.shaderSettings;

      # Arbitrary files
      explicitFiles = lib.mapAttrsToList (_: f: {
        target = f.target;
        source = buildFileDerivation f;
      }) inst.files;

      # options.txt
      optionsEntry = lib.optional (inst.settings != {}) {
        target = "options.txt";
        source = generateOptionsTxt "options.txt" inst.settings;
      };

      # servers.dat
      serversJson = pkgs.writeText "servers.json" (builtins.toJSON (
        lib.mapAttrsToList (_: s: {
          name = if s.displayName != null && s.displayName != "" then s.displayName else s.name;
          ip = if s.address != null then s.address else s.ip;
          accept_textures = s.acceptTextures;
        }) inst.servers
      ));

      serverEntry = lib.optional (inst.servers != {}) {
        target = "servers.dat";
        source = pkgs.runCommand "servers.dat" { nativeBuildInputs = [ cfg.package ]; } ''
          nite format-servers --input ${serversJson} --output $out
        '';
      };

    in modEntries ++ rpEntries ++ spEntries ++ modConfigEntries ++ shaderSettingsEntries ++ explicitFiles ++ optionsEntry ++ serverEntry;

  # Build the immutable instance directory derivation
  mkInstanceGameDir = inst:
    let
      files = compileInstanceFiles inst;
    in
      pkgs.runCommand "nite-instance-${inst.name}" {} ''
        mkdir -p $out/mods $out/resourcepacks $out/shaderpacks $out/config $out/saves $out/screenshots $out/logs $out/xaero $out/XaeroWorldMap $out/XaeroWaypoints $out/.fabric $out/CustomSkinLoader
        ${lib.concatMapStringsSep "\n" (f: ''
          mkdir -p "$(dirname "$out/${f.target}")"
          ln -s "${f.source}" "$out/${f.target}"
        '') files}
      '';

  # Generate instance manifest
  mkInstanceManifest = inst:
    let
      lockedInst = lockData.instances.${inst.name} or null;
      gameDir = mkInstanceGameDir inst;

      clientJar =
        if lockedInst != null && (lockedInst.mojang.client or null) != null then
          pkgs.fetchurl {
            name = lockedInst.mojang.client.name;
            url = lockedInst.mojang.client.url;
            sha1 = lockedInst.mojang.client.sha1;
          }
        else null;

      vanillaLibs =
        if lockedInst != null && (lockedInst.mojang.libraries or []) != [] then
          map (l: pkgs.fetchurl {
            name = l.name;
            url = l.url;
            sha1 = l.sha1;
          }) lockedInst.mojang.libraries
        else [];

      fabricLibs =
        if inst.fabric && lockedInst != null && (lockedInst.fabric_loader or null) != null then
          map (l: pkgs.fetchurl ({
            name = l.name;
            url = l.url;
          } // lib.optionalAttrs ((l.sha512 or null) != null) { sha512 = l.sha512; }
            // lib.optionalAttrs ((l.sha1 or null) != null && (l.sha512 or null) == null) { sha1 = l.sha1; }))
          lockedInst.fabric_loader.libraries
        else [];

      classpath = lib.filter (x: x != null) ([ clientJar ] ++ vanillaLibs ++ fabricLibs);

      mainClass =
        if inst.fabric then
          (if lockedInst != null && (lockedInst.fabric_loader or null) != null then
            lockedInst.fabric_loader.main_class
          else "net.fabricmc.loader.impl.launch.knot.KnotClient")
        else "net.minecraft.client.main.Main";

      assetIndexId =
        if lockedInst != null && (lockedInst.mojang.asset_index or null) != null then
          lockedInst.mojang.asset_index.id
        else "30";

      javaBin =
        if inst.java != null then inst.java
        else "${inst.jvm.package}/bin/java";

      memoryLimit =
        if inst.memory != null then inst.memory
        else inst.jvm.maxMemory;

      jvmFlags = [
        "-Djava.library.path=${lib.makeLibraryPath [ pkgs.libGL pkgs.vulkan-loader pkgs.openal pkgs.udev pkgs.wayland pkgs.libxkbcommon pkgs.libX11 pkgs.flite ]}"
        "-Dorg.lwjgl.openal.libname=${pkgs.openal}/lib/libopenal.so"
        "-Dglfw.platform=wayland"
        "-Xms${inst.jvm.minMemory}"
      ] ++ inst.jvm.extraArgs;

      manifestData = {
        name = inst.name;
        version = inst.version;
        username = inst.username;
        java = javaBin;
        main_class = mainClass;
        classpath = classpath;
        game_dir = gameDir;
        assets_dir = "${config.home.homeDirectory}/.local/share/nite/shared/assets";
        asset_index = assetIndexId;
        memory = memoryLimit;
        jvm_flags = jvmFlags;
        bwrap_bin = "${pkgs.bubblewrap}/bin/bwrap";
        sandbox_lib = "${cfg.package}/lib/libnite_sandbox.so";
      };
    in
      pkgs.writeText "nite-manifest-${inst.name}.json" (builtins.toJSON manifestData);

in
{
  options.modules.nite = {
    enable = lib.mkEnableOption "nite pure declarative Minecraft launcher";

    package = lib.mkOption {
      type = lib.types.package;
      default = inputs.nite.packages.${pkgs.system}.default;
      defaultText = "inputs.nite.packages.\${pkgs.system}.default";
      description = "The nite package to use.";
    };

    lockFile = lib.mkOption {
      type = lib.types.nullOr lib.types.path;
      default = null;
      description = "Path to nite.lock file.";
    };

    instances = lib.mkOption {
      type = lib.types.coercedTo
        (lib.types.listOf (lib.types.submodule instanceSubmodule))
        (list: lib.listToAttrs (map (i: lib.nameValuePair i.name i) list))
        (lib.types.attrsOf (lib.types.submodule instanceSubmodule));
      default = {};
      description = "Declarative Minecraft instances.";
    };
  };

  config = lib.mkIf cfg.enable {
    home.packages = [ cfg.package pkgs.libxkbcommon ]
      ++ (lib.mapAttrsToList (name: _:
        pkgs.writeShellScriptBin "nite-${name}" ''
          exec ${cfg.package}/bin/nite run "${name}" "$@"
        ''
      ) cfg.instances)
      ++ [
        (pkgs.writeShellScriptBin "nite-menu" ''
          set -euo pipefail
          INSTANCES="${lib.concatStringsSep "\\n" (lib.mapAttrsToList (n: i: "${n} - ${i.meta.displayName}") cfg.instances)}"
          if command -v fuzzel >/dev/null 2>&1; then
            CHOSEN=$(echo -e "$INSTANCES" | fuzzel --dmenu -p "Minecraft: " | awk '{print $1}')
          elif command -v rofi >/dev/null 2>&1; then
            CHOSEN=$(echo -e "$INSTANCES" | rofi -dmenu -p "Minecraft" | awk '{print $1}')
          elif command -v wofi >/dev/null 2>&1; then
            CHOSEN=$(echo -e "$INSTANCES" | wofi --dmenu -p "Minecraft: " | awk '{print $1}')
          else
            echo "=== Minecraft Instances ==="
            echo -e "$INSTANCES"
            read -rp "Select instance: " CHOSEN
          fi
          if [ -n "''${CHOSEN:-}" ]; then
            exec ${cfg.package}/bin/nite run "$CHOSEN"
          fi
        '')
      ];

    xdg.desktopEntries = lib.mapAttrs' (name: inst:
      lib.nameValuePair "nite-${name}" {
        name = inst.meta.displayName;
        comment = inst.meta.description;
        icon = inst.meta.icon;
        categories = inst.meta.categories;
        exec = "${cfg.package}/bin/nite run ${name}";
        terminal = false;
        actions = {
          "screenshots" = {
            name = "Open Screenshots";
            exec = "xdg-open ${config.home.homeDirectory}/.local/share/nite/instances/${name}/screenshots";
          };
          "saves" = {
            name = "Open Saves";
            exec = "xdg-open ${config.home.homeDirectory}/.local/share/nite/instances/${name}/saves";
          };
        };
      }
    ) cfg.instances;

    xdg.dataFile = lib.mapAttrs' (name: inst:
      lib.nameValuePair "nite/instances/${name}.json" {
        source = mkInstanceManifest inst;
      }
    ) cfg.instances // (
      lib.listToAttrs (
        lib.filter (x: x.value != null) (
          lib.mapAttrsToList (name: inst:
            let
              lockedInst = lockData.instances.${name} or null;
              assetIdx =
                if lockedInst != null && (lockedInst.mojang.asset_index or null) != null then
                  pkgs.fetchurl {
                    name = "${lockedInst.mojang.asset_index.id}.json";
                    url = lockedInst.mojang.asset_index.url;
                    sha1 = lockedInst.mojang.asset_index.sha1;
                  }
                else null;
              assetIdxId =
                if lockedInst != null && (lockedInst.mojang.asset_index or null) != null then
                  lockedInst.mojang.asset_index.id
                else "30";
            in {
              name = "nite/shared/assets/indexes/${assetIdxId}.json";
              value = if assetIdx != null then { source = assetIdx; } else null;
            }
          ) cfg.instances
        )
      )
    );
  };
}
