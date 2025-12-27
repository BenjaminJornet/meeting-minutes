import os
import gc
import shutil
import tempfile
import logging
from typing import Optional, Dict, Any, List

import torch
from fastapi import FastAPI, UploadFile, File, Form, HTTPException
from fastapi.middleware.cors import CORSMiddleware

import whisperx
from pyannote.audio import Pipeline
import time
import subprocess
import sys
import signal
import traceback
import threading
import faulthandler
import asyncio


# -----------------------------
# Logging
# -----------------------------
logging.basicConfig(level=logging.INFO)
logger = logging.getLogger("asr-service")


# -----------------------------
# PyTorch 2.6+ weights_only workaround (trusted checkpoints)
# -----------------------------
def patch_torch_load_for_trusted_checkpoints() -> None:
    """
    PyTorch >=2.6 sets torch.load(weights_only=True) by default.
    pyannote/lightning can therefore fail loading older checkpoints requiring pickled classes (OmegaConf).
    We force weights_only=False when TORCH_FORCE_NO_WEIGHTS_ONLY_LOAD=true.
    """
    flag = os.getenv("TORCH_FORCE_NO_WEIGHTS_ONLY_LOAD", "").strip().lower()
    if flag not in {"1", "true", "yes", "y", "on"}:
        return

    orig_load = torch.load

    def patched_load(*args, **kwargs):
        # OVERRIDE HARD: even if caller passed weights_only=True explicitly
        kwargs["weights_only"] = False
        return orig_load(*args, **kwargs)

    torch.load = patched_load  # type: ignore[assignment]
    logger.warning("Patched torch.load: all loads will use weights_only=False (trusted checkpoints).")

    # Optional: allowlist OmegaConf globals if API exists (harmless even with weights_only=False)
    try:
        import omegaconf

        if hasattr(torch.serialization, "add_safe_globals"):
            torch.serialization.add_safe_globals(
                [
                    omegaconf.listconfig.ListConfig,
                    omegaconf.dictconfig.DictConfig,
                ]
            )
            logger.info("Added OmegaConf globals to torch.serialization safe globals.")
    except Exception as e:
        logger.debug(f"Could not add safe globals (optional): {e}")


patch_torch_load_for_trusted_checkpoints()


# -----------------------------
# App config
# -----------------------------
MODEL_SIZE = os.getenv("WHISPER_MODEL_SIZE", "large-v3")
DEVICE = os.getenv("DEVICE", "cuda" if torch.cuda.is_available() else "cpu")
COMPUTE_TYPE = os.getenv("COMPUTE_TYPE", "float16" if DEVICE == "cuda" else "float32")
BATCH_SIZE = int(os.getenv("BATCH_SIZE", "16"))
HF_TOKEN = os.getenv("HF_TOKEN", "").strip() or None
VAD_METHOD = os.getenv("WHISPERX_VAD_METHOD", "silero")

PYANNOTE_DIAR_MODEL = "pyannote/speaker-diarization-3.1"

# -----------------------------
# Globals (cached models)
# -----------------------------
asr_model = None
diar_pipeline = None

# Per-client job locks for serializing chunk processing per upload
job_locks: Dict[str, asyncio.Lock] = {}

app = FastAPI(title="Enhanced ASR Service (WhisperX + Pyannote diarization)")

app.add_middleware(
    CORSMiddleware,
    allow_origins=["*"],
    allow_methods=["*"],
    allow_headers=["*"],
)


def get_asr_model():
    global asr_model
    if asr_model is None:
        logger.info(f"🚀 Loading whisperx model='{MODEL_SIZE}' device='{DEVICE}' compute_type='{COMPUTE_TYPE}' vad_method='{VAD_METHOD}'")
        asr_model = whisperx.load_model(
            MODEL_SIZE, 
            DEVICE, 
            compute_type=COMPUTE_TYPE,
            vad_method=VAD_METHOD
        )
        logger.info("✅ ASR model loaded.")
    return asr_model


