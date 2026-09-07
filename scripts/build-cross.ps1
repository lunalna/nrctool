param(
    [ValidateSet("windows", "linux", "all")]
    [string]$Platform = "all"
)

$ErrorActionPreference = "Stop"
$ProjectDir = Split-Path -Parent $PSScriptRoot
$DistDir = Join-Path $ProjectDir "dist"

function Require-Command($Name, $InstallHint) {
    if (-not (Get-Command $Name -ErrorAction SilentlyContinue)) {
        throw "$Name is required. $InstallHint"
    }
}

function Build-Windows {
    Require-Command "cargo-xwin" "Run: cargo install --locked cargo-xwin"
    rustup target add x86_64-pc-windows-msvc
    cargo xwin build --release --target x86_64-pc-windows-msvc

    $PackageDir = Join-Path $DistDir "nrctool-windows-x86_64"
    New-Item -ItemType Directory -Force $PackageDir | Out-Null
    Copy-Item (Join-Path $ProjectDir "target/x86_64-pc-windows-msvc/release/nrctool.exe") (Join-Path $PackageDir "NRCTool.exe")
    Copy-Item (Join-Path $ProjectDir "README.md") $PackageDir
    Compress-Archive -Path $PackageDir -DestinationPath "$PackageDir.zip" -Force
    Write-Host "Created $PackageDir.zip"
}

function Build-Linux {
    Require-Command "cargo-zigbuild" "Run: cargo install --locked cargo-zigbuild"
    Require-Command "zig" "Install Zig from https://ziglang.org/download/"
    rustup target add x86_64-unknown-linux-gnu
    cargo zigbuild --release --target x86_64-unknown-linux-gnu.2.17

    $PackageDir = Join-Path $DistDir "nrctool-linux-x86_64"
    New-Item -ItemType Directory -Force $PackageDir | Out-Null
    Copy-Item (Join-Path $ProjectDir "target/x86_64-unknown-linux-gnu/release/nrctool") $PackageDir
    Copy-Item (Join-Path $ProjectDir "assets/app-icon.png") (Join-Path $PackageDir "nrctool.png")
    Copy-Item (Join-Path $ProjectDir "packaging/linux/nrctool.desktop") $PackageDir
    Copy-Item (Join-Path $ProjectDir "README.md") $PackageDir
    tar -C $DistDir -czf "$PackageDir.tar.gz" "nrctool-linux-x86_64"
    Write-Host "Created $PackageDir.tar.gz"
}

New-Item -ItemType Directory -Force $DistDir | Out-Null
Set-Location $ProjectDir

if ($Platform -eq "windows" -or $Platform -eq "all") { Build-Windows }
if ($Platform -eq "linux" -or $Platform -eq "all") { Build-Linux }
