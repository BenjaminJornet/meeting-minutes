# Remote Desktop Setup for Meetily

This guide explains how to run Meetily with the **backend services on a remote server** (e.g., a Windows PC with GPU) while using the **desktop app on your local machine** (e.g., a MacBook).

## 🏗️ Architecture Overview

```
┌─────────────────────────────────────────────────────────────────┐
│                     YOUR LOCAL MACHINE (Mac)                    │
│                                                                 │
│  ┌──────────────────────────────────────────────────────────┐   │
│  │              Meetily Tauri Desktop App                   │   │
│  │  • Captures microphone audio locally                     │   │
│  │  • Runs Whisper/Parakeet locally (CoreML/Metal)          │   │
│  │  • Sends transcripts to remote backend                   │   │
│  └──────────────────────────────────────────────────────────┘   │
│                              │                                  │
│                              │ Tailscale VPN                    │
│                              ▼                                  │
└─────────────────────────────────────────────────────────────────┘
                               │
                               │ Network (Tailscale: 100.64.x.x)
                               │
                               ▼
┌─────────────────────────────────────────────────────────────────┐
│               REMOTE SERVER (Windows + GPU)                     │
│                                                                 │
│  ┌──────────────────┐  ┌──────────────────┐  ┌───────────────┐  │
│  │  meetily-backend │  │  whisper-server  │  │    Ollama     │  │
│  │   (FastAPI)      │  │     (GPU)        │  │   (GPU LLM)   │  │
│  │   Port: 5167     │  │   Port: 8178     │  │  Port: 11434  │  │
│  └────────┬─────────┘  └──────────────────┘  └───────────────┘  │
│           │                                          │          │
│           ▼                                          │          │
│  ┌──────────────────┐                                │          │
│  │   enhanced-asr   │                                │          │
│  │ (Faster-Whisper) │                                │          │
│  │   Port: 8000     │                                │          │
│  └──────────────────┘                                │          │
│           │                                          │          │
│           └──────────────┬───────────────────────────┘          │
│                          │                                      │
│               Docker Network: meeting-minutes                   │
└─────────────────────────────────────────────────────────────────┘
```

## 📋 Prerequisites

### On the Remote Server (Windows)
- Docker Desktop with NVIDIA GPU support
- NVIDIA GPU (e.g., RTX 4090)
- Tailscale installed and connected
- Note your Tailscale IP (e.g., `100.64.0.4`)

### On Your Local Machine (Mac)
- macOS 12+ (for CoreML/Metal acceleration)
- **Xcode** (full version from App Store, not just Command Line Tools) - required for CoreML/Metal build
- Rust toolchain installed (`curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh`)
- Node.js 18+ via nvm (`nvm install 20 && nvm use 20`)
- pnpm (`brew install pnpm`)
- CMake (`brew install cmake`)
- Tailscale installed and connected to the same network

> ⚠️ **Important**: After installing Xcode, run: `sudo xcode-select -s /Applications/Xcode.app/Contents/Developer`

## 🖥️ Step 1: Setup Remote Server (Windows)

### 1.1 Start Backend Services

```powershell
cd meeting-minutes/backend

# Create network if it doesn't exist
docker network create meeting-minutes

# Copy and configure environment
cp .env.example .env
# Edit .env if needed

# Build and start services
docker compose -p meeting-minutes --profile default build
docker compose -p meeting-minutes --profile default up -d
```

### 1.2 Verify Services

```powershell
# Check all containers are running
docker ps

# Verify GPU is detected
docker logs whisper-server 2>&1 | Select-String "CUDA"

# Test endpoints
curl http://localhost:5167/get-meetings
curl http://localhost:8178/
```

### 1.3 Install Ollama (Optional, for AI summaries)

```powershell
# Install Ollama on Windows
winget install Ollama.Ollama

# Pull a model
ollama pull llama3.2

# Verify it's running
curl http://localhost:11434/api/tags
```

### 1.4 Configure Firewall

Ensure these ports are accessible via Tailscale:
- `5167` - Meetily Backend API
- `11434` - Ollama API

