# 🐳 Meeting Minutes (Meetily) - Docker Installation Guide

This guide explains how to run the Meetily **backend services** with Docker, including GPU acceleration for fast transcription and AI summaries.

> **📱 Frontend Note**: The Meetily desktop app (Tauri) runs on your local machine, not in Docker.
> This is required because the app needs direct access to your microphone and system audio.
> See [REMOTE_DESKTOP_SETUP.md](REMOTE_DESKTOP_SETUP.md) for running the desktop app with a remote backend.

## 📋 Prerequisites

### Required
- **Docker Desktop** (Windows/macOS) or **Docker Engine** (Linux)
- **Git** for cloning the repository

### For GPU Acceleration (Recommended)
- **NVIDIA GPU** (RTX 20xx or newer recommended)
- **NVIDIA Driver** 525.60+ 
- **NVIDIA Container Toolkit** (for Docker GPU support)

#### Installing NVIDIA Container Toolkit (Linux)
```bash
# Add NVIDIA repository
distribution=$(. /etc/os-release;echo $ID$VERSION_ID)
curl -s -L https://nvidia.github.io/nvidia-docker/gpgkey | sudo apt-key add -
curl -s -L https://nvidia.github.io/nvidia-docker/$distribution/nvidia-docker.list | sudo tee /etc/apt/sources.list.d/nvidia-docker.list

# Install
sudo apt-get update
sudo apt-get install -y nvidia-container-toolkit
sudo systemctl restart docker
```

#### Windows with Docker Desktop
Docker Desktop for Windows automatically supports NVIDIA GPUs if you have:
- WSL2 backend enabled
- NVIDIA drivers installed on Windows
- GPU enabled in Docker Desktop settings

## 🚀 Quick Start (5 minutes)

### 1. Clone the Repository
```bash
git clone https://github.com/Zackriya-Solutions/meeting-minutes.git
cd meeting-minutes
```

### 2. Configure Environment

The project uses a **single centralized `.env` file** at the project root that is shared by both backend and frontend.

```bash
# From project root
cp .env.example .env
```

Edit `.env` to customize:
```env
# Shared URLs (used by both backend and frontend)
MEETILY_BACKEND_URL=http://localhost:5167
MEETILY_OLLAMA_URL=http://localhost:11434

# For GPU (default - recommended)
DOCKERFILE=Dockerfile.server-gpu
TAG=gpu

# For CPU only (slower, no GPU required)
# DOCKERFILE=Dockerfile.server-cpu
# TAG=cpu

# Enhanced ASR Configuration (Batch Processing)
# Model size for faster-whisper (tiny, base, small, medium, large-v2, large-v3)
ENHANCED_ASR_MODEL_SIZE=large-v3

# Language for transcription
MEETILY_LANGUAGE=fr  # Change to your language (en, de, es, etc.)

# Whisper model (larger = better accuracy, more VRAM)
WHISPER_MODEL=models/ggml-large-v3.bin
```

### 3. Create Docker Network
```bash
cd backend
docker network create meeting-minutes
```

### 4. Build and Start Services
```bash
# Build all images (first time only, takes ~10 minutes)
docker compose --env-file ../.env --profile default build

# Start all services
docker compose --env-file ../.env --profile default up -d
```

### 5. Download Whisper Model (First Run)
The model downloads automatically on first start. For large-v3 (~3GB), this takes a few minutes.

Check progress:
```bash
docker logs -f whisper-server
```

### 6. Verify Services Are Running

The following 3 containers should be running:
```bash
docker ps --format "table {{.Names}}\t{{.Status}}\t{{.Ports}}"
```

| Service | URL | Description |
|---------|-----|-------------|
| **whisper-server** | http://localhost:8178 | Whisper.cpp streaming ASR |
| **enhanced-asr** | http://localhost:8000 | Faster-whisper batch processing |
| **meetily-backend** | http://localhost:5167 | FastAPI backend (API at `/docs`) |

### 7. Build and Run the Desktop App
The frontend is a Tauri desktop app that runs locally on your machine.
See [REMOTE_DESKTOP_SETUP.md](REMOTE_DESKTOP_SETUP.md) for detailed instructions.

Quick start:
```bash
cd ../frontend
# The frontend uses the same root .env file automatically
# If you need to override settings, edit the root .env
pnpm install
pnpm run tauri:dev  # Development mode
# OR
pnpm run tauri:build  # Production build
```

## 📊 Service Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                    YOUR LOCAL MACHINE                       │
│  ┌────────────────────────────────────────────────────────┐ │
│  │           Meetily Desktop App (Tauri)                  │ │
│  │  • Captures audio from your microphone                 │ │
│  │  • Runs Whisper/Parakeet locally for transcription     │ │
│  │  • Sends transcripts to backend for storage            │ │
│  └────────────────────────────────────────────────────────┘ │
│                              │                              │
└──────────────────────────────│──────────────────────────────┘
                               │ Network (local or Tailscale)
                               ▼
