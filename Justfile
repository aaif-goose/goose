# Justfile

mod goose2 'ui/goose2'

# list all tasks
default:
  @just --list

# Run all style checks and formatting (precommit validation)
check-everything:
    @echo "🔧 RUNNING ALL STYLE CHECKS..."
    @echo "  → Formatting Rust code..."
    cargo fmt --all
    @echo "  → Running clippy linting..."
    cargo clippy --all-targets -- -D warnings
    @echo "  → Checking UI code formatting..."
    cd ui/desktop && pnpm run lint:check
    @echo "  → Validating OpenAPI schema..."
    ./scripts/check-openapi-schema.sh
    @echo ""
    @echo "✅ All style checks passed!"

# Default release command
release-binary:
    @echo "Building release version..."
    cargo build --release
    @just copy-binary
    @echo "Generating OpenAPI schema..."
    cargo run -p goose-server --bin generate_schema

# release-windows docker build command
win_docker_build_sh := '''rustup target add x86_64-pc-windows-gnu && \
	apt-get update && \
	apt-get install -y mingw-w64 protobuf-compiler cmake && \
	export CC_x86_64_pc_windows_gnu=x86_64-w64-mingw32-gcc && \
	export CXX_x86_64_pc_windows_gnu=x86_64-w64-mingw32-g++ && \
	export AR_x86_64_pc_windows_gnu=x86_64-w64-mingw32-ar && \
	export CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER=x86_64-w64-mingw32-gcc && \
	export PKG_CONFIG_ALLOW_CROSS=1 && \
	export PROTOC=/usr/bin/protoc && \
	export PATH=/usr/bin:\$PATH && \
	protoc --version && \
	cargo build --release --target x86_64-pc-windows-gnu && \
	GCC_DIR=\$(ls -d /usr/lib/gcc/x86_64-w64-mingw32/*/ | head -n 1) && \
	cp \$GCC_DIR/libstdc++-6.dll /usr/src/myapp/target/x86_64-pc-windows-gnu/release/ && \
	cp \$GCC_DIR/libgcc_s_seh-1.dll /usr/src/myapp/target/x86_64-pc-windows-gnu/release/ && \
	cp /usr/x86_64-w64-mingw32/lib/libwinpthread-1.dll /usr/src/myapp/target/x86_64-pc-windows-gnu/release/
'''

