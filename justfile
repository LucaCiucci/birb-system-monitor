
# List all available targets
default:
    @just --list
    @exit 1

# Create all Linux packages
package-linux: cross-debs flatpak appimage

install-deb: (cross-deb "x86_64-unknown-linux-gnu")
    sudo dpkg -i target/debian/birb-monitor_*amd64.deb

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

ci: check check-all

check-all: check check-fmt check-clippy check-unused-deps

check:
    cargo check

check-fmt:
    cargo fmt -- --check

check-clippy:
    cargo clippy -- -D warnings
check-unused-deps:
    cargo +nightly udeps
    cargo machete

cross-debs: (cross-deb "x86_64-unknown-linux-gnu") (cross-deb "aarch64-unknown-linux-gnu")

cross-deb TARGET: (cross-build-for TARGET) install-cargo-deb
    cargo deb --no-build --target {{TARGET}}

# x86_64-unknown-linux-gnu
cross-build-for TARGET: install-cargo-cross
    cross build --release --target {{TARGET}}

install-dist-tools: install-cargo-deb install-cargo-cross

install-cargo-deb:
    cargo install cargo-deb --version 3.8.0 --locked

install-cargo-cross:
    cargo install cargo-cross --version 1.6.0 --locked

update-man:
    mkdir -p man
    cargo run -- doc man man/
