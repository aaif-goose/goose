{
  description = "goose - An AI agent CLI";

  inputs = {
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    nixpkgs.url = "nixpkgs/nixos-unstable";
  };

  outputs = { self, nixpkgs, flake-utils, rust-overlay }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        overlays = [ rust-overlay.overlays.default ];
        pkgs = import nixpkgs { inherit system overlays; };
        rust = pkgs.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml;
        
        # Read package metadata from Cargo.toml
        cargoToml = builtins.fromTOML (builtins.readFile ./crates/goose-cli/Cargo.toml);
        workspaceToml = builtins.fromTOML (builtins.readFile ./Cargo.toml);
        
        commonInputs = [
          rust
          pkgs.rust-analyzer
          pkgs.pkg-config
          pkgs.openssl
        ];
        
        darwinInputs = with pkgs; [
          libiconv
          apple-sdk
        ];
        
        buildInputs = commonInputs
          ++ pkgs.lib.optionals pkgs.stdenv.isDarwin darwinInputs;
      in
      {
        packages.default = pkgs.rustPlatform.buildRustPackage {
          pname = cargoToml.package.name;
          version = workspaceToml.workspace.package.version;
          src = self;

          cargoLock = {
            lockFile = ./Cargo.lock;
            outputHashes = {
              "cudaforge-0.1.6" = "sha256-w0e/mfx08BkphDEFEWxuyxyZu/gHiG0m6RHx+3BLzDY=";
            };
          };

          LIBCLANG_PATH = "${pkgs.libclang.lib}/lib";

          nativeBuildInputs = with pkgs; [
            pkg-config
            clang
            cmake
          ];

          buildInputs = with pkgs; [
            openssl
            cacert       # CA certificates for tests
            libxcb       # Required for xcap screenshot functionality
            dbus         # Required for system integration features
          ] ++ pkgs.lib.optionals pkgs.stdenv.isDarwin darwinInputs;

          # Build only the CLI package
          cargoBuildFlags = [ "--package" "goose-cli" ];
          
          # Enable tests with proper environment
          # Tests need writable HOME and XDG directories for config/cache access
          doCheck = true;
          checkPhase = ''
            export HOME=$(mktemp -d)
            export XDG_CONFIG_HOME=$HOME/.config
            export XDG_DATA_HOME=$HOME/.local/share
            export XDG_STATE_HOME=$HOME/.local/state
            export XDG_CACHE_HOME=$HOME/.cache
            mkdir -p $XDG_CONFIG_HOME $XDG_DATA_HOME $XDG_STATE_HOME $XDG_CACHE_HOME
            
            # Run tests for goose-cli package only
            cargo test --package goose-cli --release
          '';

          meta = with pkgs.lib; {
            description = workspaceToml.workspace.package.description;
            homepage = workspaceToml.workspace.package.repository;
            license = licenses.asl20;  # Maps from "Apache-2.0" in Cargo.toml
            mainProgram = "goose";
          };
        };

        devShells.default = pkgs.mkShell {
          packages = buildInputs ++ (with pkgs; [
            cargo-watch
            cargo-edit
            clippy
            gemini-cli # potentially useful during dev/testing
            go_1_25 # 'just' run-ui
            just # used in dev/test
            nodejs_24 # 'just' run-ui
            ripgrep
            rustfmt
            libxcb
            dbus
            yarn # 'just' install-deps
          ]);
          
          shellHook = ''
            echo "goose development environment"
            echo "Rust version: $(rustc --version)"
            echo ""
            echo "Commands:"
            echo "  nix build           - Build goose CLI"
            echo "  nix run             - Run goose CLI"
            echo "  cargo build -p goose-cli - Build with cargo"
            echo "  cargo run -p goose-cli   - Run with cargo"
          '';
        };
      }
    );
}
