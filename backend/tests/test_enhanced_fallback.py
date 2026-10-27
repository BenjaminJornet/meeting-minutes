import tempfile
import unittest
from pathlib import Path
from unittest.mock import AsyncMock, patch

import httpx

import enhanced


class EnhancedFallbackTests(unittest.IsolatedAsyncioTestCase):
    async def test_later_failed_chunk_does_not_reuse_previous_primary_result(self):
        with tempfile.TemporaryDirectory() as directory:
            audio = Path(directory) / "synthetic.wav"
            enhanced.AudioSegment.silent(duration=5000).export(audio, format="wav")
            primary_calls = 0
            fallback_calls = 0

            def respond(request):
                nonlocal primary_calls, fallback_calls
                if request.method == "GET":
                    return httpx.Response(200, json={})
                if request.url.path == "/diarize":
                    return httpx.Response(200, json={"turns": [{"start": 0, "end": 5, "speaker": "Speaker A"}]})
                if request.url.host == "primary.test":
                    primary_calls += 1
                    if primary_calls == 1:
                        return httpx.Response(200, json={"segments": [{"start": 0, "end": 1, "text": "First primary chunk"}]})
                    return httpx.Response(500)
                fallback_calls += 1
                return httpx.Response(200, json={"segments": [{"start": 0, "end": 1, "text": "Later fallback chunk"}]})

            client_class = httpx.AsyncClient
            with patch.object(enhanced, "ENHANCED_ASR_URL", "http://primary.test"), \
                 patch.object(enhanced, "WHISPER_SERVER_URL", "http://fallback.test"), \
                 patch.object(enhanced, "_JOBS_STATE_FILE", str(Path(directory) / "jobs.json")), \
                 patch.dict(enhanced.os.environ, {"CHUNK_LENGTH_MS": "4000"}), \
                 patch.object(enhanced.httpx, "AsyncClient", side_effect=lambda **kwargs: client_class(transport=httpx.MockTransport(respond))), \
                 patch.object(enhanced.asyncio, "sleep", new=AsyncMock()):
                manager = enhanced.EnhancedTranscriptionManager()
                manager.jobs["synthetic"] = {"status": "processing", "progress": 0}
                await manager._process_job("synthetic", str(audio), "fr")
            self.assertEqual(primary_calls, 4)
            self.assertEqual(fallback_calls, 1)
            self.assertEqual(manager.jobs["synthetic"]["status"], "completed")
            segments = manager.jobs["synthetic"]["result"]["segments"]
            self.assertEqual([segment["text"] for segment in segments], ["First primary chunk", "Later fallback chunk"])
            self.assertEqual(segments[1]["start"], 2)
            self.assertFalse(audio.exists())

    async def test_failed_primary_uses_fallback_and_preserves_failure_when_both_fail(self):
        for fallback_status in (200, 500):
            with self.subTest(fallback_status=fallback_status), tempfile.TemporaryDirectory() as directory:
                audio = Path(directory) / "synthetic.wav"
                enhanced.AudioSegment.silent(duration=200).export(audio, format="wav")
                requests = []

                def respond(request):
                    requests.append((request.method, request.url.host, request.url.path))
                    if request.method == "GET":
                        return httpx.Response(200, json={})
                    if request.url.path == "/diarize":
                        return httpx.Response(200, json={"turns": [{"start": 0, "end": 1, "speaker": "Speaker A"}]})
                    if request.url.host == "primary.test":
                        return httpx.Response(500, json={"error": "synthetic primary failure"})
                    return httpx.Response(fallback_status, json={"segments": [{"start": 0, "end": 0.2, "text": "Synthetic fallback"}]})

                client_class = httpx.AsyncClient
                transport = httpx.MockTransport(respond)
                with patch.object(enhanced, "ENHANCED_ASR_URL", "http://primary.test"), \
                     patch.object(enhanced, "WHISPER_SERVER_URL", "http://fallback.test"), \
                     patch.object(enhanced, "_JOBS_STATE_FILE", str(Path(directory) / "jobs.json")), \
                     patch.object(enhanced.httpx, "AsyncClient", side_effect=lambda **kwargs: client_class(transport=transport)), \
                     patch.object(enhanced.asyncio, "sleep", new=AsyncMock()):
                    manager = enhanced.EnhancedTranscriptionManager()
                    manager.jobs["synthetic"] = {"status": "processing", "progress": 0}
                    await manager._process_job("synthetic", str(audio), "fr")
                self.assertEqual(sum(method == "POST" and host == "primary.test" and route == "/transcribe"
                                     for method, host, route in requests), 3)
                self.assertTrue(any(host == "fallback.test" and route == "/inference" for _, host, route in requests))
                self.assertFalse(audio.exists())
                if fallback_status == 200:
                    self.assertEqual(manager.jobs["synthetic"]["status"], "completed")
                    self.assertEqual(manager.jobs["synthetic"]["result"]["segments"][0]["text"], "Synthetic fallback")
                else:
                    self.assertEqual(manager.jobs["synthetic"]["status"], "failed")
                    self.assertNotIn("result", manager.jobs["synthetic"])
                    self.assertNotIn("UnboundLocalError", manager.jobs["synthetic"]["error"])

    async def test_whisper_json_error_is_not_treated_as_success(self):
        with tempfile.TemporaryDirectory() as directory:
            audio = Path(directory) / "synthetic.wav"
            enhanced.AudioSegment.silent(duration=200).export(audio, format="wav")
            requests = []

            def respond(request):
                requests.append((request.url.host, request.url.path))
                if request.method == "GET":
                    return httpx.Response(200, json={})
                if request.url.path == "/diarize":
                    return httpx.Response(200, json={"turns": []})
                if request.url.host == "primary.test":
                    return httpx.Response(500, json={"error": "primary"})
                return httpx.Response(200, json={"error": "failed to read WAV file"})

            client_class = httpx.AsyncClient
            with patch.object(enhanced, "ENHANCED_ASR_URL", "http://primary.test"), \
                 patch.object(enhanced, "WHISPER_SERVER_URL", "http://fallback.test"), \
                 patch.object(enhanced, "_JOBS_STATE_FILE", str(Path(directory) / "jobs.json")), \
                 patch.object(enhanced.httpx, "AsyncClient", side_effect=lambda **kwargs: client_class(transport=httpx.MockTransport(respond))), \
                 patch.object(enhanced.asyncio, "sleep", new=AsyncMock()):
                manager = enhanced.EnhancedTranscriptionManager()
                manager.jobs["synthetic"] = {"status": "processing", "progress": 0}
                await manager._process_job("synthetic", str(audio), "fr")
            self.assertEqual(manager.jobs["synthetic"]["status"], "failed")
            self.assertIn("Whisper server returned an error", manager.jobs["synthetic"]["error"])
            self.assertNotIn("result", manager.jobs["synthetic"])
            self.assertFalse(audio.exists())
