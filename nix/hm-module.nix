{ config, lib, pkgs, inputs, ... }:

let
  cfg = config.modules.nite;

  # Load nite.lock if available
  lockData =
    if cfg.lockFile != null && builtins.pathExists cfg.lockFile then
      builtins.fromJSON (builtins.readFile cfg.lockFile)
    else if builtins.pathExists ../nite.lock then
      builtins.fromJSON (builtins.readFile ../nite.lock)
    else
      { instances = {}; };

  renderOptionValue = v:
    if builtins.isBool v then (if v then "true" else "false")
    else if builtins.isFloat v then toString v
    else if builtins.isInt v then toString v
    else ''"${toString v}"'';

  # Build an immutable instance gameDir derivation in /nix/store
  mkInstanceGameDir = inst:
    let
      lockedInst = lockData.instances.${inst.name} or null;

      modDerivations =
        if lockedInst != null && (lockedInst.mods or []) != [] then
          map (m: pkgs.fetchurl {
            name = m.filename;
            url = m.url;
            sha512 = m.sha512;
          }) lockedInst.mods
        else [];

      resourcepackDerivations =
        if lockedInst != null && (lockedInst.resourcepacks or []) != [] then
          map (rp: pkgs.fetchurl {
            name = rp.filename;
            url = rp.url;
            sha512 = rp.sha512;
          }) lockedInst.resourcepacks
        else [];

      shaderpackDerivations =
        if lockedInst != null && (lockedInst.shaderpacks or []) != [] then
          map (sp: pkgs.fetchurl {
            name = sp.filename;
            url = sp.url;
            sha512 = sp.sha512;
          }) lockedInst.shaderpacks
        else [];

      optionsTxt = pkgs.writeText "options.txt" (
        lib.concatStringsSep "\n" (
          lib.mapAttrsToList (k: v: "${k}:${renderOptionValue v}") inst.settings
        ) + "\n"
      );

      serversJson = pkgs.writeText "servers.json" (builtins.toJSON (
        map (s: {
          name = s.name;
          ip = s.ip;
          accept_textures = s.acceptTextures;
        }) inst.servers
      ));

      serversDat =
        if inst.servers != [] then
          pkgs.runCommand "servers.dat" { nativeBuildInputs = [ cfg.package ]; } ''
            nite format-servers --input ${serversJson} --output $out
          ''
        else null;

    in pkgs.runCommand "nite-instance-${inst.name}" {} ''
      mkdir -p $out/mods $out/resourcepacks $out/shaderpacks $out/config

      # Symlink immutable mods
      ${lib.concatMapStringsSep "\n" (m: ''
        ln -s ${m} $out/mods/${m.name}
      '') modDerivations}

      # Symlink immutable resource packs
      ${lib.concatMapStringsSep "\n" (rp: ''
        ln -s ${rp} $out/resourcepacks/${rp.name}
      '') resourcepackDerivations}

      # Symlink immutable shader packs
      ${lib.concatMapStringsSep "\n" (sp: ''
        ln -s ${sp} $out/shaderpacks/${sp.name}
      '') shaderpackDerivations}

      # Options.txt
      ln -s ${optionsTxt} $out/options.txt

      # Servers.dat
      ${lib.optionalString (serversDat != null) ''
        ln -s ${serversDat} $out/servers.dat
      ''}

      # Mod configs
      ${lib.concatStringsSep "\n" (
        lib.mapAttrsToList (mod: kvs:
          let
            jsonContent = pkgs.writeText "${mod}.json" (builtins.toJSON kvs);
            propsContent = pkgs.writeText "${mod}.properties" (
              lib.concatStringsSep "\n" (
                lib.mapAttrsToList (k: v: "${k}=${renderOptionValue v}") kvs
              ) + "\n"
            );
          in
          if mod == "iris" then ''
            ln -s ${propsContent} $out/config/iris.properties
          ''
          else if mod == "continuity" then ''
            mkdir -p $out/config/continuity
            ln -s ${propsContent} $out/config/continuity/continuity.properties
          ''
          else ''
            ln -s ${jsonContent} $out/config/${mod}.json
          ''
        ) inst.modsConfig
      )}

      # Shader pack settings overrides
      ${lib.concatStringsSep "\n" (
        lib.mapAttrsToList (pack: kvs:
          let
            packTxt = pkgs.writeText "${pack}.txt" (
              lib.concatStringsSep "\n" (
                lib.mapAttrsToList (k: v: "${k}=${renderOptionValue v}") kvs
              ) + "\n"
            );
          in ''
            ln -s ${packTxt} $out/shaderpacks/${pack}.txt
          ''
        ) inst.shaderSettings
      )}
    '';

  # Generate manifest JSON read by nite binary
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
          map (l: pkgs.fetchurl {
            name = l.name;
            url = l.url;
          }) lockedInst.fabric_loader.libraries
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
        else "17";

      manifestData = {
        name = inst.name;
        version = inst.version;
        username = inst.username;
        java = if inst.java != null then inst.java else "${pkgs.temurin-bin-25}/bin/java";
        main_class = mainClass;
        classpath = classpath;
        game_dir = gameDir;
        assets_dir = "${cfg.package}/share/nite/assets";
        asset_index = assetIndexId;
        memory = if inst.memory != null then inst.memory else "4G";
        jvm_flags = [
          "-Djava.library.path=${lib.makeLibraryPath [ pkgs.libGL pkgs.vulkan-loader pkgs.openal pkgs.udev pkgs.wayland pkgs.libxkbcommon pkgs.libX11 ]}"
          "-Dorg.lwjgl.openal.libname=${pkgs.openal}/lib/libopenal.so"
        ];
        bwrap_bin = "${pkgs.bubblewrap}/bin/bwrap";
        sandbox_lib = "${cfg.package}/lib/libnite_sandbox.so";
      };
    in
      pkgs.writeText "nite-manifest-${inst.name}.json" (builtins.toJSON manifestData);

  instanceType = lib.types.submodule {
    options = {
      name = lib.mkOption {
        type = lib.types.str;
        description = "Instance identifier (e.g. `nite run <name>`).";
      };

      username = lib.mkOption {
        type = lib.types.str;
        default = "Player";
        description = "In-game player username (deterministic offline UUID).";
      };

      version = lib.mkOption {
        type = lib.types.str;
        description = "Minecraft version (e.g. \"1.21.1\", \"26.1.2\").";
      };

      fabric = lib.mkOption {
        type = lib.types.bool;
        default = true;
        description = "Whether to launch with Fabric Loader.";
      };

      mods = lib.mkOption {
        type = lib.types.listOf lib.types.str;
        default = [];
        description = "Modrinth mod slugs.";
      };

      resourcepacks = lib.mkOption {
        type = lib.types.listOf lib.types.str;
        default = [];
        description = "Modrinth resource pack slugs.";
      };

      shaderpacks = lib.mkOption {
        type = lib.types.listOf lib.types.str;
        default = [];
        description = "Modrinth shader pack slugs.";
      };

      servers = lib.mkOption {
        type = lib.types.listOf (lib.types.submodule {
          options = {
            name = lib.mkOption { type = lib.types.str; };
            ip = lib.mkOption { type = lib.types.str; };
            acceptTextures = lib.mkOption { type = lib.types.bool; default = true; };
          };
        });
        default = [];
        description = "Declarative multiplayer server entries (compiled to immutable servers.dat).";
      };

      settings = lib.mkOption {
        type = lib.types.attrsOf (lib.types.oneOf [ lib.types.bool lib.types.int lib.types.float lib.types.str ]);
        default = {};
        description = "Declarative options.txt settings.";
      };

      modsConfig = lib.mkOption {
        type = lib.types.attrsOf (lib.types.attrsOf (lib.types.oneOf [ lib.types.bool lib.types.int lib.types.float lib.types.str ]));
        default = {};
        description = "Declarative mod configs placed in config/.";
      };

      shaderSettings = lib.mkOption {
        type = lib.types.attrsOf (lib.types.attrsOf (lib.types.oneOf [ lib.types.bool lib.types.int lib.types.float lib.types.str ]));
        default = {};
        description = "Declarative shader pack settings placed in shaderpacks/<pack>.txt.";
      };

      memory = lib.mkOption {
        type = lib.types.nullOr lib.types.str;
        default = null;
        description = "JVM memory limit, e.g. \"4G\".";
      };

      java = lib.mkOption {
        type = lib.types.nullOr lib.types.str;
        default = null;
        description = "Path to Java binary.";
      };
    };
  };

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
      type = lib.types.listOf instanceType;
      default = [];
      description = "List of declared Minecraft instances.";
    };
  };

  config = lib.mkIf cfg.enable {
    home.packages = [ cfg.package ] ++ (
      map (inst: pkgs.writeShellScriptBin "nite-${inst.name}" ''
        exec ${cfg.package}/bin/nite run "${inst.name}" "$@"
      '') cfg.instances
    );

    xdg.dataFile = lib.listToAttrs (
      map (inst: {
        name = "nite/instances/${inst.name}.json";
        value = {
          source = mkInstanceManifest inst;
        };
      }) cfg.instances
    );
  };
}
