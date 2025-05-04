{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    rust-overlay.url = "github:oxalica/rust-overlay";
  };

  outputs =
    {
      self,
      nixpkgs,
      rust-overlay,
    }:
    let
      system = "x86_64-linux";
      pkgs = import nixpkgs {
        inherit system;
        overlays = [ rust-overlay.overlays.default ];
      };
      toolchain = pkgs.rust-bin.fromRustupToolchainFile ./toolchain.toml;
    in
    {
      devShells.${system}.default = pkgs.mkShell {
        buildInputs =
          [
            (pkgs.python3.withPackages (
              python-pkgs: with python-pkgs; [
                jupyter
                ipython
                polars
                numpy
                matplotlib
              ]
            ))
          ]
          ++ (with pkgs; [
            openssl
            pkg-config
            gdal
            dbus
            udev
            libxkbcommon
            vulkan-tools
            vulkan-headers
            vulkan-loader
            vulkan-validation-layers
            wayland
          ]);

        packages =
          [ toolchain ]
          ++ (with pkgs; [
            evcxr
            rust-analyzer-unwrapped
            wasm-pack
            trunk
          ]);

        RUST_SRC_PATH = "${toolchain}/lib/rustlib/src/rust/library";

        LD_LIBRARY_PATH = "$LD_LIBRARY_PATH:${
          with pkgs;
          lib.makeLibraryPath [
            openssl
            gdal
            udev
            vulkan-loader
            libxkbcommon
            wayland
          ]
        }";

        AMD_VULKAN_ICD = "RADV";

        shellHook = ''
          export PATH="$PATH:$HOME/.cargo/bin"
          exec nu
        '';
      };
    };
}
