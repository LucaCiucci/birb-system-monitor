
# List all available targets
default:
    @just --list
    @exit 1

# Create all Linux packages
package-linux: package-deb flatpak appimage

# Build a .deb package using cargo-deb
package-deb:
    cargo deb
    mkdir -p dist
    cp target/debian/birb-monitor_*.deb dist/

install-deb: package-deb
    sudo dpkg -i dist/birb-monitor_*.deb

uninstall-deb:
    sudo dpkg -r birb-monitor

# Build a Flatpak using flatpak-builder
flatpak:
    flatpak-builder --force-clean --install-deps-from=flathub build-dir flatpak/io.github.luciu.birb-monitor.yml

# Build an AppImage
appimage:
    ./build-appimage.sh

# Just build the release binary (no packaging)
binary:
    cargo build --release

# Clean all build artifacts
clean:
    cargo clean
    rm -rf build-dir *.AppDir *.AppImage

ci: check-unused-deps

check-unused-deps:
    cargo +nightly udeps
    cargo machete