┌─────────────────────────────────────────────────────────────┐
│           DOCKER SERVER (Windows/Linux + GPU)               │
│                    meeting-minutes network                  │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│  ┌─────────────────┐    ┌─────────────────────┐             │
│  │ meetily-backend │    │   whisper-server    │             │
│  │   (FastAPI)     │    │ (Whisper.cpp + CUDA)│             │
│  │   Port: 5167    │    │     Port: 8178      │             │
│  │  📁 Database   │    │   🎮 RTX 4090 GPU   │             │
│  └─────────────────┘    └─────────────────────┘             │
│           │                                                 │
│           ▼                                                 │
│  ┌──────────────────┐                                       │
│  │  enhanced-asr    │                                       │
│  │ (Faster-Whisper) │                                       │
│  │   Port: 8000     │                                       │
│  │ ⚡ Batch GPU ASR │                                       │
│  └──────────────────┘                                       │
│                                                             │
└─────────────────────────────────────────────────────────────┘
                              │
                              ▼
                    ┌─────────────────┐
                    │     Ollama      │
                    │ (host or Docker)│
                    │   Port: 11434   │
                    │  🤖 LLM for AI  │
                    └─────────────────┘
```

> **Note**: The whisper-server is optional if you run transcription locally on your machine.
> It's useful for batch processing or if your local machine doesn't have GPU acceleration.

## 🔧 Common Commands

### Start Services
```bash
cd backend
docker compose --env-file ../.env --profile default up -d
```

### Stop Services
```bash
docker compose --env-file ../.env --profile default down
```

### View Logs
```bash
# All services
docker compose --env-file ../.env logs -f

# Specific service
docker logs -f whisper-server
docker logs -f meetily-backend
```

### Rebuild After Code Changes
```bash
docker compose --env-file ../.env --profile default build --no-cache
docker compose --env-file ../.env --profile default up -d --force-recreate
```

### Check GPU Status
```bash
docker exec whisper-server nvidia-smi
```

## 🔄 Updating

```bash
# Pull latest code
git pull origin main

# Rebuild backend images
cd backend
docker compose --env-file ../.env --profile default build

# Restart with new images
docker compose --env-file ../.env --profile default up -d --force-recreate

# Rebuild desktop app (on your local machine)
cd ../frontend
pnpm install
pnpm run tauri:build
```

## 🐛 Troubleshooting

### GPU Not Detected
```bash
# Verify NVIDIA driver
nvidia-smi

# Verify Docker can see GPU
docker run --rm --gpus all nvidia/cuda:12.3.1-base-ubuntu22.04 nvidia-smi

# Check whisper-server logs
docker logs whisper-server | grep -i cuda
```

### Model Download Failed
```bash
# Manually download model
docker compose --env-file ../.env --profile download up model-downloader

# Or restart whisper-server
docker restart whisper-server
```

### Port Already in Use
Edit `.env` to change ports:
```env
WHISPER_PORT=8179
APP_PORT=5168
FRONTEND_PORT=3119
```

### Container Won't Start
```bash
# Check logs for errors
docker logs whisper-server

# Rebuild from scratch
docker compose --env-file ../.env --profile default down -v
docker compose --env-file ../.env --profile default build --no-cache
docker compose --env-file ../.env --profile default up -d
```

## 📁 Data Persistence

Data is stored in Docker volumes:
- `meeting-minutes_whisper_models` - Whisper AI models
- `meeting-minutes_whisper_uploads` - Temporary audio files
- `meeting-minutes_meeting_app_logs` - Application logs
- `./data/` - SQLite database (mounted from host)

### Backup Database
```bash
cp backend/data/meeting_minutes.db backup/
```

## 🌐 Remote Access

For using the desktop app on a different machine (e.g., Mac) while backend runs on your server:

See **[REMOTE_DESKTOP_SETUP.md](REMOTE_DESKTOP_SETUP.md)** for detailed instructions.

Quick overview:
1. Install Tailscale on both machines
2. Note your server's Tailscale IP (e.g., `100.64.0.4`)
3. Set `MEETILY_BACKEND_URL=http://100.64.0.4:5167` in frontend `.env`
4. Configure Ollama endpoint in app settings

## 📝 Configuration Reference

| Variable | Default | Description |
|--ENHANCED_ASR_PORT` | `8000` | Enhanced ASR service port |
| `ENHANCED_ASR_MODEL_SIZE` | `large-v3` | Model size for enhanced batch processing |
| `--------|---------|-------------|
| `DOCKERFILE` | `Dockerfile.server-cpu` | Dockerfile to use (gpu/cpu) |
| `TAG` | `cpu` | Image tag (gpu/cpu) |
| `WHISPER_MODEL` | `models/ggml-base.en.bin` | Whisper model path |
| `WHISPER_LANGUAGE` | `en` | Transcription language |
| `WHISPER_THREADS` | `0` | CPU threads (0=auto) |
| `WHISPER_PORT` | `8178` | Whisper server port |
| `APP_PORT` | `5167` | Backend API port |
| `OLLAMA_HOST` | `http://host.docker.internal:11434` | Ollama URL |

## 🎯 Performance Tips

1. **Use GPU**: 10-50x faster than CPU for Whisper server
2. **Local transcription on Mac**: Apple Silicon is excellent for Whisper/Parakeet via CoreML
3. **Choose the right model**:
   - `tiny` - Fastest, least accurate
   - `base` - Good balance for short audio
   - `large-v3` - Best accuracy, requires 4GB+ VRAM
4. **Allocate enough RAM**: 8GB minimum for large models
5. **Use SSD storage**: Faster model loading

## 📚 Related Documentation

- [REMOTE_DESKTOP_SETUP.md](REMOTE_DESKTOP_SETUP.md) - Run desktop app with remote backend
- [BUILDING.md](BUILDING.md) - Build from source
- [architecture.md](architecture.md) - System architecture details

---

Made with ❤️ by the Meetily community
