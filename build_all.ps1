Write-Host "=======================================" -ForegroundColor Cyan
Write-Host " Building Full AIMAXXING Suite (Windows)" -ForegroundColor Cyan
Write-Host "=======================================" -ForegroundColor Cyan

Write-Host "[INFO] Building Gateway Backend..." -ForegroundColor Blue
cargo build -p aimaxxing-gateway --release
if ($LASTEXITCODE -ne 0) {
    Write-Host "[ERROR] Gateway build failed." -ForegroundColor Red
    exit 1
}

Write-Host "[INFO] Building Frontend Panel..." -ForegroundColor Blue
cargo build -p aimaxxing-panel --release
if ($LASTEXITCODE -ne 0) {
    Write-Host "[ERROR] Panel build failed." -ForegroundColor Red
    exit 1
}

Write-Host "=======================================" -ForegroundColor Green
Write-Host " All components built successfully! 🚀 " -ForegroundColor Green
Write-Host "=======================================" -ForegroundColor Green
Write-Host ""
Write-Host "To start the Backend Gateway:"
Write-Host "  cargo run -p aimaxxing-gateway --release -- web"
Write-Host ""
Write-Host "To start the Frontend Panel:"
Write-Host "  cargo run -p aimaxxing-panel --release -- --url http://127.0.1:3000"
