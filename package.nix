{ lib
, rustPlatform
, fetchFromGitHub
, pkg-config
, makeWrapper
, unzip
, temurin-bin-25
, openal
, libGL
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
  version = "0.1.0";

  # When building from source (nix build / nix run in the repo), use the local tree.
  # External consumers (e.g. overlays) should override src with fetchFromGitHub:
  #
  #   src = fetchFromGitHub {
  #     owner = "thelevnet";
  #     repo  = "nite";
  #     rev   = "v${version}";
  #     hash  = "sha256-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=";
  #   };
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
    mkdir -p $out/share/nite
    cp -r recipes $out/share/nite/
  '';

  postFixup = ''
    wrapProgram $out/bin/nite \
      --prefix PATH : "${lib.makeBinPath [ temurin-bin-25 unzip ]}" \
      --prefix LD_LIBRARY_PATH : "${lib.makeLibraryPath runtimeLibs}" \
      --set-default NITE_RECIPES_PATH "$out/share/nite/recipes" \
      --set NIX_OPENAL_LIB "${openal}/lib/libopenal.so"
  '';

  meta = with lib; {
    description = "Declarative Minecraft instance launcher for NixOS";
    homepage = "https://github.com/thelevnet/nite";
    license = licenses.agpl3Plus;
    mainProgram = "nite";
    maintainers = [ ];
    platforms = platforms.linux;
  };
}