# Build Windows executable
release-windows:
    #!/usr/bin/env sh
    if [ "$(uname)" = "Darwin" ] || [ "$(uname)" = "Linux" ]; then
        echo "Building Windows executable using Docker..."
        docker volume create goose-windows-cache || true
        docker run --rm \
            -v "$(pwd)":/usr/src/myapp \
            -v goose-windows-cache:/usr/local/cargo/registry \
            -w /usr/src/myapp \
            rust:latest \
            sh -c "{{win_docker_build_sh}}"
    else
        echo "Building Windows executable using Docker through PowerShell..."
        powershell.exe -Command "docker volume create goose-windows-cache; \`
            docker run --rm \`
                -v ${PWD}:/usr/src/myapp \`
                -v goose-windows-cache:/usr/local/cargo/registry \`
                -w /usr/src/myapp \`
                rust:latest \`
                sh -c '{{win_docker_build_sh}}'"
    fi
    echo "Windows executable and required DLLs created at ./target/x86_64-pc-windows-gnu/release/"

# Build for Intel Mac
release-intel:
    @echo "Building release version for Intel Mac..."
    cargo build --release --target x86_64-apple-darwin
    @just copy-binary-intel

copy-binary BUILD_MODE="release":
    @if [ -f ./target/{{BUILD_MODE}}/goosed ]; then \
        echo "Copying goosed binary from target/{{BUILD_MODE}}..."; \
        cp -p ./target/{{BUILD_MODE}}/goosed ./ui/desktop/src/bin/; \
    else \
        echo "Binary not found in target/{{BUILD_MODE}}"; \
        exit 1; \
    fi
    @if [ -f ./target/{{BUILD_MODE}}/goose ]; then \
        echo "Copying goose CLI binary from target/{{BUILD_MODE}}..."; \
        cp -p ./target/{{BUILD_MODE}}/goose ./ui/desktop/src/bin/; \
    else \
        echo "goose CLI binary not found in target/{{BUILD_MODE}}"; \
        exit 1; \
    fi

# Copy binary command for Intel build
copy-binary-intel:
    @if [ -f ./target/x86_64-apple-darwin/release/goosed ]; then \
        echo "Copying Intel goosed binary to ui/desktop/src/bin with permissions preserved..."; \
        cp -p ./target/x86_64-apple-darwin/release/goosed ./ui/desktop/src/bin/; \
    else \
        echo "Intel release binary not found."; \
        exit 1; \
    fi
    @if [ -f ./target/x86_64-apple-darwin/release/goose ]; then \
        echo "Copying Intel goose CLI binary to ui/desktop/src/bin..."; \
        cp -p ./target/x86_64-apple-darwin/release/goose ./ui/desktop/src/bin/; \
    else \
        echo "Intel goose CLI binary not found."; \
        exit 1; \
    fi

# Copy Windows binary command
copy-binary-windows:
    @powershell.exe -Command "if (Test-Path ./target/x86_64-pc-windows-gnu/release/goosed.exe) { \
        Write-Host 'Copying Windows binary and DLLs to ui/desktop/src/bin...'; \
        Copy-Item -Path './target/x86_64-pc-windows-gnu/release/goosed.exe' -Destination './ui/desktop/src/bin/' -Force; \
        Copy-Item -Path './target/x86_64-pc-windows-gnu/release/*.dll' -Destination './ui/desktop/src/bin/' -Force; \
    } else { \
        Write-Host 'Windows binary not found.' -ForegroundColor Red; \
        exit 1; \
    }"

# Run UI with latest
run-ui:
    @just release-binary
    @echo "Running UI..."
    cd ui/desktop && pnpm install && pnpm run start-gui

run-ui-playwright:
    #!/usr/bin/env sh
    just release-binary
    echo "Running UI with Playwright debugging..."
    RUN_DIR="$HOME/goose-runs/$(date +%Y%m%d-%H%M%S)"
    mkdir -p "$RUN_DIR"
    echo "Using isolated directory: $RUN_DIR"
    cd ui/desktop && ENABLE_PLAYWRIGHT=true GOOSE_PATH_ROOT="$RUN_DIR" pnpm run start-gui

run-ui-only:
    @echo "Running UI..."
    cd ui/desktop && pnpm install && pnpm run start-gui

debug-ui:
    @echo "🚀 Starting goose frontend in external backend mode"
    cd ui/desktop && \
    export GOOSE_EXTERNAL_BACKEND=true && \
    export GOOSE_SERVER__SECRET_KEY="${GOOSE_SERVER__SECRET_KEY:-test}" && \
    pnpm install && \
    pnpm run start-gui

# Run UI with main process debugging enabled
# To debug main process:
# 1. Run: just debug-ui-main-process
# 2. Open Chrome → chrome://inspect
# 3. Click "Open dedicated DevTools for Node"
# 4. If not auto-detected, click "Configure" and add: localhost:9229

debug-ui-main-process:
	@echo "🔍 Starting goose UI with main process debugging enabled"
	@just release-binary
	cd ui/desktop && \
	pnpm install && \
	pnpm run start-gui-debug

# Package the desktop app locally for testing (macOS)
# Applies ad-hoc code signing with entitlements (needed for mic access, etc.)
package-ui:
    @just release-binary
    @echo "Packaging desktop app..."
    cd ui/desktop && pnpm install && pnpm run package
    @echo "Signing with entitlements..."
    codesign --force --deep --sign - --entitlements ui/desktop/entitlements.plist ui/desktop/out/Goose-darwin-arm64/Goose.app
    @echo "Done! Launch with: open ui/desktop/out/Goose-darwin-arm64/Goose.app"

# Run UI with latest (Windows version)
run-ui-windows:
    @just release-windows
    @powershell.exe -Command "Write-Host 'Copying Windows binary...'"
    @just copy-binary-windows
    @powershell.exe -Command "Write-Host 'Running UI...'; Set-Location ui/desktop; pnpm install; pnpm run start-gui"

# Run Docusaurus server for documentation
run-docs:
    @echo "Running docs server..."
    cd documentation && yarn && yarn start

# Run server
run-server:
    @echo "Running server..."
    cargo run -p goose-server --bin goosed agent

# Check if OpenAPI schema is up-to-date
check-openapi-schema: generate-openapi
    ./scripts/check-openapi-schema.sh

# Generate OpenAPI specification without starting the UI
generate-openapi:
    @echo "Generating OpenAPI schema..."
    cargo run -p goose-server --bin generate_schema
    @echo "Generating frontend API..."
    cd ui/desktop && npx @hey-api/openapi-ts

# Check if generated ACP schema and TypeScript types are up-to-date
check-acp-schema: generate-acp-types
    #!/usr/bin/env bash
    set -e
    echo "🔍 Checking ACP schema and generated types are up-to-date..."
    if ! git diff --exit-code crates/goose/acp-schema.json crates/goose/acp-meta.json ui/sdk/src/generated/; then
      echo ""
      echo "❌ ACP generated files are out of date!"
      echo ""
      echo "Run 'just generate-acp-types' locally, then commit the changes."
      exit 1
    fi
    echo "✅ ACP schema and generated types are up-to-date"

# Generate ACP JSON schema from Rust types
generate-acp-schema:
    @echo "Generating ACP schema..."
    cd crates/goose && cargo run --bin generate-acp-schema
    @echo "ACP schema generated: crates/goose/acp-schema.json, crates/goose/acp-meta.json"

# Generate ACP TypeScript types from JSON schema (requires generate-acp-schema first)
generate-acp-types: generate-acp-schema
    @echo "Generating ACP TypeScript types..."
    cd ui/sdk && npx tsx generate-schema.ts
    @echo "ACP TypeScript types generated in ui/sdk/src/generated/"

# Build SDK TypeScript package (schema + types + compile)
build-sdk: generate-acp-types
    @echo "Compiling ACP TypeScript..."
    cd ui/sdk && pnpm run build:ts
    @echo "ACP package built."

# Generate manpages for the CLI
generate-manpages:
    @echo "Generating manpages..."
    cargo run -p goose-cli --bin generate_manpages
    @echo "Manpages generated at target/man/"

# make GUI with latest binary
lint-ui:
    cd ui/desktop && pnpm run lint:check

# make GUI with latest binary
make-ui:
    @just release-binary
    cd ui/desktop && pnpm run bundle:default

# make GUI with latest Windows binary
make-ui-windows:
    @just release-windows
    #!/usr/bin/env sh
    set -e
    if [ -f "./target/x86_64-pc-windows-gnu/release/goosed.exe" ]; then \
        echo "Cleaning destination directory..." && \
        rm -rf ./ui/desktop/src/bin && \
        mkdir -p ./ui/desktop/src/bin && \
        echo "Copying Windows binary and DLLs..." && \
        cp -f ./target/x86_64-pc-windows-gnu/release/goosed.exe ./ui/desktop/src/bin/ && \
        cp -f ./target/x86_64-pc-windows-gnu/release/*.dll ./ui/desktop/src/bin/ && \
        echo "Starting Windows package build..." && \
        (cd ui/desktop && pnpm run bundle:windows) && \
        echo "Windows package build complete!"; \
    else \
        echo "Windows binary not found."; \
        exit 1; \
    fi

# make GUI with latest binary
make-ui-intel:
    @just release-intel
    cd ui/desktop && pnpm run bundle:intel



# Run UI with debug build
run-dev:
    @echo "Building development version..."
    cargo build
    @just copy-binary debug
    @echo "Running UI..."
    cd ui/desktop && pnpm run start-gui

# Install all dependencies (run once after fresh clone)
install-deps:
    cd ui/desktop && pnpm install
    cd documentation && yarn

ensure-release-branch:
    #!/usr/bin/env bash
    branch=$(git rev-parse --abbrev-ref HEAD); \
    if [[ ! "$branch" == release/* ]]; then \
        echo "Error: You are not on a release branch (current: $branch)"; \
        exit 1; \
    fi

    # check that main is up to date with upstream main
    git fetch
    # @{u} refers to upstream branch of current branch
    if [ "$(git rev-parse HEAD)" != "$(git rev-parse @{u})" ]; then \
        echo "Error: Your branch is not up to date with the upstream branch"; \
        echo "  ensure your branch is up to date (git pull)"; \
        exit 1; \
    fi

# validate the version is semver, and not the current version
validate version:
    #!/usr/bin/env bash
    if [[ ! "{{ version }}" =~ ^[0-9]+\.[0-9]+\.[0-9]+(-.*)?$ ]]; then
      echo "[error]: invalid version '{{ version }}'."
      echo "  expected: semver format major.minor.patch or major.minor.patch-<suffix>"
      exit 1
    fi

    current_version=$(just get-tag-version)
    if [[ "{{ version }}" == "$current_version" ]]; then
      echo "[error]: current_version '$current_version' is the same as target version '{{ version }}'"
      echo "  expected: new version in semver format"
      exit 1
    fi

get-next-minor-version:
    @python -c "import sys; v=sys.argv[1].split('.'); print(f'{v[0]}.{int(v[1])+1}.0')" $(just get-tag-version)

get-next-patch-version:
    @python -c "import sys; v=sys.argv[1].split('.'); print(f'{v[0]}.{v[1]}.{int(v[2])+1}')" $(just get-tag-version)

# derive the prior release tag from a version
# patch bump (e.g. 1.25.1): prior is v1.25.0 (deterministic)
# minor bump (e.g. 1.26.0): prior is highest v1.25.* GitHub release
get-prior-version version:
    #!/usr/bin/env bash
    IFS='.' read -r major minor patch <<< "{{ version }}"
    if [[ "$patch" -gt 0 ]]; then
      echo "v${major}.${minor}.$((patch - 1))"
    elif [[ "$minor" -gt 0 ]]; then
      prev_minor=$((minor - 1))
      prefix="v${major}.${prev_minor}."
      best=$(gh release list --limit 100 --exclude-drafts --exclude-pre-releases \
        --json tagName --jq "[.[] | select(.tagName | startswith(\"${prefix}\"))][0].tagName")
      if [[ -n "$best" && "$best" != "null" ]]; then
        echo "$best"
      fi
    fi

# update version numbers in all manifests
bump-version version:
    @just validate {{ version }} || exit 1
    @uvx --from=toml-cli toml set --toml-path=Cargo.toml "workspace.package.version" {{ version }}
    @cd ui/desktop && npm pkg set "version={{ version }}"
    # update Cargo.lock after bumping versions in Cargo.toml
    @cargo update --workspace
    @just set-openapi-version {{ version }}

# rebuild canonical model registry and mapping report from models.dev
build-canonical-models:
    @cargo run --bin build_canonical_models

# bump version, rebuild canonical models, and commit
prepare-release version:
    @just bump-version {{ version }}
    @just build-canonical-models
    @git add \
        Cargo.toml \
        Cargo.lock \
        ui/desktop/package.json \
        ui/pnpm-lock.yaml \
        ui/desktop/openapi.json \
        crates/goose/src/providers/canonical/data/canonical_models.json \
        crates/goose/src/providers/canonical/data/provider_metadata.json
    @git commit --message "chore(release): release version {{ version }}"

set-openapi-version version:
    @jq '.info.version |= "{{ version }}"' ui/desktop/openapi.json > ui/desktop/openapi.json.tmp && mv ui/desktop/openapi.json.tmp ui/desktop/openapi.json

# extract version from Cargo.toml
get-tag-version:
    @uvx --from=toml-cli toml get --toml-path=Cargo.toml "workspace.package.version"

# create the git tag from Cargo.toml, checking we're on a release branch
tag: ensure-release-branch
    git tag v$(just get-tag-version)

# create tag and push to origin (use this when release branch is merged to main)
tag-push: tag
    # this will kick of ci for release
    git push origin tag v$(just get-tag-version)

# generate release notes from git commits
release-notes old:
    #!/usr/bin/env bash
    git log --pretty=format:"- %s" {{ old }}..v$(just get-tag-version)

### s = file separator based on OS
s := if os() == "windows" { "\\" } else { "/" }

### testing/debugging
os:
  echo "{{os()}}"
  echo "{{s}}"

# Make just work on Window
set windows-shell := ["powershell.exe", "-NoLogo", "-Command"]

### Build the core code
### profile = --release or "" for debug
### allparam = OR/AND/ANY/NONE --workspace --all-features --all-targets
win-bld profile allparam:
  cargo run {{profile}} -p goose-server --bin  generate_schema
  cargo build {{profile}} {{allparam}}

### Build just debug
win-bld-dbg:
  just win-bld " " " "

### Build debug and test, examples,...
win-bld-dbg-all:
  just win-bld " " "--workspace --all-targets --all-features"

### Build just release
win-bld-rls:
  just win-bld "--release" " "

### Build release and test, examples, ...
win-bld-rls-all:
  just win-bld "--release" "--workspace --all-targets --all-features"

### Install pnpm stuff
win-app-deps:
  cd ui{{s}}desktop ; pnpm install

### Windows copy {release|debug} files to ui\desktop\src\bin
### s = os dependent file separator
### profile = release or debug
win-copy-win profile:
  copy target{{s}}{{profile}}{{s}}*.exe ui{{s}}desktop{{s}}src{{s}}bin
  copy target{{s}}{{profile}}{{s}}*.dll ui{{s}}desktop{{s}}src{{s}}bin

### "Other" copy {release|debug} files to ui/desktop/src/bin
### s = os dependent file separator
### profile = release or debug
win-copy-oth profile:
  find target{{s}}{{profile}}{{s}} -maxdepth 1 -type f -executable -print -exec cp {} ui{{s}}desktop{{s}}src{{s}}bin \;

### copy files depending on OS
### profile = release or debug
win-app-copy profile="release":
  just win-copy-{{ if os() == "windows" { "win" } else { "oth" } }} {{profile}}

### Only copy binaries, pnpm install, start-gui
### profile = release or debug
### s = os dependent file separator
win-app-run profile:
  just win-app-copy {{profile}}
  just win-app-deps
  cd ui{{s}}desktop ; pnpm run start-gui

### Only run debug desktop, no build
win-run-dbg:
  just win-app-run "debug"

### Only run release desktop, nu build
win-run-rls:
  just win-app-run "release"

### Build and run debug desktop. tot = cli and desktop
### allparam = nothing or -all passed on command line
### -all = build with --workspace --all-targets --all-features
win-total-dbg *allparam:
  just win-bld-dbg{{allparam}}
  just win-run-dbg

### Build and run release desktop
### allparam = nothing or -all passed on command line
### -all = build with --workspace --all-targets --all-features
win-total-rls *allparam:
  just win-bld-rls{{allparam}}
  just win-run-rls

build-test-tools:
  cargo build -p goose-test

record-mcp-tests: build-test-tools
  GOOSE_RECORD_MCP=1 cargo test --package goose --test mcp_integration_test
  git add crates/goose/tests/mcp_replays/

bundle-goose2:
  cargo build --release --package goose-cli --bin goose
  cp target/release/goose target/release/goose-$(rustc --print host-tuple)
  @just goose2::bundle
