#!/bin/zsh
set -euo pipefail

project_dir="${0:A:h:h}"
app_dir="$project_dir/target/release/bundle/NRCTool.app"
contents_dir="$app_dir/Contents"
resources_dir="$contents_dir/Resources"
install_app=false
notarize_app=false

for argument in "$@"; do
    case "$argument" in
        --install) install_app=true ;;
        --notarize) notarize_app=true ;;
        -h|--help)
            echo "Usage: $0 [--install] [--notarize]"
            exit 0
            ;;
        *)
            echo "Unknown argument: $argument" >&2
            exit 2
            ;;
    esac
done

signing_identity="${CODESIGN_IDENTITY:-}"
if $notarize_app && [[ -z "$signing_identity" ]]; then
    developer_id_identities=("${(@f)$(security find-identity -v -p codesigning | sed -n 's/.*"\(Developer ID Application:[^"]*\)".*/\1/p')}")
    signing_identity="${developer_id_identities[1]:-}"

    if [[ -z "$signing_identity" ]]; then
        echo "--notarize requires a Developer ID Application certificate in your keychain." >&2
        echo "Install the certificate or set CODESIGN_IDENTITY explicitly." >&2
        exit 1
    fi
fi

cd "$project_dir"
cargo build --release

rm -rf "$app_dir"
mkdir -p "$contents_dir/MacOS" "$resources_dir"
cp target/release/nrctool "$contents_dir/MacOS/nrctool"
cp macos/Info.plist "$contents_dir/Info.plist"
cp macos/resources/Assets.car "$resources_dir/Assets.car"
cp macos/resources/Icon.icns "$resources_dir/Icon.icns"

if [[ -n "$signing_identity" ]]; then
    codesign --force --deep --options runtime --timestamp --sign "$signing_identity" "$app_dir"
else
    codesign --force --sign - "$app_dir"
fi

if $notarize_app; then
    "$project_dir/scripts/notarize-macos-app.sh"
fi

dist_dir="$project_dir/dist"
dmg_path="$dist_dir/NRCTool.dmg"
dmg_staging_dir="$(mktemp -d)"
trap 'rm -rf "$dmg_staging_dir"' EXIT

mkdir -p "$dist_dir"
ditto "$app_dir" "$dmg_staging_dir/NRCTool.app"
rm -f "$dmg_path"
hdiutil create -volname "NRCTool" -srcfolder "$dmg_staging_dir" -ov -format UDZO "$dmg_path"

if $install_app; then
    installed_app="/Applications/NRCTool.app"
    rm -rf "$installed_app"
    ditto "$app_dir" "$installed_app"
    echo "$installed_app"
fi

echo "$dmg_path"
