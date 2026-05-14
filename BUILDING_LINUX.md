# Building goose Desktop on Linux

This guide covers building the goose Desktop application from source on various Linux distributions.

## Prerequisites

### System Dependencies

**Debian/Ubuntu:**
```bash
sudo apt update
sudo apt install -y dpkg fakeroot build-essential clang libxcb1-dev libxcb-util-dev protobuf-compiler libvulkan-dev libvulkan1 glslc
```

**Arch/Manjaro:**
```bash
sudo pacman -S --needed dpkg fakeroot base-devel vulkan-headers vulkan-icd-loader shaderc
```

**Fedora/RHEL/CentOS:**
```bash
sudo dnf install dpkg-dev fakeroot gcc gcc-c++ make libxcb-devel vulkan-headers vulkan-loader glslc
```

**openSUSE:**
```bash
sudo zypper install dpkg fakeroot gcc gcc-c++ make vulkan-headers vulkan-loader glslc
```

**android / termux:**

goose is not officially support termux build yet, you need some minor patch to fix build issues.
We will publish goose (block-goose) into termux-packages. <!-- NOTE: package name kept for backwards compat -->
If you want to try there is a non-official build, https://github.com/shawn111/goose/releases/download/termux/goose-termux-aarch64.tar.bz2
For more details, see: https://github.com/aaif-goose/goose/pull/3890

```bash
pkg install rust
pkg install cmake protobuf clang build-essential
```

### Development Tools

- **Rust**: Install via [rustup](https://rustup.rs/)
- **Node.js**: Version 22.9.0 or later (use [nvm](https://github.com/nvm-sh/nvm) for version management)
- **pnpm**: Version 10 or later (managed via Hermit, or install globally)
- **just**: Install via `cargo install just` after Rust is installed. More [info](https://github.com/casey/just#packages)

## Build Process

### 1. Clone and Setup
```bash
git clone https://github.com/aaif-goose/goose.git
cd goose
```

### 2. Build

Build Goose CLI:

```bash
cargo build --release -p goose-cli
```

Build Goose Server:

```bash
cargo build --release -p goose-server
```

This command should give you a list of possible packages in the
workspace:

```bash
cargo test -p
```

### 3. Prepare the Desktop Application
```bash
cd ui/desktop
pnpm install

# Copy the server binary to the expected location
mkdir -p src/bin
cp ../../target/release/goosed src/bin/
```

### 4. Build the Application

#### Option A: ZIP Distribution (Recommended)
Works on all Linux distributions:
```bash
pnpm run make --targets=@electron-forge/maker-zip
```

Output: `out/make/zip/linux/x64/goose-linux-x64-{version}.zip`

#### Option B: DEB Package
For Debian/Ubuntu systems:
```bash
pnpm run make --targets=@electron-forge/maker-deb
```

Output: `out/make/deb/x64/goose_{version}_amd64.deb`

#### Option C: Both Formats
```bash
pnpm run make
```

### 5. Run the Application

#### From Build Directory
```bash
./out/goose-linux-x64/goose
```

#### Install DEB Package (if built)
```bash
sudo dpkg -i out/make/deb/x64/goose_*.deb
```

## Troubleshooting

### Common Issues

#### Missing System Dependencies
If you see errors about missing `dpkg`, `fakeroot`, Vulkan headers, or `glslc`:
```bash
# Install the missing packages for your distribution (see Prerequisites above)
```

#### GLib Warnings
You may see warnings like:
```
GLib-GObject: instance has no handler with id
```
These are harmless and don't affect functionality. To suppress them, create a launcher script:

```bash
#!/bin/bash
cd /path/to/goose/ui/desktop/out/goose-linux-x64
./goose 2>&1 | grep -v "GLib-GObject" | grep -v "browser_main_loop"
```

#### Server Binary Not Found
If you see "Could not find goosed binary", ensure you've:
1. Built the Rust backend: `cargo build --release -p goose-server`
2. Copied it to the right location: `cp ../../target/release/goosed src/bin/`
3. Rebuilt the application: `pnpm run make`

### Distribution-Specific Notes

#### Arch/Manjaro
- The RPM maker is disabled by default as it's not compatible with Arch-based systems
- Use the ZIP distribution method for maximum compatibility

#### Flatpak
Flatpak builds are supported via CI. To build locally:
```bash
# Install flatpak and flatpak-builder
sudo apt install flatpak flatpak-builder

# Add Flathub remote
flatpak remote-add --if-not-exists --user flathub https://dl.flathub.org/repo/flathub.flatpakrepo

# Build with Electron Forge
pnpm run make --targets=@electron-forge/maker-flatpak
```

Output: `out/make/flatpak/x86_64/*.flatpak`

#### Snap
Building as Snap packages is not currently supported but may be added in the future.

## Development Workflow

For active development:

1. **Backend changes**: Rebuild with `cargo build --release -p goose-server` and copy the binary
2. **Frontend changes**: Use `pnpm run start` for hot reload during development
3. **Full rebuild**: Run the complete build process above

## Creating System Integration

### Desktop Entry
Create `~/.local/share/applications/goose.desktop`:
```ini
[Desktop Entry]
Name=goose AI Agent
Comment=Local AI agent for development tasks
Exec=/path/to/goose/ui/desktop/out/goose-linux-x64/goose %U
Icon=/path/to/goose/ui/desktop/out/goose-linux-x64/resources/app.asar.unpacked/src/images/icon.png
Terminal=false
Type=Application
Categories=Development;Utility;
StartupNotify=true
MimeType=x-scheme-handler/goose
```

### System-wide Installation
To install system-wide:
```bash
sudo cp -r out/goose-linux-x64 /opt/goose
sudo ln -s /opt/goose/goose /usr/local/bin/goose-gui
```

## Contributing

When contributing changes that affect the Linux build process, please:

1. Test on multiple distributions if possible
2. Update this documentation
3. Update `ui/desktop/README.md` if needed
4. Consider CI/CD implications for automated builds
