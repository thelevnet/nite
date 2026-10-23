{
  description = "nite - Pure declarative Minecraft instance launcher and sandbox for NixOS";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, flake-utils }:
    let
      overlay = final: prev: {
        nite = final.callPackage ./package.nix { };
      };
    in
    {
      overlays.default = overlay;

      # Home Manager Module for pure declarative instances
      homeManagerModules.nite = import ./nix/hm-module.nix;
      homeManagerModules.default = import ./nix/hm-module.nix;

      # NixOS Module to enable the Nix plugin for automatic nite.lock updates during nix flake update
      nixosModules.nite = { pkgs, ... }: {
        nix.settings.plugin-files = [ "${pkgs.nite}/lib/libnix_nite.so" ];
      };
      nixosModules.default = self.nixosModules.nite;
    }
    //
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs {
          inherit system;
          overlays = [ overlay ];
        };
      in
      {
        packages = {
          default = pkgs.nite;
          nite = pkgs.nite;
        };

        apps = {
          default = {
            type = "app";
            program = "${pkgs.nite}/bin/nite";
          };
          lock = {
            type = "app";
            program = "${pkgs.nite}/bin/nite-lock";
          };
        };

        devShells.default = pkgs.mkShell {
          strictDeps = true;
          nativeBuildInputs = [
            pkgs.cargo
            pkgs.rustc
            pkgs.clippy
            pkgs.rustfmt
            pkgs.pkg-config
            pkgs.unzip
            pkgs.bubblewrap
            pkgs.temurin-bin-25
          ];

          buildInputs = [
            pkgs.openal
            pkgs.libGL
            pkgs.vulkan-loader
            pkgs.stdenv.cc.cc.lib
            pkgs.udev
            pkgs.wayland
            pkgs.libxkbcommon
            pkgs.libX11
            pkgs.libXcursor
            pkgs.libXrandr
            pkgs.libXi
            pkgs.libXext
          ];

          shellHook = ''
            export LD_LIBRARY_PATH="${pkgs.lib.makeLibraryPath [
              pkgs.openal
              pkgs.libGL
              pkgs.vulkan-loader
              pkgs.stdenv.cc.cc.lib
              pkgs.udev
              pkgs.wayland
              pkgs.libxkbcommon
              pkgs.libX11
              pkgs.libXcursor
              pkgs.libXrandr
              pkgs.libXi
              pkgs.libXext
            ]}:/run/opengl-driver/lib:$LD_LIBRARY_PATH"
            export NIX_OPENAL_LIB="${pkgs.openal}/lib/libopenal.so"
          '';
        };
      }
    );
}
