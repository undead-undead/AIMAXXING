# AIMAXXING Windows Build & Package Script
# Generates Lite (binary only) and Recommended (Tools + Bash) installers.

$VERSION = "0.3.0"
$BUILD_DIR = "target\release"
$BIN_DIR = "bin"
$DATA_DIR = "data"

Write-Host "--- 1. Building AIMAXXING Core & Gateway ---" -ForegroundColor Cyan
cargo build --release -p aimaxxing-gateway

if (-not (Test-Path $BIN_DIR)) { New-Item -ItemType Directory -Path $BIN_DIR }

Write-Host "--- 2. Collecting Standalone Binaries ---" -ForegroundColor Cyan
# Downloading standalone tools for bundling (if not already present locally)
if (-not (Test-Path "$BIN_DIR\uv.exe")) {
    Write-Host "Downloading uv.exe..."
    Invoke-WebRequest -Uri "https://github.com/astral-sh/uv/releases/latest/download/uv-x86_64-pc-windows-msvc.zip" -OutFile "$BIN_DIR\uv.zip"
    Expand-Archive -Path "$BIN_DIR\uv.zip" -DestinationPath "$BIN_DIR\uv_tmp" -Force
    Move-Item -Path "$BIN_DIR\uv_tmp\uv.exe" -Destination "$BIN_DIR\uv.exe" -Force
    Remove-Item -Path "$BIN_DIR\uv.zip", "$BIN_DIR\uv_tmp" -Recurse
}

if (-not (Test-Path "$BIN_DIR\pixi.exe")) {
    Write-Host "Downloading pixi.exe..."
    Invoke-WebRequest -Uri "https://github.com/prefix-dev/pixi/releases/latest/download/pixi-x86_64-pc-windows-msvc.exe" -OutFile "$BIN_DIR\pixi.exe"
}

Write-Host "--- 3. Preparing Pre-provisioned Bash Environment (Recommended Version) ---" -ForegroundColor Cyan
# In a real CI environment, we would run 'pixi install bash' here
# and copy the result to data/envs/bash for bundling.
if (-not (Test-Path "$DATA_DIR\envs\bash")) { New-Item -ItemType Directory -Path "$DATA_DIR\envs\bash" -Force }

Write-Host "--- 4. Generating Inno Setup Installer ---" -ForegroundColor Cyan
if (Get-Command "iscc" -ErrorAction SilentlyContinue) {
    iscc aimaxxing_setup.iss
} else {
    Write-Warning "Inno Setup Compiler (iscc.exe) not found in PATH. Skipping installer generation."
    Write-Host "You can still use the collected files in $BUILD_DIR and $BIN_DIR manually."
}

Write-Host "Done! Setup file generated as aimaxxing_setup.exe (if ISCC was available)." -ForegroundColor Green
