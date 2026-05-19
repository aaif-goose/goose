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
              "opentelemetry-0.31.0" = "sha256-3uayFFBnM9LHnWpDdbKv+qjMvaqowrODAcZuK20/cdA=";
            };
          };
          
          LIBCLANG_PATH = "${pkgs.libclang.lib}/lib";

          # Pre-fetch rusty_v8 binary to avoid network access during build
          # Map Nix system to rusty_v8 target triple
          RUSTY_V8_ARCHIVE = let
            cargoLock = builtins.fromTOML (builtins.readFile ./Cargo.lock);
            rustyV8Version = (builtins.head (builtins.filter (p: p.name == "v8") cargoLock.package)).version;
            rustyV8Target = {
              "x86_64-linux" = "x86_64-unknown-linux-gnu";
              "aarch64-linux" = "aarch64-unknown-linux-gnu";
              "x86_64-darwin" = "x86_64-apple-darwin";
              "aarch64-darwin" = "aarch64-apple-darwin";
            }.${system} or (throw "Unsupported system: ${system}");
            rustyV8Sha256 = {
              "x86_64-linux" = "sha256-chV1PAx40UH3Ute5k3lLrgfhih39Rm3KqE+mTna6ysE=";
              "aarch64-linux" = "sha256-4IivYskhUSsMLZY97+g23UtUYh4p5jk7CzhMbMyqXyY=";
              "x86_64-darwin" = "sha256-1jUuC+z7saQfPYILNyRJanD4+zOOhXU2ac/LFoytwho=";
              "aarch64-darwin" = "sha256-yHa1eydVCrfYGgrZANbzgmmf25p7ui1VMas2A7BhG6k=";
            }.${system};
          in pkgs.fetchurl {
            url = "https://github.com/denoland/rusty_v8/releases/download/v${rustyV8Version}/librusty_v8_release_${rustyV8Target}.a.gz";
            sha256 = rustyV8Sha256;
          };

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
