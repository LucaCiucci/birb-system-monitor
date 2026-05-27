# Build all distribution packages
# ─────────────────────────────────────────────────

# Build a .deb package using cargo-deb
deb:
    cargo deb

# Build a Flatpak using flatpak-builder
flatpak:
    flatpak-builder --force-clean --install-deps-from=flathub build-dir flatpak/io.github.luciu.birb-monitor.yml

# Build an AppImage
appimage:
    ./build-appimage.sh

# Build all three distribution formats
all: deb flatpak appimage

# Just build the release binary (no packaging)
binary:
    cargo build --release

# Clean all build artifacts
clean:
    cargo clean
    rm -rf build-dir *.AppDir *.AppImage

# List all available targets
default:
    @just --list
