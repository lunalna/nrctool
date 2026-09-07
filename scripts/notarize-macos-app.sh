#!/bin/zsh
set -euo pipefail

project_dir="${0:A:h:h}"
app_dir="$project_dir/target/release/bundle/NRCTool.app"
archive="$project_dir/target/release/bundle/NRCTool-notarization.zip"
profile="${1:-nrctool-notary}"

if [[ ! -d "$app_dir" ]]; then
    echo "Build NRCTool.app first." >&2
    exit 1
fi

signature="$(codesign -dv --verbose=4 "$app_dir" 2>&1)"
if [[ "$signature" != *"Authority=Developer ID Application"* ]]; then
    echo "NRCTool.app is not signed with a Developer ID Application certificate." >&2
    exit 1
fi

rm -f "$archive"
ditto -c -k --keepParent "$app_dir" "$archive"
xcrun notarytool submit "$archive" --keychain-profile "$profile" --wait
xcrun stapler staple "$app_dir"
xcrun stapler validate "$app_dir"
spctl --assess --type execute --verbose=2 "$app_dir"
