{
  description = "nite - Declarative Minecraft instance launcher for NixOS";

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

      homeManagerModules.nite = import ./nix/hm-module.nix;
      # Convenience alias matching the home-manager convention
      homeManagerModules.default = import ./nix/hm-module.nix;
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

        apps.default = {
          type = "app";
          program = "${pkgs.nite}/bin/nite";
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
            pkgs.libx11
            pkgs.libxcursor
            pkgs.libxrandr
            pkgs.libxi
            pkgs.libxext
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
              pkgs.libx11
              pkgs.libxcursor
              pkgs.libxrandr
              pkgs.libxi
              pkgs.libxext
            ]}:/run/opengl-driver/lib:$LD_LIBRARY_PATH"
            export NIX_OPENAL_LIB="${pkgs.openal}/lib/libopenal.so"
          '';
        };
      }
    );
}