def get_diar_pipeline():
    global diar_pipeline
    if diar_pipeline is not None:
        return diar_pipeline

    if not HF_TOKEN:
        logger.warning("⚠️ HF_TOKEN not set. Diarization disabled.")
        return None

    logger.info(f"🚀 Loading pyannote Pipeline '{PYANNOTE_DIAR_MODEL}' ...")
    # HF examples accept token/use_auth_token; token works on recent HF clients
    # pyannote.audio 3.x uses use_auth_token
    diar_pipeline = Pipeline.from_pretrained(PYANNOTE_DIAR_MODEL, use_auth_token=HF_TOKEN)

    # Try to move to desired device; if that fails (e.g., cuDNN mismatch), explicitly fallback to CPU
    try:
        diar_pipeline.to(torch.device(DEVICE))
        logger.info(f"✅ Moved diarization pipeline to {DEVICE}")
    except Exception as e:
        logger.warning(f"⚠️ Could not move diarization pipeline to {DEVICE}: {e}")
        try:
            diar_pipeline.to(torch.device("cpu"))
            logger.info("⚠️ Moved diarization pipeline to cpu as a fallback")
        except Exception as e2:
            logger.warning(f"❌ Could not move diarization pipeline to cpu: {e2}; disabling diarization")
            return None

    logger.info("✅ Diarization pipeline loaded.")
    return diar_pipeline


def _log_gpu_stats(prefix: str = ""):
    try:
        if torch.cuda.is_available():
            dev = torch.cuda.current_device()
            mem_alloc = torch.cuda.memory_allocated(dev)
            mem_reserved = torch.cuda.memory_reserved(dev)
            logger.info(f"📊 {prefix} torch.cuda available: device={dev}, allocated={mem_alloc}, reserved={mem_reserved}, device_count={torch.cuda.device_count()}, cuda_version={torch.version.cuda}")
            # run a short nvidia-smi query for host-level metrics
            try:
                out = subprocess.check_output([
                    "nvidia-smi",
                    "--query-gpu=index,name,utilization.gpu,memory.used",
                    "--format=csv,noheader,nounits",
                ], text=True, stderr=subprocess.DEVNULL)
                logger.info(f"📊 {prefix} nvidia-smi: {out.strip()}")
            except Exception as e:
                logger.debug(f"🔍 {prefix} nvidia-smi failed: {e}")
        else:
            logger.info(f"ℹ️ {prefix} torch.cuda not available")
    except Exception as e:
        logger.warning(f"⚠️ {prefix} Could not log GPU stats: {e}")


def _dump_all_thread_traces(prefix: str = ""):
    """Dump stack traces for all threads and a small faulthandler dump for post-mortem."""
    try:
        logger.warning(f"⚠️ {prefix} Dumping all thread stacks")
        for tid, frame in sys._current_frames().items():
            logger.warning(f"⚠️ Thread {tid} stack:\n" + "\n".join(traceback.format_stack(frame)))
        # Faulthandler write to stderr - ensure it is enabled
        try:
            faulthandler.dump_traceback(all_threads=True)
        except Exception as e:
            logger.debug(f"🔍 faulthandler.dump_traceback failed: {e}")
    except Exception as e:
        logger.warning(f"⚠️ {prefix} Could not dump threads: {e}")


def _signal_handler(signum, frame):
    try:
        logger.error(f"❌ Received signal {signum}; dumping traces before exit")
        _dump_all_thread_traces(prefix=f"signal {signum}")
    finally:
        # Re-raise the signal default to allow normal termination
        signal.signal(signum, signal.SIG_DFL)
        os.kill(os.getpid(), signum)


def _install_crash_handlers():
    try:
        faulthandler.enable()
        # Register SIGTERM and SIGINT handlers to capture stacks on termination
        for s in (signal.SIGTERM, signal.SIGINT):
            try:
                signal.signal(s, _signal_handler)
            except Exception as e:
                logger.debug(f"Could not register handler for signal {s}: {e}")
        # Register excepthook
        def _ex_hook(exc_type, exc, tb):
            logger.error("❌ Uncaught exception", exc_info=(exc_type, exc, tb))
            _dump_all_thread_traces(prefix="uncaught-exception")
            # call default
            sys.__excepthook__(exc_type, exc, tb)
        sys.excepthook = _ex_hook
        logger.info("✅ Crash handlers installed (faulthandler + signal + excepthook)")
    except Exception as e:
        logger.warning(f"⚠️ Failed to install crash handlers: {e}")


