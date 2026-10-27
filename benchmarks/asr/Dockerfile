FROM python:3.12-slim
ARG ENGINE=qwen
RUN apt-get update && apt-get install -y --no-install-recommends ffmpeg libsndfile1 git build-essential
RUN if [ "$ENGINE" = pyannote31 ]; then \
      pip install --no-cache-dir torch==2.5.1 torchaudio==2.5.1 --index-url https://download.pytorch.org/whl/cu124; \
    else pip install --no-cache-dir torch torchaudio --index-url https://download.pytorch.org/whl/cu128; fi
RUN case "$ENGINE" in \
    qwen) pip install --no-cache-dir qwen-asr ;; \
    whisper) pip install --no-cache-dir faster-whisper 'av<19' ;; \
    canary|parakeet) pip install --no-cache-dir 'nemo_toolkit[asr]' ;; \
    nemotron-diarization) pip install --no-cache-dir 'nemo_toolkit[asr]==3.0.0' && \
      pip install --no-cache-dir --no-deps 'nemo-toolkit @ git+https://github.com/NVIDIA-NeMo/Speech.git@d6b28a9d1e05fba0ae4e8bd3f1a0bbc6df2051a8' && \
      pip install --no-cache-dir 'lhotse==2.0.0a6' ;; \
    pyannote31) pip install --no-cache-dir 'torch==2.5.1' 'torchaudio==2.5.1' 'numpy<2' 'pyannote.audio==3.3.2' 'huggingface_hub<1' ;; \
    community) pip install --no-cache-dir 'pyannote.audio>=4,<5' ;; \
    scoring) pip install --no-cache-dir pyannote.metrics pyannote.database ;; \
    *) exit 2 ;; esac
RUN pip install --no-cache-dir soundfile && pip freeze > /runtime-packages.txt
COPY run.py score.py score_diarization.py /bench/
ENTRYPOINT ["python", "/bench/run.py"]
