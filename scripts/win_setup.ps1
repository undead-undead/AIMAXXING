# AIMAXXING Windows Setup & Build Script
# This script prepares the environment and builds the MSI installer.

Write-Host "--- AIMAXXING // Windows Kernel Provisioning ---" -ForegroundColor Blue

# 1. Check for Rust
if (!(Get-Command cargo -ErrorAction SilentlyContinue)) {
    Write-Host "[!] Rust not found. Installing rustup..." -ForegroundColor Yellow
    Invoke-WebRequest -Uri "https://static.rust-lang.org/rustup/dist/x86_64-pc-windows-msvc/rustup-init.exe" -OutFile "rustup-init.exe"
    Start-Process -FilePath "rustup-init.exe" -ArgumentList "-y" -Wait
    Remove-Item "rustup-init.exe"
    $env:Path += ";$env:USERPROFILE\.cargo\bin"
}

# 2. Check for Dioxus CLI
if (!(Get-Command dx -ErrorAction SilentlyContinue)) {
    Write-Host "[+] Installing Dioxus CLI..." -ForegroundColor Cyan
    cargo install dioxus-cli
}

# 3. Check for WiX Toolset (Required for MSI bundling)
if (!(Get-Command candle -ErrorAction SilentlyContinue)) {
    Write-Host "[!] WiX Toolset not found. MSI bundling might fail." -ForegroundColor Yellow
    Write-Host "[*] Please install WiX Toolset v3 from: https://wixtoolset.org/releases/" -ForegroundColor Gray
}

# 4. Build the Project
Write-Host "[+] Compiling AIMAXXING // Mission Control..." -ForegroundColor Green
cd aimaxxing-ui
dx bundle --platform desktop --release

Write-Host "`n--- BUILD COMPLETE ---" -ForegroundColor Green
Write-Host "Installer Location: aimaxxing-ui/target/dx/AIMAXXING/release/windows/bundle/msi/" -ForegroundColor DarkCyan
Write-Host "You can now distribute the .msi file to your users for a one-click installation experience."