@app.get("/debug-gpu")
def debug_gpu():
    """Return quick GPU/Torch diagnostics for debugging."""
    info = {
        "torch_cuda_available": torch.cuda.is_available(),
        "torch_version": torch.__version__,
        "cuda_version": torch.version.cuda if hasattr(torch, "version") else None,
        "device_count": torch.cuda.device_count() if torch.cuda.is_available() else 0,
    }
    try:
        out = subprocess.check_output([
            "nvidia-smi",
            "--query-gpu=index,name,utilization.gpu,memory.used,memory.total",
            "--format=csv,noheader,nounits",
        ], text=True)
        info["nvidia-smi"] = [line.strip() for line in out.strip().splitlines()]
    except Exception as e:
        info["nvidia-smi-error"] = str(e)
    return info


import multiprocessing
import uuid

# Ensure a safe multiprocessing start method for CUDA (use 'spawn' to avoid forking issues)
try:
    multiprocessing.set_start_method('spawn', force=False)
    logger.info("✅ Set multiprocessing start method to 'spawn'")
except RuntimeError:
    logger.debug("🔍 Multiprocessing start method already set")

# Worker settings
WORKER_TIMEOUT = int(os.getenv("WORKER_TIMEOUT", "1200"))  # seconds (default 20 minutes)
_worker_request_queue: Optional[multiprocessing.Queue] = None
_worker_process: Optional[multiprocessing.Process] = None
_monitor_stop_event: Optional[threading.Event] = None


def _child_process_transcribe(child_conn, tmp_path_c, language_c, diarize_c):
    """Top-level child process target to perform a transcription and send back the result."""
    try:
        res = _do_transcribe_impl(tmp_path_c, language_c, diarize_c)
        child_conn.send(("ok", res))
    except Exception as e:
        try:
            child_conn.send(("error", str(e)))
        except Exception:
            pass
    finally:
        try:
            child_conn.close()
        except Exception:
            pass


def _child_process_diarize(child_conn, tmp_path_c):
    """Top-level child process target to perform diarization only and send back turns."""
    try:
        pipeline = get_diar_pipeline()
        if pipeline is None:
            child_conn.send(("error", "Diarization not configured (missing HF_TOKEN or pipeline load failure)"))
            return
        diar = pipeline(tmp_path_c)
        turns = []
        for turn, _, speaker in diar.itertracks(yield_label=True):
            turns.append({"start": float(turn.start), "end": float(turn.end), "speaker": str(speaker)})
        child_conn.send(("ok", {"turns": turns}))
    except Exception as e:
        try:
            child_conn.send(("error", str(e)))
        except Exception:
            pass
    finally:
        try:
            child_conn.close()
        except Exception:
            pass


class InferenceWorker(multiprocessing.Process):
    """Persistent inference worker process that preloads models and executes transcriptions.

    The main server sends jobs via a Queue: (job_id, tmp_path, language, diarize, child_conn)
    The worker sends back (status, payload) over the provided Pipe connection.
    """

    def __init__(self, request_queue: multiprocessing.Queue):
        # Worker must not be daemonic so it can spawn child processes for isolation
        super().__init__(daemon=False)
        self.request_queue = request_queue

    def run(self):
        # Install crash handlers in worker as well
        try:
            _install_crash_handlers()
        except Exception:
            pass

        logger.info(f"🚀 Inference worker starting (pid={os.getpid()})")
        # Preload models in worker
        try:
            get_asr_model()
            get_diar_pipeline()
        except Exception as e:
            logger.exception(f"❌ Worker preloading failed: {e}")

        while True:
            job = None
            try:
                job = self.request_queue.get()
            except Exception as e:
                logger.exception(f"❌ Worker queue get failed: {e}")
                break

            if job is None:
                logger.info("🛑 Worker received shutdown signal")
                break

            job_id, tmp_path, language, diarize, conn = job
            try:
                logger.info(f"⚙️ Worker processing job {job_id} file={tmp_path} lang={language} diarize={diarize}")

                parent_child_conn, child_conn = multiprocessing.Pipe(duplex=False)
                p = multiprocessing.Process(target=_child_process_transcribe, args=(child_conn, tmp_path, language, diarize), daemon=False)
                p.start()
                logger.info(f"🚀 Started child process pid={p.pid} for job {job_id}")

                # Wait for result with timeout
                waited = 0
                poll_interval = 0.5
                while waited < WORKER_TIMEOUT and not parent_child_conn.poll(poll_interval):
                    waited += poll_interval
                if parent_child_conn.poll(0):
                    status, payload = parent_child_conn.recv()
                    if status == "ok":
                        try:
                            conn.send(("ok", payload))
                        except Exception as e:
                            logger.warning(f"⚠️ Worker failed to send response for job {job_id}: {e}")
                    else:
                        try:
                            conn.send(("error", payload))
                        except Exception:
                            pass
                else:
                    # Timeout - child is unresponsive, terminate
                    try:
                        p.terminate()
                        logger.warning(f"⚠️ Child process pid={p.pid} timed out and was terminated for job {job_id}")
                    except Exception as e:
                        logger.exception(f"❌ Failed to terminate child process pid={p.pid}: {e}")
                    try:
                        conn.send(("error", "Child transcription timed out"))
                    except Exception:
                        pass

                # Ensure child has exited
                p.join(timeout=1)
                exitcode = getattr(p, "exitcode", None)
                if exitcode and exitcode != 0:
                    logger.warning(f"⚠️ Child process pid={p.pid} exited with code {exitcode} for job {job_id}")

            except Exception as e:
                logger.exception(f"❌ Worker job {job_id} failed: {e}")
                try:
                    conn.send(("error", str(e)))
                except Exception:
                    pass

        logger.info("🛑 Inference worker exiting")


