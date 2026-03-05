#!/bin/bash
set -e

# AIMAXXING Full Build Script
echo "📦 Building AIMAXXING Panel (WASM)..."
cd panel
trunk build --release
cd ..

echo "🦀 Building AIMAXXING Gateway (Native)..."
cargo build -p aimaxxing-gateway --release

echo "✨ Build Complete!"
echo "You can now run the gateway, and it will serve the UI at http://localhost:3000"
echo "Command: ./target/release/aimaxxing-gateway web"
