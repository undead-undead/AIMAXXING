# Walkthrough: Local Media Integration

This update enables high-performance, local Speech-to-Text (STT) and Text-to-Speech (TTS) capabilities within AIMAXXING.

## New Features

### 1. Local Media Runtime (Gateway)
- **Whisper STT**: Integrated OpenAI Whisper (via `candle-transformers`) for local transcription.
- **Piper TTS**: Integrated Piper for fast, high-quality local voice synthesis.
- **Model Pooling**: Media models now participate in the global LRU `ModelPool`, respecting your RAM and VRAM budgets.

### 2. Panel UI Enhancements
- **Speech Tab**:
    - **Whisper Configuration**: Select between `ggml-tiny.en` and other models. Choose your transcription language (English, Chinese, or Auto-detect).
    - **Piper Configuration**: Select local voices like `en_US-lessac-medium` or `zh_CN-huayan-medium`.
    - **Model Management**: One-click **Download** and **Load** buttons directly in the UI. No more manual file moving.
    - **Status Indicators**: Real-time status for each media component (Loaded, Installed, Not Found).
- **System Tab**:
    - **Resource Budgets**: Added a new **System RAM Limit** slider (in addition to the existing VRAM slider) to control how much memory local models can consume.

## How to Enable

1. Go to the **Speech** tab in the AIMAXXING Panel.
2. Under **Local Whisper (STT)**, select your preferred model.
3. If the status is "Not Found", click **Download**. The Gateway will fetch the weights to your `data/models` directory.
4. Once "Installed", click **Load** to move it into VRAM/RAM. Status will change to "Loaded".
5. Toggle **Local Speech Enabled** for Piper if you want local text-to-speech.
6. Use the **🔄 Sync Global Voice to Gateway** button to make these settings the default for all your Agents.

## Developer Notes
- Media handlers are now part of the main Gateway server for improved stability.
- Backend routing: `/api/media/transcribe` and `/api/media/synthesize`.
- Model management: `/api/models/download` and `/api/models/load`.