@app.on_event("startup")
async def startup_event():
    try:
        _install_crash_handlers()
        logger.info(f"ℹ️ Process PID={os.getpid()}, thread_count={threading.active_count()}")
        # Start worker process
        try:
            global _worker_request_queue, _worker_process, _monitor_stop_event
            _worker_request_queue = multiprocessing.Queue()
            _worker_process = InferenceWorker(_worker_request_queue)
            _worker_process.start()
            logger.info(f"🚀 Started inference worker pid={_worker_process.pid}")
        except Exception as e:
            logger.warning(f"⚠️ Could not start inference worker: {e}")

        # Start monitor thread to keep worker alive (attempt restarts)
        try:
            _monitor_stop_event = threading.Event()

            def _monitor():
                backoff = 1
                while not _monitor_stop_event.is_set():
                    try:
                        global _worker_request_queue, _worker_process
                        if _worker_process is None or not _worker_process.is_alive():
                            exitcode = getattr(_worker_process, "exitcode", None)
                            logger.warning(f"⚠️ Inference worker not alive; exitcode={exitcode}; attempting restart")
                            try:
                                # recreate queue and worker
                                _worker_request_queue = multiprocessing.Queue()
                                _worker_process = InferenceWorker(_worker_request_queue)
                                _worker_process.start()
                                logger.info(f"🚀 Restarted inference worker pid={_worker_process.pid}")
                                backoff = 1
                            except Exception as e:
                                logger.exception(f"❌ Failed to restart worker: {e}")
                                time.sleep(backoff)
                                backoff = min(backoff * 2, 60)
                        else:
                            backoff = 1
                    except Exception as e:
                        logger.exception(f"❌ Worker monitor error: {e}")
                    # Poll interval
                    _monitor_stop_event.wait(5)

            thr = threading.Thread(target=_monitor, daemon=True)
            thr.start()
            logger.info("✅ Started inference worker monitor thread")
        except Exception as e:
            logger.warning(f"⚠️ Could not start worker monitor: {e}")

        # NOTE: we avoid preloading heavy models in the main process to reduce memory pressure.
        # The inference worker preloads models and handles requests; a local fallback will load models on-demand if necessary.
    except Exception as e:
        logger.warning(f"⚠️ Could not preload models: {e}")


def assign_segment_speaker(seg_start: float, seg_end: float, turns: List[Dict[str, Any]]) -> Optional[str]:
    """
    Pick speaker with max overlap between [seg_start, seg_end] and diarization turns.
    """
    best_spk = None
    best_overlap = 0.0
    for t in turns:
        s = max(seg_start, t["start"])
        e = min(seg_end, t["end"])
        overlap = max(0.0, e - s)
        if overlap > best_overlap:
            best_overlap = overlap
            best_spk = t["speaker"]
    return best_spk


