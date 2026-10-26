# ASR and diarization benchmarking

The shell examples below target Linux. For Windows with Docker Desktop/WSL2,
PowerShell commands, and a native Tauri rebuild, use the
[Windows Codex handoff](../../docs/WINDOWS_CODEX_HANDOFF.md).

These commands run isolated experiments without changing the application's
configured engines. Candidate GPU adapters still require execution on suitable
hardware before drawing conclusions about performance or accuracy.

## Prepare identical inputs

Use a Linux host with Docker, an NVIDIA GPU, and NVIDIA Container Toolkit. Open
a terminal in this directory. Put mono 16 kHz WAV files in `data/`: five extracts
of two to five minutes covering clear speech, terminology, noise, interruptions,
and French/English switching, plus a complete multi-hour meeting. Store corrected
transcripts with matching filenames in `references/` (`clear.txt` for `clear.wav`)
and speaker annotations in RTTM format. References must cover each complete input.
Use recordings you are authorized to process and keep them outside version control.

```bash
mkdir -p data references results
docker run --rm --gpus all nvidia/cuda:12.8.1-base-ubuntu24.04 nvidia-smi
```

Accept the Hugging Face access conditions for Community-1,
speaker-diarization-3.1, and segmentation-3.0 before running the pyannote adapters.
Provide a read-only token through the environment; do not store it in this guide
or a tracked configuration file:

```bash
read -rsp 'Read-only Hugging Face token: ' HF_TOKEN
export HF_TOKEN
```

## Build and run one engine at a time

Dependencies are installed inside images. Audio is mounted read-only, while
results and model caches use separate locations. Initial runs include downloads
and do not measure warm startup. Run experiments outside active recording sessions.

```bash
for engine in whisper qwen canary pyannote31 community nemotron-diarization; do
  docker build --build-arg ENGINE="$engine" -t "meetily-bench:$engine" . || break
  docker run --rm --gpus all --shm-size=2g \
    -e HF_TOKEN -e HF_HUB_DISABLE_TELEMETRY=1 \
    -v "$PWD/data:/data:ro" -v "$PWD/results:/results" \
    -v "meetily-bench-$engine-cache:/root/.cache" \
    "meetily-bench:$engine" "$engine" || break
done
unset HF_TOKEN
```

To start with **Qwen3-ASR 1.7B + ForcedAligner 0.6B** only:

```bash
docker build --build-arg ENGINE=qwen -t meetily-bench:qwen .
docker run --rm --gpus all --shm-size=2g \
  -e HF_HUB_DISABLE_TELEMETRY=1 \
  -v "$PWD/data:/data:ro" -v "$PWD/results:/results" \
  -v meetily-bench-qwen-cache:/root/.cache \
  meetily-bench:qwen qwen
```

For **Canary 1B v2**, substitute `canary` for `qwen` in both commands. For
**Community-1**, use `community` and add `-e HF_TOKEN`. For **Nemotron 3
Diarization**, use `nemotron-diarization`.

Each `*-packages.txt` file records resolved library versions. Preserve the Docker
images used for a comparison: changing model repositories or dependencies may
change future builds. Adapters follow the documented upstream APIs but are not
GPU-validated here. Preserve failures and resolved versions rather than silently
substituting engines. Local inputs, references, models, and results are ignored.

## Measurements and scoring

Each JSON records text or speaker turns, available timestamps, inference time,
real-time factor, process peak RSS, PyTorch peak allocation, and total GPU memory
sampled through `nvidia-smi`. Total GPU memory includes other services. PyTorch
allocation does not include Whisper's CTranslate2 memory. Measure once without
concurrent model generation and again under representative load; do not compare
only the PyTorch peaks. Inference time includes preprocessing and alignment, while
model loading is measured separately. The adapters explicitly request French.

```bash
docker run --rm --entrypoint python \
  -v "$PWD/results:/results:ro" -v "$PWD/references:/references:ro" \
  meetily-bench:qwen /bench/score.py /results /references
```

WER uses the same normalization for every candidate: lowercase, punctuation
removed, accents preserved. Insertions can make WER exceed 100%. Also review names,
numbers, and negation manually; WER can hide important factual errors. Diarization
adapters also write RTTM files. Compute DER against the same reference with zero
and 0.25-second collars, overlapping speech included, and optimal label matching.
Speaker labels are anonymous identifiers, not identified people.

```bash
docker build --build-arg ENGINE=scoring -t meetily-bench:scoring .
docker run --rm --entrypoint python \
  -v "$PWD/results:/results:ro" -v "$PWD/references:/references:ro" \
  meetily-bench:scoring /bench/score_diarization.py \
  /results/clear-community.rttm /references/clear.rttm
```

| Engine | WER | Names/numbers | DER / attribution | Timestamp error | Duration | Total GPU memory | Long recording |
|---|---|---|---|---|---|---|---|
| Whisper large-v3 | Pending | | | | | | |
| Qwen3-ASR + aligner | Pending | | | | | | |
| Canary v2 | Pending | | | | | | |
| pyannote 3.1 | N/A | | Pending | | | | |
| Community-1 | N/A | | Pending | | | | |
| Nemotron Diarization | N/A | | Pending | | | | |

Replay the long recording without manual splitting to reveal memory limits,
truncation, and speaker-identity drift. An out-of-memory failure or truncated
output is a failed run, not a usable speed measurement. Do not merge speaker IDs
from independent chunks without identity matching. Compare transcripts and RTTM
outputs against the same references before switching application engines. Qwen
and Community-1 are reasonable initial candidates; this harness establishes no
accuracy winner.

## Native live transcription

The GPU NeMo build does not reproduce Parakeet's native ONNX INT8 runtime.
Nemotron 3.5 ASR provides GGUF weights and a NeMo-Speech.cpp runtime; integration
into this application is not implemented. Docker measurements on macOS do not
validate Metal acceleration or Tauri pipeline latency. Compare native engines
with identical audio inside the application and measure accumulated latency,
CPU, memory, and dropped chunks. GPU-server results alone do not justify
replacing a live-transcription engine.

Download the candidate without installing packages on the host:

```bash
docker run --rm -v "$PWD/models:/models" python:3.12-slim sh -c \
  'pip install --no-cache-dir huggingface_hub && hf download nvidia/nemotron-3.5-asr-streaming-0.6b nemotron-3.5-asr-streaming-0.6b.q8_0.gguf --local-dir /models'
```

## Primary sources

- [Qwen3-ASR and alignment](https://huggingface.co/Qwen/Qwen3-ASR-1.7B)
- [Canary 1B v2](https://huggingface.co/nvidia/canary-1b-v2)
- [Community-1](https://huggingface.co/pyannote/speaker-diarization-community-1)
- [Nemotron 3 Diarization](https://huggingface.co/nvidia/Nemotron-3-Diarization)
- [Nemotron 3.5 ASR](https://huggingface.co/nvidia/nemotron-3.5-asr-streaming-0.6b)
- [NeMo-Speech.cpp runtime](https://github.com/NVIDIA/NeMo-Speech.cpp)
