FROM python:3.12-slim
ARG ENGINE=qwen
RUN apt-get update && apt-get install -y --no-install-recommends ffmpeg libsndfile1 git build-essential
RUN if [ "$ENGINE" = pyannote31 ]; then \
      pip install --no-cache-dir torch==2.5.1 torchaudio==2.5.1 --index-url https://download.pytorch.org/whl/cu124; \
    else pip install --no-cache-dir torch torchaudio --index-url https://download.pytorch.org/whl/cu128; fi
RUN case "$ENGINE" in \
    qwen) pip install --no-cache-dir qwen-asr ;; \
    whisper) pip install --no-cache-dir faster-whisper ;; \
    canary|nemotron-diarization|parakeet) pip install --no-cache-dir 'nemo_toolkit[asr]' ;; \
    pyannote31) pip install --no-cache-dir 'numpy<2' 'pyannote.audio==3.3.2' 'huggingface_hub<1' ;; \
    community) pip install --no-cache-dir 'pyannote.audio>=4,<5' ;; \
    scoring) pip install --no-cache-dir pyannote.metrics pyannote.database ;; \
    *) exit 2 ;; esac
RUN pip install --no-cache-dir soundfile && pip freeze > /runtime-packages.txt
COPY run.py score.py score_diarization.py /bench/
ENTRYPOINT ["python", "/bench/run.py"]