def _do_transcribe_impl(tmp_path: str, language: Optional[str], diarize: bool):
    """Shared implementation of transcription used by both worker and local fallback."""
    model = get_asr_model()
    audio = whisperx.load_audio(tmp_path)

    # 1) ASR
    _log_gpu_stats("before ASR")
    t0 = time.time()
    result = model.transcribe(
        audio,
        batch_size=BATCH_SIZE,
        language=language,
    )
    t1 = time.time()
    logger.info(f"✅ ASR complete: duration={t1-t0:.2f}s")
    _log_gpu_stats("after ASR")

    # 2) Alignment (word timestamps)
    logger.info("⏳ Aligning...")
    _log_gpu_stats("before alignment")
    model_a, metadata = whisperx.load_align_model(language_code=result["language"], device=DEVICE)
    aligned = whisperx.align(
        result["segments"],
        model_a,
        metadata,
        audio,
        DEVICE,
        return_char_alignments=False,
    )
    _log_gpu_stats("after alignment")
    try:
        del model_a
    except Exception:
        pass
    gc.collect()
    if DEVICE == "cuda":
        torch.cuda.empty_cache()

    # 3) Diarization (pyannote)
    turns = []
    if diarize:
        pipeline = get_diar_pipeline()
        if pipeline is not None:
            logger.info("🗣️ Diarizing...")
            _log_gpu_stats("before diarization")
            t_d0 = time.time()
            diar = pipeline(tmp_path)
            t_d1 = time.time()
            _log_gpu_stats("after diarization")
            logger.info(f"✅ Diarization complete: duration={t_d1-t_d0:.2f}s")
            for turn, _, speaker in diar.itertracks(yield_label=True):
                turns.append({"start": float(turn.start), "end": float(turn.end), "speaker": str(speaker)})

    # 4) Format output (+ assign speakers)
    out_segments = []
    for seg in aligned["segments"]:
        seg_start = float(seg["start"])
        seg_end = float(seg["end"])
        spk = assign_segment_speaker(seg_start, seg_end, turns) if turns else None

        words_out = []
        for w in seg.get("words", []) or []:
            w_start = float(w.get("start", 0.0) or 0.0)
            w_end = float(w.get("end", 0.0) or 0.0)
            w_spk = assign_segment_speaker(w_start, w_end, turns) if turns else spk

            words_out.append(
                {
                    "start": w_start,
                    "end": w_end,
                    "word": w.get("word", ""),
                    "probability": float(w.get("score", 0.0) or 0.0),
                    "speaker": w_spk,
                }
            )

        out_segments.append(
            {
                "start": seg_start,
                "end": seg_end,
                "text": (seg.get("text") or "").strip(),
                "speaker": spk,
                "words": words_out,
            }
        )

    return {"language": aligned.get("language", result["language"]), "segments": out_segments}


