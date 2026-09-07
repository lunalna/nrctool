#!/bin/zsh
set -euo pipefail

project_dir="${0:A:h:h}"
app_dir="$project_dir/target/release/bundle/NRCTool.app"
contents_dir="$app_dir/Contents"
resources_dir="$contents_dir/Resources"
install_app=false

case "${1:-}" in
    "") ;;
    --install) install_app=true ;;
    -h|--help)
        echo "Usage: $0 [--install]"
        exit 0
        ;;
    *)
        echo "Unknown argument: $1" >&2
        exit 2
        ;;
esac

cd "$project_dir"
cargo build --release

rm -rf "$app_dir"
mkdir -p "$contents_dir/MacOS" "$resources_dir"
cp target/release/nrctool "$contents_dir/MacOS/nrctool"
cp macos/Info.plist "$contents_dir/Info.plist"
cp macos/resources/Assets.car "$resources_dir/Assets.car"
cp macos/resources/Icon.icns "$resources_dir/Icon.icns"

if [[ -n "${CODESIGN_IDENTITY:-}" ]]; then
    codesign --force --deep --options runtime --timestamp --sign "$CODESIGN_IDENTITY" "$app_dir"
else
    codesign --force --sign - "$app_dir"
fi

if $install_app; then
    installed_app="/Applications/NRCTool.app"
    rm -rf "$installed_app"
    ditto "$app_dir" "$installed_app"
    echo "$installed_app"
    exit 0
fi

echo "$app_dir"
