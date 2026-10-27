"""One engine per process; no application settings or engines are modified."""
import argparse
import dataclasses
import importlib.metadata
import json
import os
from pathlib import Path
import resource
import subprocess
import threading
import time


def serializable(value):
    if dataclasses.is_dataclass(value):
        return dataclasses.asdict(value)
    if hasattr(value, "tolist"):
        return value.tolist()
    if hasattr(value, "__dict__"):
        return vars(value)
    return str(value)


class GPUMonitor:
    """Total device VRAM includes other services; sample every 250 ms."""
    def __init__(self):
        self.stop = threading.Event()
        self.samples = []
        self.thread = threading.Thread(target=self.sample, daemon=True)

    def sample(self):
        while not self.stop.is_set():
            try:
                values = subprocess.check_output([
                    "nvidia-smi", "--query-gpu=memory.used", "--format=csv,noheader,nounits"
                ], text=True, timeout=3)
                self.samples.append([int(x) for x in values.splitlines()])
            except (OSError, ValueError, subprocess.SubprocessError):
                pass
            self.stop.wait(0.25)


def load_engine(name):
    import torch
    if name == "whisper":
        from faster_whisper import WhisperModel
        return WhisperModel("large-v3", device="cuda", compute_type="float16")
    if name == "qwen":
        from qwen_asr import Qwen3ASRModel
        return Qwen3ASRModel.from_pretrained(
            "Qwen/Qwen3-ASR-1.7B", dtype=torch.bfloat16, device_map="cuda:0",
            max_inference_batch_size=1, max_new_tokens=4096,
            forced_aligner="Qwen/Qwen3-ForcedAligner-0.6B",
            forced_aligner_kwargs={"dtype": torch.bfloat16, "device_map": "cuda:0"},
        )
    if name in ("canary", "parakeet", "nemotron-diarization"):
        from nemo.collections.asr.models import ASRModel, SortformerEncLabelModel
        if name == "nemotron-diarization":
            model = SortformerEncLabelModel.from_pretrained("nvidia/Nemotron-3-Diarization").cuda().eval()
            model.sortformer_modules.chunk_len = 340
            model.sortformer_modules.chunk_right_context = 40
            model.sortformer_modules.fifo_len = 40
            model.sortformer_modules.spkcache_update_period = 300
            model._check_streaming_parameters()
            return model
        model_id = "nvidia/canary-1b-v2" if name == "canary" else "nvidia/parakeet-tdt-0.6b-v3"
        return ASRModel.from_pretrained(model_id).cuda().eval()
    from pyannote.audio import Pipeline
    model_id = "pyannote/speaker-diarization-3.1" if name == "pyannote31" else "pyannote/speaker-diarization-community-1"
    kwargs = {"use_auth_token" if name == "pyannote31" else "token": os.environ.get("HF_TOKEN")}
    return Pipeline.from_pretrained(model_id, **kwargs).to(torch.device("cuda"))


def infer(engine, name, audio):
    if name == "whisper":
        segments, _ = engine.transcribe(str(audio), language="fr", word_timestamps=True, vad_filter=True)
        rows = [dataclasses.asdict(segment) for segment in segments]
        return {"text": " ".join(row["text"].strip() for row in rows), "segments": rows}
    if name == "qwen":
        result = engine.transcribe(audio=str(audio), language="French", return_time_stamps=True)[0]
        return {"text": result.text, "timestamps": result.time_stamps}
    if name in ("canary", "parakeet"):
        kwargs = {"source_lang": "fr", "target_lang": "fr"} if name == "canary" else {}
        result = engine.transcribe([str(audio)], batch_size=1, timestamps=True, **kwargs)[0]
        return {"text": result.text, "timestamps": result.timestamp}
    if name == "nemotron-diarization":
        rows = engine.diarize(audio=[str(audio)], batch_size=1)[0]
        return {"speakers": [{"start": float(s), "end": float(e), "speaker": label}
                             for s, e, label in (row.split() for row in rows)]}
    # Decode consistently without requiring a torchcodec build matching PyTorch.
    import soundfile
    import torch
    waveform, sample_rate = soundfile.read(str(audio), dtype="float32", always_2d=True)
    output = engine({"waveform": torch.from_numpy(waveform.T.copy()), "sample_rate": sample_rate})
    annotation = output if name == "pyannote31" else output.speaker_diarization
    rows = [{"start": turn.start, "end": turn.end, "speaker": label}
            for turn, _, label in annotation.itertracks(yield_label=True)]
    return {"speakers": rows}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("engine", choices=["whisper", "qwen", "canary", "parakeet", "pyannote31", "community", "nemotron-diarization"])
    parser.add_argument("--input", type=Path, default=Path("/data"))
    parser.add_argument("--output", type=Path, default=Path("/results"))
    args = parser.parse_args()
    files = sorted(args.input.glob("*.wav"))
    if not files:
        parser.error("no WAV files found in the input directory")
    import soundfile
    import torch
    monitor = GPUMonitor()
    monitor.thread.start()
    started = time.monotonic()
    engine = load_engine(args.engine)
    torch.cuda.synchronize()
    load_seconds = time.monotonic() - started
    args.output.mkdir(parents=True, exist_ok=True)
    # Preserve the exact resolved environment for replay; no token is recorded.
    Path(args.output / f"{args.engine}-packages.txt").write_text(Path("/runtime-packages.txt").read_text())
    for audio in files:
        torch.cuda.reset_peak_memory_stats()
        start = time.monotonic()
        result = infer(engine, args.engine, audio)
        torch.cuda.synchronize()
        seconds = time.monotonic() - start
        info = soundfile.info(str(audio))
        result.update({
            "audio": audio.name, "engine": args.engine, "audio_seconds": info.duration,
            "inference_seconds": seconds, "model_load_seconds": load_seconds,
            "real_time_factor": seconds / info.duration,
            "torch_peak_allocated_bytes": torch.cuda.max_memory_allocated(),
            "gpu_total_peak_mib": max((max(x) for x in monitor.samples), default=None),
            "process_peak_rss_kib": resource.getrusage(resource.RUSAGE_SELF).ru_maxrss,
            "torch_version": torch.__version__, "cuda_version": torch.version.cuda,
        })
        destination = args.output / f"{audio.stem}-{args.engine}.json"
        destination.write_text(json.dumps(result, ensure_ascii=False, indent=2, default=serializable))
        if "speakers" in result:
            rttm = [f"SPEAKER {audio.stem} 1 {r['start']:.3f} {r['end']-r['start']:.3f} <NA> <NA> {r['speaker']} <NA> <NA>" for r in result["speakers"]]
            destination.with_suffix(".rttm").write_text("\n".join(rttm) + "\n")
        print(f"engine={args.engine} | audio={audio.name!r} | seconds={seconds:.2f} | output={str(destination)!r}", flush=True)
    monitor.stop.set()
    monitor.thread.join(timeout=5)


if __name__ == "__main__":
    main()