@app.post("/transcribe")
async def transcribe(
    file: UploadFile = File(...),
    language: Optional[str] = Form(None),
    diarize: bool = Form(True),
    client_job_id: Optional[str] = Form(None),
    chunk_index: Optional[int] = Form(None),
    job_id: Optional[str] = Form(None),  # legacy field name accepted for backward compatibility
):
    # Accept either 'client_job_id' or legacy 'job_id' from clients
    client_job_id = client_job_id or job_id
    if not client_job_id:
        logger.warning("⚠️ No client_job_id received for transcribe request; chunk serialization disabled")
    tmp_path = None
    lock = None
    try:
        # If client provided a job id, serialize requests for that job to ensure chunk-by-chunk processing
        if client_job_id:
            lock = job_locks.setdefault(client_job_id, asyncio.Lock())
            logger.info(f"⏳ Waiting on lock for client_job_id={client_job_id} chunk={chunk_index}")
            await lock.acquire()
            logger.info(f"🔒 Acquired lock for client_job_id={client_job_id} chunk={chunk_index}")

        # Save upload
        suffix = os.path.splitext(file.filename or "audio.wav")[1] or ".wav"
        with tempfile.NamedTemporaryFile(delete=False, suffix=suffix) as tmp:
            shutil.copyfileobj(file.file, tmp)
            tmp_path = tmp.name

        logger.info(f"📝 Transcribing '{file.filename}' language={language} device={DEVICE} client_job_id={client_job_id} chunk_index={chunk_index}")

        # Try to delegate to worker if available
        try:
            global _worker_request_queue, _worker_process
            if _worker_request_queue is not None and _worker_process is not None and _worker_process.is_alive():
                parent_conn, child_conn = multiprocessing.Pipe(duplex=False)
                job_id = str(uuid.uuid4())
                _worker_request_queue.put((job_id, tmp_path, language, diarize, child_conn))
                logger.info(f"📤 Delegated job {job_id} to worker pid={_worker_process.pid}")
                if parent_conn.poll(WORKER_TIMEOUT):
                    status, payload = parent_conn.recv()
                    if status == "ok":
                        return payload
                    else:
                        raise Exception(f"Worker error: {payload}")
                else:
                    raise Exception("Worker timeout")
        except Exception as e:
            logger.warning(f"⚠️ Worker path failed or unavailable: {e}; falling back to local inference")

        # Local (in-process) inference fallback
        res = _do_transcribe_impl(tmp_path, language, diarize)
        return res

    except HTTPException:
        raise
    except Exception as e:
        logger.exception(f"❌ Transcription error: {e}")
        raise HTTPException(status_code=500, detail=str(e))
    finally:
        if tmp_path and os.path.exists(tmp_path):
            os.unlink(tmp_path)
        gc.collect()
        if DEVICE == "cuda":
            torch.cuda.empty_cache()
        # Release client job lock if we acquired one
        if lock is not None:
            try:
                lock.release()
                logger.info(f"🔓 Released lock for client_job_id={client_job_id} chunk={chunk_index}")
            except Exception:
                pass
            try:
                if not lock.locked():
                    del job_locks[client_job_id]
            except Exception:
                pass

@app.get("/health")
def health_check():
    worker_ok = False
    try:
        global _worker_process
        worker_ok = _worker_process is not None and _worker_process.is_alive()
    except Exception:
        worker_ok = False

    return {
        "status": "ok",
        "device": DEVICE,
        "compute_type": COMPUTE_TYPE,
        "model": MODEL_SIZE,
        "diarization": bool(HF_TOKEN),
        "pyannote_model": PYANNOTE_DIAR_MODEL,
        "inference_worker": "alive" if worker_ok else "dead",
    }


@app.post("/diarize")
async def diarize_endpoint(
    file: UploadFile = File(...),
):
    """Perform diarization only (pyannote pipeline) in an isolated child process and return speaker turns.

    This avoids doing diarization per chunk, which blocks chunk-by-chunk ASR and causes large delays.
    """
    tmp_path = None
    try:
        with tempfile.NamedTemporaryFile(delete=False, suffix=".wav") as tmp:
            shutil.copyfileobj(file.file, tmp)
            tmp_path = tmp.name

        parent_conn, child_conn = multiprocessing.Pipe(duplex=False)
        p = multiprocessing.Process(target=_child_process_diarize, args=(child_conn, tmp_path), daemon=False)
        p.start()

        waited = 0
        poll_interval = 0.5
        while waited < WORKER_TIMEOUT and not parent_conn.poll(poll_interval):
            waited += poll_interval
        if parent_conn.poll(0):
            status, payload = parent_conn.recv()
            if status == "ok":
                return payload
            else:
                raise Exception(payload)
        else:
            try:
                p.terminate()
            except Exception:
                pass
            raise Exception("Diarization timeout")

    except Exception as e:
        logger.exception(f"❌ Diarization error: {e}")
        raise HTTPException(status_code=500, detail=str(e))
    finally:
        if tmp_path and os.path.exists(tmp_path):
            os.unlink(tmp_path)



@app.on_event("shutdown")
async def shutdown_event():
    try:
        global _worker_request_queue, _worker_process, _monitor_stop_event
        # Stop monitor thread
        try:
            if _monitor_stop_event is not None:
                _monitor_stop_event.set()
        except Exception:
            pass

        if _worker_request_queue is not None:
            try:
                _worker_request_queue.put(None)
            except Exception:
                pass
        if _worker_process is not None:
            try:
                _worker_process.join(timeout=5)
            except Exception:
                pass
    except Exception as e:
        logger.warning(f"⚠️ Error during shutdown: {e}")