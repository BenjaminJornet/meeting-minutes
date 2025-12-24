from fastapi import FastAPI, UploadFile, File, Form, HTTPException
from fastapi.middleware.cors import CORSMiddleware
from faster_whisper import WhisperModel
import os
import shutil
import tempfile
import logging
import torch

# Configure logging
logging.basicConfig(level=logging.INFO)
logger = logging.getLogger(__name__)

app = FastAPI(title="Enhanced ASR Service")

# CORS
app.add_middleware(
    CORSMiddleware,
    allow_origins=["*"],
    allow_methods=["*"],
    allow_headers=["*"],
)

# Global model variable
model = None
MODEL_SIZE = os.getenv("WHISPER_MODEL_SIZE", "large-v3")
DEVICE = "cuda" if torch.cuda.is_available() else "cpu"
COMPUTE_TYPE = "float16" if DEVICE == "cuda" else "int8"

def get_model():
    global model
    if model is None:
        logger.info(f"Loading faster-whisper model: {MODEL_SIZE} on {DEVICE} with {COMPUTE_TYPE}")
        try:
            model = WhisperModel(MODEL_SIZE, device=DEVICE, compute_type=COMPUTE_TYPE)
            logger.info("Model loaded successfully")
        except Exception as e:
            logger.error(f"Failed to load model: {e}")
            raise
    return model

@app.on_event("startup")
async def startup_event():
    # Preload model on startup
    try:
        get_model()
    except Exception as e:
        logger.warning(f"Could not preload model: {e}")

@app.post("/transcribe")
async def transcribe(
    file: UploadFile = File(...),
    language: str = Form(None),
    beam_size: int = Form(5),
    vad_filter: bool = Form(True)
):
    try:
        # Save uploaded file to temp
        with tempfile.NamedTemporaryFile(delete=False, suffix=os.path.splitext(file.filename)[1]) as tmp:
            shutil.copyfileobj(file.file, tmp)
            tmp_path = tmp.name

        logger.info(f"Transcribing file: {file.filename}, language: {language}, device: {DEVICE}")
        
        model_instance = get_model()
        
        segments, info = model_instance.transcribe(
            tmp_path,
            beam_size=beam_size,
            language=language,
            vad_filter=vad_filter,
            word_timestamps=True
        )

        # Convert generator to list immediately to process
        result_segments = []
        for segment in segments:
            result_segments.append({
                "start": segment.start,
                "end": segment.end,
                "text": segment.text,
                "words": [{
                    "start": w.start,
                    "end": w.end,
                    "word": w.word,
                    "probability": w.probability
                } for w in segment.words] if segment.words else []
            })

        os.unlink(tmp_path)
        
        return {
            "language": info.language,
            "language_probability": info.language_probability,
            "duration": info.duration,
            "segments": result_segments
        }

    except Exception as e:
        logger.error(f"Transcription error: {str(e)}")
        if 'tmp_path' in locals() and os.path.exists(tmp_path):
            os.unlink(tmp_path)
        raise HTTPException(status_code=500, detail=str(e))

@app.get("/health")
def health_check():
    return {"status": "ok", "device": DEVICE, "model": MODEL_SIZE}
