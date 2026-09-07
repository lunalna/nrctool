#!/bin/zsh
set -euo pipefail

project_dir="${0:A:h:h}"
icon_output="$(mktemp -d)"

trap 'rm -rf "$icon_output"' EXIT
cd "$project_dir"

/Applications/Xcode.app/Contents/Developer/usr/bin/actool \
    assets/Icon.icon \
    --compile "$icon_output" \
    --output-format human-readable-text \
    --notices \
    --warnings \
    --output-partial-info-plist "$icon_output/assetcatalog_generated_info.plist" \
    --app-icon Icon \
    --include-all-app-icons \
    --enable-on-demand-resources NO \
    --development-region en \
    --target-device mac \
    --minimum-deployment-target 26.0 \
    --platform macosx

mkdir -p macos/resources
cp "$icon_output/Assets.car" macos/resources/Assets.car
cp "$icon_output/Icon.icns" macos/resources/Icon.icns
