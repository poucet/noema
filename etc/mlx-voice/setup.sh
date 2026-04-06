#!/bin/bash
# Setup and run MLX-based voice server (TTS + STT) on Apple Silicon.
#
# Models:
#   TTS: mlx-community/Voxtral-4B-TTS-2603-mlx-4bit (~2.5 GB)
#   STT: mlx-community/Voxtral-Mini-4B-Realtime-2602-4bit (~3.1 GB)
#
# Endpoints (OpenAI-compatible):
#   POST /v1/audio/speech          — TTS
#   POST /v1/audio/transcriptions  — STT
#
# Usage:
#   ./setup.sh          # Install deps + start server on port 8100
#   ./setup.sh install  # Install only
#   ./setup.sh start    # Start only (assumes installed)
#
# Then point VoxtralProvider to:
#   VoxtralProvider::local("http://localhost:8100")

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
VENV_DIR="$SCRIPT_DIR/.venv"
PORT="${MLX_VOICE_PORT:-8100}"

install() {
    echo "Creating virtual environment..."
    python3 -m venv "$VENV_DIR"
    source "$VENV_DIR/bin/activate"

    echo "Installing mlx-audio with server support..."
    pip install -q "mlx-audio[server]"

    echo "Pre-downloading models..."
    python3 -c "
from huggingface_hub import snapshot_download
snapshot_download('mlx-community/Voxtral-4B-TTS-2603-mlx-4bit')
snapshot_download('mlx-community/Voxtral-Mini-4B-Realtime-2602-4bit')
print('Models downloaded.')
"
    echo "Setup complete."
}

start() {
    source "$VENV_DIR/bin/activate"
    echo "Starting MLX voice server on port $PORT..."
    echo "  TTS: mlx-community/Voxtral-4B-TTS-2603-mlx-4bit"
    echo "  STT: mlx-community/Voxtral-Mini-4B-Realtime-2602-4bit"
    python3 -m mlx_audio.server --host 0.0.0.0 --port "$PORT"
}

case "${1:-}" in
    install)
        install
        ;;
    start)
        start
        ;;
    *)
        if [ ! -d "$VENV_DIR" ]; then
            install
        fi
        start
        ;;
esac
