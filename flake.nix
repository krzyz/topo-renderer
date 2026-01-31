{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    rust-overlay.url = "github:oxalica/rust-overlay";
    crane.url = "github:ipetkov/crane";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs =
    {
      self,
      nixpkgs,
      rust-overlay,
      crane,
      flake-utils,
    }:
    flake-utils.lib.eachDefaultSystem (
      system:
      let
        pkgs = import nixpkgs {
          inherit system;
          overlays = [ rust-overlay.overlays.default ];
        };
        inherit (pkgs) lib;
        craneLib = crane.mkLib pkgs;
        topo-backend = craneLib.buildPackage ({
          pname = "topo-backend";
          cargoExtraArgs = "-p topo-backend";
          src = ./.;
        });
        toolchain = pkgs.rust-bin.fromRustupToolchainFile ./toolchain.toml;
      in
      {
        devShells.default = pkgs.mkShell {
          buildInputs = with pkgs; [
            openssl
            pkg-config
            dbus
            udev
            libxkbcommon
            vulkan-tools
            vulkan-headers
            vulkan-loader
            vulkan-validation-layers
            wayland
            #vscode-extensions.vadimcn.vscode-lldb
          ];

          packages = [
            toolchain
          ]
          ++ (with pkgs; [
            rust-analyzer-unwrapped
            cargo-edit
            wasm-pack
            trunk
            wgsl-analyzer
            just
            lldb
            superhtml
            vscode-langservers-extracted
            biome
          ]);

          RUST_SRC_PATH = "${toolchain}/lib/rustlib/src/rust/library";

          LD_LIBRARY_PATH = "$LD_LIBRARY_PATH:${
            with pkgs;
            lib.makeLibraryPath [
              openssl
              udev
              vulkan-loader
              libxkbcommon
              wayland
            ]
          }";

          AMD_VULKAN_ICD = "RADV";
          WGPU_BACKEND = "vulkan";

          shellHook = ''
            export PATH="$PATH:$HOME/.cargo/bin:$CODELLDB_PATH"
            exec nu
          '';
        };

        packages.default = topo-backend;

        apps = {
          topo-backend = flake-utils.lib.mkApp {
            drv = topo-backend;
          };
        };
      }
    );
}
