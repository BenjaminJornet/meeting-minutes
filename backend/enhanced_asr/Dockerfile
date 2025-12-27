FROM nvidia/cuda:12.8.1-cudnn-runtime-ubuntu22.04

WORKDIR /app

RUN apt-get update && apt-get install -y --no-install-recommends \
    python3 python3-venv python3-pip \
    git curl ca-certificates \
    ffmpeg \
    build-essential pkg-config \
    libsndfile1 \
 && rm -rf /var/lib/apt/lists/*

RUN python3 -m venv /app/venv
ENV PATH="/app/venv/bin:$PATH"
ENV PYTHONUNBUFFERED=1

COPY requirements.txt /app/requirements.txt

RUN pip install --upgrade pip setuptools wheel

# Ensure no preexisting torch installs and install CUDA-specific wheels explicitly
RUN pip uninstall -y torch torchvision torchaudio || true \
    && pip install --no-cache-dir --index-url https://download.pytorch.org/whl/cu128 \
      torch==2.8.0+cu128 torchvision==0.23.0+cu128 torchaudio==2.8.0+cu128

# Install remaining requirements (excluding torch/torchvision/torchaudio)
COPY constraints.txt /app/constraints.txt
RUN pip install --no-cache-dir -r /app/requirements.txt -c /app/constraints.txt

# For debugging during build: show installed versions
RUN python -c "import pkgutil, importlib, sys; import torch, torchvision; print('torch', torch.__version__); print('torchvision', torchvision.__version__); import subprocess; subprocess.run(['pip','list'])"

# Build-time sanity check for torchvision ops (nms)
RUN python -c "import torch, torchvision; from torchvision.ops import nms; print(torch.__version__, torchvision.__version__); print('nms ok')"

COPY main.py /app/main.py

EXPOSE 8000
CMD ["uvicorn", "main:app", "--host", "0.0.0.0", "--port", "8000"]