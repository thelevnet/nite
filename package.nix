{ lib
, stdenv
, rustPlatform
, pkg-config
, makeWrapper
, unzip
, bubblewrap
, temurin-bin-25
, openal
, libGL
, vulkan-loader
, udev
, wayland
, libxkbcommon
, libX11
, libXcursor
, libXrandr
, libXi
, libXext
}:

let
  runtimeLibs = [
    libGL
    vulkan-loader
    stdenv.cc.cc.lib
    openal
    udev
    wayland
    libxkbcommon
    libX11
    libXcursor
    libXrandr
    libXi
    libXext
  ];
in
rustPlatform.buildRustPackage rec {
  pname = "nite";
  version = "0.2.0";

  src = lib.cleanSource ./.;

  cargoLock = {
    lockFile = ./Cargo.lock;
  };

  nativeBuildInputs = [
    pkg-config
    makeWrapper
  ];

  buildInputs = runtimeLibs;

  postInstall = ''
    mkdir -p $out/lib $out/share/nite
    cp -r recipes $out/share/nite/

    # Compile the LD_PRELOAD sandbox library that redirects immutable game writes to /dev/null
    $CC -shared -fPIC -O2 src/sandbox.c -ldl -o $out/lib/libnite_sandbox.so

    # Compile the Nix plugin for automatic nite.lock updates during nix flake update
    $CXX -shared -fPIC -O2 -std=c++20 src/nix_plugin.cc -ldl -o $out/lib/libnix_nite.so
  '';

  postFixup = ''
    wrapProgram $out/bin/nite \
      --prefix PATH : "${lib.makeBinPath [ temurin-bin-25 unzip bubblewrap ]}" \
      --prefix LD_LIBRARY_PATH : "${lib.makeLibraryPath runtimeLibs}:/run/opengl-driver/lib" \
      --set NITE_SANDBOX_LIB "$out/lib/libnite_sandbox.so" \
      --set NITE_BWRAP_BIN "${bubblewrap}/bin/bwrap" \
      --set-default NITE_RECIPES_PATH "$out/share/nite/recipes" \
      --set NIX_OPENAL_LIB "${openal}/lib/libopenal.so"

    ln -s $out/bin/nite $out/bin/nite-lock
  '';

  meta = with lib; {
    description = "Pure declarative Minecraft instance launcher and sandbox for NixOS";
    homepage = "https://github.com/thelevnet/nite";
    license = licenses.agpl3Plus;
    mainProgram = "nite";
    maintainers = [ ];
    platforms = platforms.linux;
  };
}
