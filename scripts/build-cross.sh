#!/usr/bin/env bash
set -euo pipefail

project_dir="$(cd "$(dirname "$0")/.." && pwd)"
dist_dir="$project_dir/dist"
platform="${1:-all}"

build_windows() {
    command -v cargo-xwin >/dev/null 2>&1 || {
        echo "cargo-xwin is required: cargo install --locked cargo-xwin"
        exit 1
    }

    rustup target add x86_64-pc-windows-msvc
    cargo xwin build --release --target x86_64-pc-windows-msvc

    package_dir="$dist_dir/nrctool-windows-x86_64"
    mkdir -p "$package_dir"
    cp "$project_dir/target/x86_64-pc-windows-msvc/release/nrctool.exe" "$package_dir/NRCTool.exe"
    cp "$project_dir/README.md" "$package_dir/README.md"
    ditto -c -k --norsrc --keepParent "$package_dir" "$package_dir.zip" 2>/dev/null || \
        (cd "$dist_dir" && zip -qr "nrctool-windows-x86_64.zip" "nrctool-windows-x86_64")
    echo "Created $package_dir.zip"
}

build_linux() {
    command -v cargo-zigbuild >/dev/null 2>&1 || {
        echo "cargo-zigbuild is required: cargo install --locked cargo-zigbuild"
        exit 1
    }
    command -v zig >/dev/null 2>&1 || {
        echo "Zig is required. On macOS: brew install zig"
        exit 1
    }

    rustup target add x86_64-unknown-linux-gnu
    cargo zigbuild --release --target x86_64-unknown-linux-gnu.2.17

    package_dir="$dist_dir/nrctool-linux-x86_64"
    mkdir -p "$package_dir"
    cp "$project_dir/target/x86_64-unknown-linux-gnu/release/nrctool" "$package_dir/nrctool"
    cp "$project_dir/assets/app-icon.png" "$package_dir/nrctool.png"
    cp "$project_dir/packaging/linux/nrctool.desktop" "$package_dir/nrctool.desktop"
    cp "$project_dir/README.md" "$package_dir/README.md"
    tar -C "$dist_dir" -czf "$package_dir.tar.gz" "nrctool-linux-x86_64"
    echo "Created $package_dir.tar.gz"
}

mkdir -p "$dist_dir"
cd "$project_dir"

case "$platform" in
    windows) build_windows ;;
    linux) build_linux ;;
    all)
        build_windows
        build_linux
        ;;
    *)
        echo "Usage: $0 [windows|linux|all]"
        exit 1
        ;;
esac