> ⚠️ Port `8178` (Whisper Server) is NOT needed remotely - transcription runs locally on your Mac!

## 💻 Step 2: Build Desktop App on Mac

### 2.1 Clone and Setup

```bash
# Clone the repository
git clone https://github.com/Zackriya-Solutions/meeting-minutes.git
cd meeting-minutes/frontend

# Install dependencies
nvm use 20.15.1
pnpm install
```

### 2.2 Configure Remote Backend

Create your environment file:

```bash
cp .env.example .env
```

Edit `.env` with your server's Tailscale IP:

```bash
# .env
MEETILY_BACKEND_URL=http://100.64.0.4:5167
MEETILY_OLLAMA_URL=http://100.64.0.4:11434
```

### 2.3 Build the App

```bash
# Development mode (for testing)
pnpm run tauri:dev

# Production build
pnpm run tauri:build
```

The built app will be in `src-tauri/target/release/bundle/`.

### 2.4 Configure Ollama Endpoint in App

After launching Meetily:

1. Click the **Settings** icon (⚙️)
2. Go to **Model** tab
3. Set **Ollama Endpoint** to: `http://100.64.0.4:11434`
4. Click **Save**

## 🔧 Environment Variables Reference

| Variable | Description | Default | Example (Remote) |
|----------|-------------|---------|------------------|
| `MEETILY_BACKEND_URL` | FastAPI backend URL | `http://localhost:5167` | `http://100.64.0.4:5167` |
| `MEETILY_OLLAMA_URL` | Ollama LLM server URL | `http://localhost:11434` | `http://100.64.0.4:11434` |

> **Note**: Both variables can be set in `.env` file OR Ollama endpoint can be configured in the app's Settings UI.

## 🔍 Troubleshooting

### Cannot connect to backend

1. Verify Tailscale is connected on both machines:
   ```bash
   tailscale status
   ```

2. Test connectivity:
   ```bash
   curl http://100.64.0.4:5167/get-meetings
   ```

3. Check Windows firewall allows port 5167

### Ollama models not loading

1. Verify Ollama is running on remote server:
   ```bash
   curl http://100.64.0.4:11434/api/tags
   ```

2. Check you've pulled at least one model:
   ```powershell
   ollama list
   ```

### Transcription not working

Transcription runs **locally** on your Mac using Whisper or Parakeet. Ensure:

1. You've downloaded a transcription model in Settings → Transcription
2. Your Mac has sufficient RAM (8GB+ recommended)
3. For best performance on Mac, use Parakeet (optimized for Apple Silicon)

**Why local transcription is recommended:**
- Apple Silicon (M1/M2/M3) provides excellent performance via Metal/CoreML
- No network latency - instant real-time transcription
- Privacy: audio never leaves your machine
- Works offline

> 💡 **Tip**: If you have a `large-v3` Whisper model on your remote server but want to use it,
> the best approach is to download the same model locally. Mac M-series chips handle
> transcription extremely well, and you avoid network latency.

### App won't build on Mac

```bash
# Ensure Xcode is properly configured (not just Command Line Tools)
sudo xcode-select -s /Applications/Xcode.app/Contents/Developer

# Ensure Rust is up to date
rustup update

# Clean and rebuild
cd frontend
rm -rf node_modules src-tauri/target
nvm use 20.15.1
pnpm install
pnpm run tauri:build
```

## 📊 Performance Tips

1. **Use Parakeet on Mac** - It's optimized for Apple Silicon and provides excellent real-time transcription
2. **Use Whisper locally** - Apple Silicon makes local transcription fast, even with `large-v3` model
3. **Use Ollama on your GPU server** - Heavy LLM inference for summaries benefits from your RTX 4090
4. **Keep backend close to data** - Your meeting database stays on the server for centralized storage

## 🔒 Security Notes

- Tailscale provides encrypted point-to-point connections
- No data transits the public internet
- Transcription happens locally - audio never leaves your Mac
- Only transcripts and meeting metadata are sent to the backend
