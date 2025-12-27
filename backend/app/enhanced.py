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
            logger.info(f"🚀 Starting enhanced transcription job {job_id} for {file_path}")
            
            # 1. Normalize Audio
            # Ensure temp dir exists
            os.makedirs("/tmp", exist_ok=True)
            
            audio = AudioSegment.from_file(file_path)
            audio = audio.set_frame_rate(16000).set_channels(1)
            
            duration_ms = len(audio)
            # Use smaller chunks to keep per-chunk processing time reasonable
            chunk_length_ms = int(os.getenv("CHUNK_LENGTH_MS", str(2 * 60 * 1000)))  # default 2 minutes (configurable via CHUNK_LENGTH_MS)
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
                # Wait briefly for enhanced-asr health if it's restarting (helps avoid transient connection drops)
                async def _wait_for_health(client, url: str, tries: int = 3, delay: float = 1.0) -> bool:
                    for attempt in range(1, tries + 1):
                        try:
                            resp = await client.get(url, timeout=5.0)
                            if resp.status_code == 200:
                                return True
                        except Exception:
                            logger.debug(f"🔍 Health check attempt {attempt}/{tries} to {url} failed")
                        await asyncio.sleep(delay)
                    return False

                for idx, (offset_ms, chunk) in enumerate(chunks):
                    # Export chunk to temp file
                    chunk_filename = f"/tmp/{job_id}_{idx}.wav"
                    chunk.export(chunk_filename, format="wav")
                    
                    try:
                        # Try enhanced-asr first with retries and fallback to whisper-server on transient errors
                        async def _post_with_retries(url: str, fobj, files_field_name: str = "file", data: dict = None, tries: int = 3):
                            data = data or {}
                            last_exc = None
                            for attempt in range(1, tries + 1):
                                try:
                                    # Ensure file pointer is at start for each attempt
                                    try:
                                        fobj.seek(0)
                                    except Exception:
                                        pass
                                    files_payload = {files_field_name: (f"chunk_{idx}.wav", fobj, "audio/wav")}
                                    logger.debug(f"🔍 POST attempt {attempt}/{tries} to {url} (chunk {idx})")
                                    # add a per-request timeout so a single attempt doesn't block for extremely long
                                    resp = await client.post(url, files=files_payload, data=data, timeout=300.0)
                                    resp.raise_for_status()
                                    logger.info(f"✅ POST attempt {attempt}/{tries} to {url} succeeded (chunk {idx}) status={resp.status_code}")
                                    try:
                                        json_resp = resp.json()
                                        logger.debug(f"🔍 Response JSON from {url} (chunk {idx}): {json_resp}")
                                    except Exception:
                                        logger.debug(f"🔍 Response text from {url} (chunk {idx}): {resp.text}")
                                        # fallback to empty dict when response is not JSON parsable
                                        json_resp = {}
                                    return json_resp
                                except Exception as e:
                                    last_exc = e
                                    logger.warning(f"⚠️ POST attempt {attempt} to {url} failed for chunk {idx}: {e}")
                                    # backoff (simple)
                                    await asyncio.sleep(1 * attempt)
                            # All attempts failed
                            raise last_exc

                        with open(chunk_filename, "rb") as f:
                            # include job metadata so server can serialize per-job work
                            # use 'client_job_id' to match enhanced-asr expected form parameter
                            # Disable per-chunk diarization to avoid long blocking CPU runs; run a single diarization pass after all chunks
                            data = {"client_job_id": job_id, "chunk_index": idx, "diarize": False}
                            if language:
                                data["language"] = language

                            # If the enhanced-asr service is restarting we may get connection drops; wait briefly for health
                            enh_health_url = f"{ENHANCED_ASR_URL}/health"
                            # If service is restarting, wait a little longer before sending the first chunk
                            enh_healthy = await _wait_for_health(client, enh_health_url, tries=10, delay=1.0)
                            if not enh_healthy:
                                logger.warning("⚠️ Enhanced ASR health check did not succeed before sending chunk; skipping direct post to enhanced-asr and using whisper-server fallback to avoid hangs (waited 10 attempts)")

                            # If enhanced-asr appears healthy, try it first; otherwise skip to whisper fallback
                            if enh_healthy:
                                try:
                                    logger.info(f"📤 Sending chunk {idx+1}/{total_chunks} to enhanced-asr (job={job_id} chunk={idx})")
                                    result = await _post_with_retries(f"{ENHANCED_ASR_URL}/transcribe", f, data=data, tries=3)
                                except Exception as e_enh:
                                    logger.warning(f"⚠️ Enhanced ASR failed for chunk {idx} (exception type={type(e_enh).__name__}): {repr(e_enh)}. Will try whisper-server fallback...")
                                    # fallthrough to whisper fallback below
                            # If not healthy or enhanced-asr failed, fallback to whisper-server
                            if (not enh_healthy) or (result is None):
                                    logger.info(f"🔄 Falling back to whisper-server for chunk {idx}")
                                    # Fallback to whisper-server: prefer /inference, but handle ConnectError vs HTTP 404 intelligently
                                    whisper_endpoints = [f"{WHISPER_SERVER_URL}/inference", f"{WHISPER_SERVER_URL}/transcribe"]
                                    result = None
                                    last_exc = None

                                    async def _wait_whisper(url_base: str, tries: int = 3, delay: float = 1.0) -> bool:
                                        for attempt in range(1, tries + 1):
                                            try:
                                                resp = await client.get(url_base, timeout=5.0)
                                                if resp.status_code < 500:
                                                    return True
                                            except Exception:
                                                logger.debug(f"Whisper health check attempt {attempt}/{tries} to {url_base} failed")
                                            await asyncio.sleep(delay)
                                        return False

                                    # Pre-check whisper server health once (gives faster path for transient restarts)
                                    if not await _wait_whisper(WHISPER_SERVER_URL, tries=3, delay=1.0):
                                        logger.warning("⚠️ Whisper server health check did not succeed before fallback; it may be restarting")

                                    for ep in whisper_endpoints:
                                        try:
                                            logger.info(f"🔄 Trying whisper-server fallback endpoint {ep} for chunk {idx}")
                                            result = await _post_with_retries(ep, f, data=data, tries=2)
                                            logger.info(f"✅ Fallback to whisper-server succeeded for chunk {idx} via {ep}")
                                            break

                                        except httpx.HTTPStatusError as he:
                                            # If endpoint returns 404, it simply does not exist — try the next one
                                            if he.response is not None and he.response.status_code == 404:
                                                last_exc = he
                                                logger.warning(f"⚠️ Endpoint {ep} returned 404; skipping to next endpoint")
                                                continue
                                            last_exc = he
                                            logger.warning(f"⚠️ HTTP status error for {ep}: {he}")

                                        except httpx.ConnectError as ce:
                                            # Cannot connect to whisper server — wait briefly and retry once after health check
                                            last_exc = ce
                                            logger.warning(f"⚠️ Connect error to {ep}: {ce}; will wait and retry after health check")
                                            if not await _wait_whisper(WHISPER_SERVER_URL, tries=3, delay=1.0):
                                                logger.debug("🔍 Whisper server still unhealthy after wait; moving to next endpoint")
                                                continue
                                            # Try one retry
                                            try:
                                                result = await _post_with_retries(ep, f, data=data, tries=1)
                                                logger.info(f"✅ Fallback to whisper-server succeeded for chunk {idx} via {ep} (after reconnect)")
                                                break
                                            except Exception as e_retry:
                                                last_exc = e_retry
                                                logger.warning(f"⚠️ Retry to {ep} failed: {e_retry}")

                                        except Exception as e_w:
                                            last_exc = e_w
                                            logger.warning(f"⚠️ Fallback attempt to {ep} failed for chunk {idx}: {e_w}")

                                    if result is None:
                                        logger.error(f"❌ All endpoints failed for chunk {idx}: {last_exc}")
                                        # Re-raise to mark job as failed
                                        raise last_exc
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
                    # Small pause to ensure sequential processing and avoid tight loops
                    await asyncio.sleep(0.1)
            
            # Merge and Deduplicate
            final_segments = self._merge_segments(all_segments)

            # Perform a single diarization pass on the full file (faster overall and avoids per-chunk CPU blocking)
            try:
                # Use a fresh HTTP client for the diarization request (the earlier `client` might have been closed)
                async with httpx.AsyncClient(timeout=600.0) as diar_client:
                    with open(file_path, "rb") as fdiar:
                        logger.info(f"🗣️ Requesting diarization for job {job_id}")
                        resp = await diar_client.post(
                            f"{ENHANCED_ASR_URL}/diarize",
                            files={"file": (os.path.basename(file_path), fdiar, "audio/wav")},
                            timeout=600.0,
                        )
                        resp.raise_for_status()
                        diar_json = resp.json()

                        # Some error payloads may be returned as {"error": "..."}
                        if isinstance(diar_json, dict) and diar_json.get("error"):
                            logger.warning(f"⚠️ Diarization endpoint returned error for job {job_id}: {diar_json.get('error')}")
                            turns = []
                        else:
                            turns = diar_json.get("turns", []) or []

                        # If no turns returned, try whisper-server diarization fallback (useful for short/mono inputs)
                        if not turns:
                            # Log some diagnostics about the file (channels/duration) to help debugging
                            try:
                                import wave
                                with wave.open(file_path, 'rb') as wf:
                                    channels = wf.getnchannels()
                                    duration_s = wf.getnframes() / wf.getframerate()
                                logger.info(f"ℹ️ Diarization returned no turns for job {job_id}; file channels={channels} duration_s={duration_s:.2f}")
                            except Exception:
                                logger.debug("🔍 Could not read file properties for diarization diagnostics")

                            # Attempt whisper-server fallback if configured
                            whisper_ep = f"{WHISPER_SERVER_URL}/inference"
                            try:
                                logger.info(f"🔄 Attempting whisper-server diarization fallback for job {job_id} via {whisper_ep}")
                                with open(file_path, 'rb') as fwh:
                                    # whisper-server expects form with file and optional diarize flag
                                    wresp = await diar_client.post(whisper_ep, files={"file": (os.path.basename(file_path), fwh, "audio/wav")}, data={"diarize": "true"}, timeout=600.0)
                                wresp.raise_for_status()
                                wjson = wresp.json()
                                # Extract turns from whisper response if present
                                turns = wjson.get("turns", []) or []
                                # Some servers may embed speaker labels directly in segments
                                if not turns and isinstance(wjson, dict):
                                    segments = wjson.get("segments", []) or []
                                    # build turns from segments with speaker labels
                                    speakers = {}
                                    for s in segments:
                                        spk = s.get("speaker")
                                        if spk is None:
                                            continue
                                        # use segment mids to create turns if absent
                                        speakers.setdefault(spk, []).append({"start": s.get("start"), "end": s.get("end")})
                                    # flatten into turns list with speaker names
                                    turns = []
                                    for spk, ranges in speakers.items():
                                        for r in ranges:
                                            turns.append({"start": r["start"], "end": r["end"], "speaker": spk})

                                if turns:
                                    logger.info(f"✅ Whisper-server fallback returned {len(turns)} turns for job {job_id}")
                                else:
                                    logger.info(f"ℹ️ Whisper-server fallback did not return diarization turns for job {job_id}")

                            except Exception as e_wf:
                                logger.warning(f"⚠️ Whisper-server diarization fallback failed for job {job_id}: {e_wf}")

                        if turns:
                            def assign_speaker(seg_start, seg_end, turns):
                                best_spk = None
                                best_overlap = 0.0
                                for t in turns:
                                    s = max(seg_start, t["start"])
                                    e = min(seg_end, t["end"])
                                    overlap = max(0.0, e - s)
                                    if overlap > best_overlap:
                                        best_overlap = overlap
                                        best_spk = t["speaker"]
                                # If no overlap found, try nearest-turn fallback
                                if best_spk is None and turns:
                                    # pick turn with nearest center distance
                                    mid = (seg_start + seg_end) / 2.0
                                    best_spk = min(turns, key=lambda t: abs((t["start"]+t["end"]) / 2.0 - mid))["speaker"]
                                return best_spk

                            for seg in final_segments:
                                seg_start = seg["start"]
                                seg_end = seg["end"]
                                seg["speaker"] = assign_speaker(seg_start, seg_end, turns)
                            logger.info(f"✅ Diarization applied for job {job_id}")
                        else:
                            logger.info(f"ℹ️ No diarization turns returned for job {job_id}; speakers will remain unset.")
            except Exception as e:
                logger.warning(f"⚠️ Diarization failed for job {job_id}: {e}")

            self.jobs[job_id]["status"] = "completed"
            self.jobs[job_id]["result"] = {"segments": final_segments}
            logger.info(f"🎉 Job {job_id} completed with {len(final_segments)} segments")
            
        except Exception as e:
            logger.error(f"❌ Job {job_id} failed: {e}", exc_info=True)
            self.jobs[job_id]["status"] = "failed"
            self.jobs[job_id]["error"] = str(e)
        finally:
            # Cleanup the uploaded file
            if os.path.exists(file_path):
                try:
                    os.remove(file_path)
                    logger.info(f"🗑️ Deleted cached file: {file_path}")
                except Exception as e:
                    logger.warning(f"⚠️ Failed to delete cached file {file_path}: {e}")

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
