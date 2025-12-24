from fastapi import FastAPI, UploadFile, File, Form, HTTPException
from fastapi.middleware.cors import CORSMiddleware
import whisperx
import os
import shutil
import tempfile
import logging
import torch
import gc

# Configure logging
logging.basicConfig(level=logging.INFO)
logger = logging.getLogger(__name__)

app = FastAPI(title="Enhanced ASR Service (WhisperX)")

# CORS
app.add_middleware(
    CORSMiddleware,
    allow_origins=["*"],
    allow_methods=["*"],
    allow_headers=["*"],
)

# Global variables
model = None
diarize_model = None
MODEL_SIZE = os.getenv("WHISPER_MODEL_SIZE", "large-v3")
DEVICE = "cuda" if torch.cuda.is_available() else "cpu"
COMPUTE_TYPE = "float16" if DEVICE == "cuda" else "int8"
HF_TOKEN = os.getenv("HF_TOKEN")

def get_model():
    global model
    if model is None:
        logger.info(f"Loading whisperx model: {MODEL_SIZE} on {DEVICE} with {COMPUTE_TYPE}")
        try:
            model = whisperx.load_model(MODEL_SIZE, DEVICE, compute_type=COMPUTE_TYPE)
            logger.info("Model loaded successfully")
        except Exception as e:
            logger.error(f"Failed to load model: {e}")
            raise
    return model

def get_diarize_model():
    global diarize_model
    if diarize_model is None and HF_TOKEN:
        logger.info("Loading diarization model...")
        try:
            diarize_model = whisperx.DiarizationPipeline(use_auth_token=HF_TOKEN, device=DEVICE)
            logger.info("Diarization model loaded")
        except Exception as e:
            logger.error(f"Failed to load diarization model: {e}")
            # Don't raise, just return None so we can proceed without diarization
            return None
    return diarize_model

@app.on_event("startup")
async def startup_event():
    # Preload model on startup
    try:
        get_model()
        if HF_TOKEN:
            get_diarize_model()
        else:
            logger.warning("HF_TOKEN not set. Diarization will be disabled.")
    except Exception as e:
        logger.warning(f"Could not preload models: {e}")

@app.post("/transcribe")
async def transcribe(
    file: UploadFile = File(...),
    language: str = Form(None),
    beam_size: int = Form(5),
    vad_filter: bool = Form(True)
):
    tmp_path = None
    try:
        # Save uploaded file to temp
        with tempfile.NamedTemporaryFile(delete=False, suffix=os.path.splitext(file.filename)[1]) as tmp:
            shutil.copyfileobj(file.file, tmp)
            tmp_path = tmp.name

        logger.info(f"Transcribing file: {file.filename}, language: {language}, device: {DEVICE}")
        
        # 1. Transcribe
        model_instance = get_model()
        audio = whisperx.load_audio(tmp_path)
        
        result = model_instance.transcribe(audio, batch_size=16, language=language)
        
        # 2. Align (Required for accurate word timestamps for diarization)
        logger.info("Aligning transcript...")
        model_a, metadata = whisperx.load_align_model(language_code=result["language"], device=DEVICE)
        result = whisperx.align(result["segments"], model_a, metadata, audio, DEVICE, return_char_alignments=False)
        
        # Cleanup alignment model to save VRAM
        del model_a
        gc.collect()
        if DEVICE == "cuda":
            torch.cuda.empty_cache()

        # 3. Diarize (if token available)
        diarize_instance = get_diarize_model()
        if diarize_instance:
            logger.info("Diarizing...")
            diarize_segments = diarize_instance(audio)
            result = whisperx.assign_word_speakers(diarize_segments, result)
        
        # Format output
        result_segments = []
        for segment in result["segments"]:
            result_segments.append({
                "start": segment["start"],
                "end": segment["end"],
                "text": segment["text"].strip(),
                "speaker": segment.get("speaker"), # Will be None if no diarization
                "words": [{
                    "start": w.get("start", 0),
                    "end": w.get("end", 0),
                    "word": w["word"],
                    "probability": w.get("score", 0) # whisperx uses 'score'
                } for w in segment.get("words", [])]
            })

        return {
            "language": result["language"],
            "segments": result_segments
        }

    except Exception as e:
        logger.error(f"Transcription error: {str(e)}")
        raise HTTPException(status_code=500, detail=str(e))
    finally:
        if tmp_path and os.path.exists(tmp_path):
            os.unlink(tmp_path)
        
        # Optional: aggressive GC
        gc.collect()
        if DEVICE == "cuda":
            torch.cuda.empty_cache()
        if 'tmp_path' in locals() and os.path.exists(tmp_path):
            os.unlink(tmp_path)
        raise HTTPException(status_code=500, detail=str(e))

@app.get("/health")
def health_check():
    return {"status": "ok", "device": DEVICE, "model": MODEL_SIZE}
