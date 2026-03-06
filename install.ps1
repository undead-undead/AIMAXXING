param(
    [switch]$Force
)

Write-Host "==========================================" -ForegroundColor Cyan
Write-Host " AIMAXXING Interactive Installer (Windows)" -ForegroundColor Cyan
Write-Host "==========================================" -ForegroundColor Cyan
Write-Host ""

function Prompt-Secret {
    param (
        [string]$Prompt,
        [string]$VarName
    )
    if ([Environment]::GetEnvironmentVariable($VarName)) {
        Write-Host "[INFO] Found $VarName in environment. Skipping prompt." -ForegroundColor Blue
        return
    }
    
    $secureString = Read-Host -Prompt "$Prompt (Leave blank if none) " -AsSecureString
    $ptr = [System.Runtime.InteropServices.Marshal]::SecureStringToBSTR($secureString)
    $inputStr = [System.Runtime.InteropServices.Marshal]::PtrToStringAuto($ptr)
    [System.Runtime.InteropServices.Marshal]::ZeroFreeBSTR($ptr)

    if (![string]::IsNullOrEmpty($inputStr)) {
        [Environment]::SetEnvironmentVariable($VarName, $inputStr, "Process")
    }
}

function Prompt-Value {
    param (
        [string]$Prompt,
        [string]$Default,
        [string]$VarName
    )
    if ([Environment]::GetEnvironmentVariable($VarName)) {
        return
    }

    $inputStr = Read-Host -Prompt "$Prompt [$Default]"
    if ([string]::IsNullOrEmpty($inputStr) -and ![string]::IsNullOrEmpty($Default)) {
        [Environment]::SetEnvironmentVariable($VarName, $Default, "Process")
    } else {
        [Environment]::SetEnvironmentVariable($VarName, $inputStr, "Process")
    }
}

Write-Host "[INFO] Checking system environment..." -ForegroundColor Blue

if (!(Get-Command "cargo" -ErrorAction SilentlyContinue)) {
    Write-Host "[ERROR] Rust/Cargo not found. Please install Rust via https://rustup.rs" -ForegroundColor Red
    exit 1
}

Write-Host "[OK] System environment looks good." -ForegroundColor Green

Write-Host ""
Write-Host "=== Configuration Wizard ===" -ForegroundColor Cyan
Write-Host "[INFO] Let's set up your environment variables." -ForegroundColor Blue

Prompt-Secret "OpenAI API Key" "OPENAI_API_KEY"
Prompt-Secret "Anthropic API Key" "ANTHROPIC_API_KEY"
Prompt-Secret "DeepSeek API Key" "DEEPSEEK_API_KEY"
Prompt-Secret "Gemini API Key" "GEMINI_API_KEY"
Prompt-Value "Ollama Base URL" "http://localhost:11434/v1" "OLLAMA_BASE_URL"
Prompt-Value "Server Port" "3000" "PORT"

Write-Host "[INFO] Generating .env file..." -ForegroundColor Blue

$envContent = @"
# AIMAXXING Config (Native Windows Mode)
PORT=$([Environment]::GetEnvironmentVariable("PORT"))
OLLAMA_BASE_URL=$([Environment]::GetEnvironmentVariable("OLLAMA_BASE_URL"))
OPENAI_API_KEY=$([Environment]::GetEnvironmentVariable("OPENAI_API_KEY"))
ANTHROPIC_API_KEY=$([Environment]::GetEnvironmentVariable("ANTHROPIC_API_KEY"))
DEEPSEEK_API_KEY=$([Environment]::GetEnvironmentVariable("DEEPSEEK_API_KEY"))
GEMINI_API_KEY=$([Environment]::GetEnvironmentVariable("GEMINI_API_KEY"))
RUST_LOG=info
"@

Set-Content -Path ".env" -Value $envContent
Write-Host "[OK] Configuration saved to .env" -ForegroundColor Green

Write-Host ""
Write-Host "=== Building AIMAXXING ===" -ForegroundColor Cyan
Write-Host "[INFO] Building gateway... (This may take a few minutes)" -ForegroundColor Blue

cargo build -p aimaxxing-gateway --release

if ($LASTEXITCODE -eq 0) {
    Write-Host ""
    Write-Host "========================================" -ForegroundColor Green
    Write-Host "    AIMAXXING Ready for Launch! 🚀      " -ForegroundColor Green
    Write-Host "========================================" -ForegroundColor Green
    Write-Host "Start Server:   cargo run -p aimaxxing-gateway --release -- web"
} else {
    Write-Host "[ERROR] Build failed. Please check the cargo output." -ForegroundColor Red
    exit 1
}
