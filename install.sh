#!/bin/bash
set -e

# ==========================================
# AIMAXXING Interactive Installer (v2.0 - Native)
# ==========================================

# Colors & Formatting
GREEN='\033[0;32m'
BLUE='\033[0;34m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
BOLD='\033[1m'
NC='\033[0m' # No Color

# Helper Functions
info()  { echo -e "${BLUE}[INFO]${NC}  $1"; }
ok()    { echo -e "${GREEN}[OK]${NC}    $1"; }
warn()  { echo -e "${YELLOW}[WARN]${NC}  $1"; }
err()   { echo -e "${RED}[ERROR]${NC} $1" >&2; }
die()   { err "$1"; exit 1; }

prompt_secret() {
    local prompt="$1"
    local var_name="$2"
    local input
    if [ -n "${!var_name}" ]; then
        info "Found $var_name in environment. Skipping prompt."
        return
    fi
    echo -ne "${BOLD}$prompt${NC}: "
    read -s input
    echo ""
    if [ -n "$input" ]; then
        export "$var_name"="$input"
    fi
}

prompt_value() {
    local prompt="$1"
    local default="$2"
    local var_name="$3"
    if [ -n "${!var_name}" ]; then
        return
    fi
    if [ -n "$default" ]; then
        echo -ne "${BOLD}$prompt${NC} [$default]: "
    else
        echo -ne "${BOLD}$prompt${NC}: "
    fi
    read input
    if [ -z "$input" ]; then
        export "$var_name"="$default"
    else
        export "$var_name"="$input"
    fi
}

# Banner
echo -e "${BLUE}
    ___    ___    ___   _______    ___       __ 
   /   |  /   |  /   | / ____/ /   /   |     / / 
  / /| | / /| | / /| |/ /   / /   / /| | /| / /  
 / ___ |/ ___ |/ ___ / /___/ /___/ ___ |/ |/ /   
/_/  |_/_/  |_/_/  |_\____/_____/_/  |_|  |__/    
                                                  
=== AIMAXXING Native Installer ===${NC}"

# 1. System Check
info "Checking system environment..."

if ! command -v cargo &> /dev/null; then
    die "Rust/Cargo not found. Please install Rust via https://rustup.rs"
fi

#[cfg(target_os = "linux")]
if [[ "$OSTYPE" == "linux-gnu"* ]]; then
    if ! command -v bwrap &> /dev/null; then
        warn "Bubblewrap (bwrap) not found. Required for sandbox isolation on Linux."
        read -p "Attempt to install bubblewrap via sudo apt? (y/N) " install_bwrap
        if [[ "$install_bwrap" =~ ^[Yy]$ ]]; then
            sudo apt update && sudo apt install -y bubblewrap
        fi
    fi
fi

ok "System environment looks good."

# 2. Configuration Wizard
echo -e "\n${BOLD}=== Configuration Wizard ===${NC}"
info "Let's set up your environment variables."

# LLM Keys
info "You need at least one LLM Provider API Key (or use Ollama)."
prompt_secret "OpenAI API Key" "OPENAI_API_KEY"
prompt_secret "Anthropic API Key" "ANTHROPIC_API_KEY"
prompt_secret "DeepSeek API Key" "DEEPSEEK_API_KEY"
prompt_secret "Gemini API Key" "GEMINI_API_KEY"
prompt_value "Ollama Base URL" "http://localhost:11434/v1" "OLLAMA_BASE_URL"

prompt_value "Server Port" "3000" "PORT"

# 3. Generate config
info "Generating .env file..."

cat > .env <<EOF
# AIMAXXING Config (Native Mode)
PORT=${PORT}
OLLAMA_BASE_URL=${OLLAMA_BASE_URL}
OPENAI_API_KEY=${OPENAI_API_KEY}
ANTHROPIC_API_KEY=${ANTHROPIC_API_KEY}
DEEPSEEK_API_KEY=${DEEPSEEK_API_KEY}
GEMINI_API_KEY=${GEMINI_API_KEY}
RUST_LOG=info
EOF

chmod 600 .env
ok "Configuration saved to .env"

# 4. Build
echo -e "\n${BOLD}=== Building AIMAXXING ===${NC}"
info "Building gateway... (This may take a few minutes)"

cargo build -p aimaxxing-gateway --release

if [ $? -eq 0 ]; then
    echo -e "\n${GREEN}========================================${NC}"
    echo -e "${GREEN}    AIMAXXING Ready for Launch! 🚀       ${NC}"
    echo -e "${GREEN}========================================${NC}"
    echo -e "Start Server:   ${BOLD}cargo run -p aimaxxing-gateway --release -- web${NC}"
    echo -e "Web Interface:  ${BOLD}http://localhost:${PORT}${NC}"
else
    err "Build failed. Check cargo output for errors."
    exit 1
fi
