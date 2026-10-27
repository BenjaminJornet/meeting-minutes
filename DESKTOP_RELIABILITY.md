# Audio archiving and long meeting summaries

## Audio archiving

Archiving shares a filesystem lock with recording, importing, and retranscription.
It uploads `audio.mp4` through the existing webhook contract, then streams the
remote file back to compare its size and SHA-256 digest. Confirmation is written
atomically to `metadata.json` before deletion. Cleanup removes application-managed
audio files, including checkpoints and temporary files, without following symbolic
links. Transcripts, summaries, and other metadata remain available.

Incomplete cleanup leaves the archive in `cleanup_pending`. Retrying verifies the
remote file again and resumes cleanup without uploading confirmed audio again.
Legacy archives have an inventory action before cleanup. Unavailable, truncated,
or invalid remote files prevent local deletion. When an original digest is not
available, FFmpeg must also decode the entire remote stream successfully without
creating a local copy. Set `MEETILY_FFMPEG_PATH` to select the executable; otherwise
the application searches its standard locations and `PATH`.

Conversion WAV files are unique to each operation. Their owners remove them on
success, failure, or cancellation; startup recovery removes abandoned files under
the meeting lock. Files imported from outside application storage remain intact.
Restoration verifies downloaded content before publishing `audio.mp4`.

System backups and manually exported audio are outside application-managed cleanup.

For isolated validation, set `MEETILY_RECORDINGS_DIR` to an absolute directory
before starting the application. Recording, import, archive inventory, and startup
audio recovery use this directory instead of the shared media folder. An invalid
relative override stops startup rather than falling back to existing recordings.

Private runtime configuration may be supplied through process environment variables
or the existing external `.env` lookup. Installer resources contain templates only;
private `.env` files must remain outside the installation package.

## Copying a conversation

The copy action includes the entire selected transcript version, regardless of
search or speaker filters. Historical arrays and wrappers are normalized, and
renamed speakers and elapsed timestamps appear in compact plain-text lines:

```text
[01:12:34] Speaker A : The proposal is approved.
```

Elapsed time continues beyond one hour. Missing speakers use a localized unknown
speaker label; missing timestamps use `--:--:--`. Invalid input or denied clipboard
access produces an explicit error instead of copying serialized JSON.

## Long meeting summaries

The AI model settings store a profile for each server, provider, and model. Defaults
are a requested context of 32,768 tokens, a maximum of 30,000 characters per chunk,
and 4,096 output tokens. These are starting values to validate with the selected
model and deployment hardware.

The character limit applies to each chunk, not the whole transcript. A transcript
of 100,000 characters requires approximately four overlapping analyses followed by
consolidation. Existing saved profiles retain their configured values; change the
maximum chunk size in model settings to update them. Model metadata provides an
upper bound and does not establish the context actually loaded by the server.

Ollama requests use `/api/chat`, `think: false`, `options.num_ctx`, and
`options.num_predict`. Cloud providers do not receive Ollama-specific parameters.
Budgeting includes prompts, templates, custom instructions, and reserved output.
When the runtime exposes no compatible tokenizer, the fallback estimates tokens
from UTF-8 bytes with a 20% margin. Chunks shrink to fit the available budget and
overlap by at most 1,000 characters.

Each chunk is analyzed with the custom instructions. Intermediate notes retain
source references, facts, actions, owners, dates, numbers, and decision changes.
Notes are consolidated through additional levels when necessary, then rendered
with the existing Markdown template. Empty or truncated output, or a chunk that
fails after three attempts, stops generation. Missing steps are never silently
skipped.

An additive SQLite migration stores profiles, progress, and successful steps.
Retrying with the same transcript, model, instructions, and options reuses saved
steps. Changing those inputs invalidates their reuse. Summarization is compression:
complete chunk coverage does not guarantee that every detail survives or that the
model introduces no unsupported claims.

See the [verification guide](verification/README.md) and
[ASR benchmarking instructions](benchmarks/asr/README.md).
