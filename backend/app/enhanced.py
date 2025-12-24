import os
import logging
import uuid
import asyncio
import httpx
from pydub import AudioSegment
from typing import List, Dict, Optional
import json
import time

logger = logging.getLogger(__name__)

ENHANCED_ASR_URL = os.getenv("ENHANCED_ASR_URL", "http://enhanced-asr:8000")
WHISPER_SERVER_URL = os.getenv("WHISPER_SERVER_URL", "http://whisper-server:8178")

class EnhancedTranscriptionManager:
    def __init__(self):
        self.jobs = {}  # job_id -> {status, result, error, progress}

    async def start_job(self, file_path: str, language: str = None) -> str:
        job_id = str(uuid.uuid4())
        self.jobs[job_id] = {
            "status": "processing",
            "progress": 0,
            "created_at": time.time()
        }
        # Start processing in background
        asyncio.create_task(self._process_job(job_id, file_path, language))
        return job_id

    def get_job_status(self, job_id: str):
        return self.jobs.get(job_id)

    async def _process_job(self, job_id: str, file_path: str, language: str):
        try:
            logger.info(f"Starting enhanced transcription job {job_id} for {file_path}")
            
            # 1. Normalize Audio
            # Ensure temp dir exists
            os.makedirs("/tmp", exist_ok=True)
            
            audio = AudioSegment.from_file(file_path)
            audio = audio.set_frame_rate(16000).set_channels(1)
            
            duration_ms = len(audio)
            chunk_length_ms = 5 * 60 * 1000  # 5 minutes
            overlap_ms = 2000  # 2 seconds
            
            chunks = []
            # If audio is short, just one chunk
            if duration_ms <= chunk_length_ms:
                chunks.append((0, audio))
            else:
                for i in range(0, duration_ms, chunk_length_ms - overlap_ms):
                    # Ensure we don't go past end
                    end = min(i + chunk_length_ms, duration_ms)
                    chunk = audio[i:end]
                    chunks.append((i, chunk))
                    if end == duration_ms:
                        break
            
            total_chunks = len(chunks)
            all_segments = []
            
            async with httpx.AsyncClient(timeout=600.0) as client:
                for idx, (offset_ms, chunk) in enumerate(chunks):
                    # Export chunk to temp file
                    chunk_filename = f"/tmp/{job_id}_{idx}.wav"
                    chunk.export(chunk_filename, format="wav")
                    
                    try:
                        # Try enhanced-asr first
                        with open(chunk_filename, "rb") as f:
                            files = {"file": (f"chunk_{idx}.wav", f, "audio/wav")}
                            data = {"language": language} if language else {}
                            
                            try:
                                logger.info(f"Sending chunk {idx+1}/{total_chunks} to enhanced-asr")
                                response = await client.post(f"{ENHANCED_ASR_URL}/transcribe", files=files, data=data)
                                response.raise_for_status()
                                result = response.json()
                            except Exception as e:
                                logger.warning(f"Enhanced ASR failed for chunk {idx}, falling back to whisper-server: {e}")
                                # Fallback to whisper-server logic could go here
                                # For now, re-raise to fail the job
                                raise e

                        # Process segments
                        offset_sec = offset_ms / 1000.0
                        for seg in result.get("segments", []):
                            seg["start"] += offset_sec
                            seg["end"] += offset_sec
                            # Adjust word timestamps if present
                            if "words" in seg:
                                for w in seg["words"]:
                                    w["start"] += offset_sec
                                    w["end"] += offset_sec
                            all_segments.append(seg)
                            
                    finally:
                        if os.path.exists(chunk_filename):
                            os.remove(chunk_filename)
                    
                    # Update progress
                    self.jobs[job_id]["progress"] = int(((idx + 1) / total_chunks) * 100)
            
            # Merge and Deduplicate
            final_segments = self._merge_segments(all_segments)
            
            self.jobs[job_id]["status"] = "completed"
            self.jobs[job_id]["result"] = {"segments": final_segments}
            logger.info(f"Job {job_id} completed with {len(final_segments)} segments")
            
        except Exception as e:
            logger.error(f"Job {job_id} failed: {e}", exc_info=True)
            self.jobs[job_id]["status"] = "failed"
            self.jobs[job_id]["error"] = str(e)

    def _merge_segments(self, segments: List[Dict]) -> List[Dict]:
        if not segments:
            return []
            
        # Sort by start time
        segments.sort(key=lambda x: x["start"])
        
        merged = []
        for seg in segments:
            if not merged:
                merged.append(seg)
                continue
            
            last = merged[-1]
            
            # Check for exact text duplication (the loop bug)
            if seg["text"].strip() == last["text"].strip():
                continue
            
            # Check for overlap
            if seg["start"] < last["end"]:
                # If significant overlap and text is similar, it might be the overlap region
                # Simple heuristic: if start is within last 2 seconds of previous (overlap window)
                # and text is similar, skip.
                # For now, we just trust the new segment's start and append.
                # But we should probably truncate the previous segment if it overlaps too much.
                if seg["start"] < last["end"] - 0.1:
                    last["end"] = seg["start"]
            
            merged.append(seg)
            
        return merged

enhanced_manager = EnhancedTranscriptionManager()
