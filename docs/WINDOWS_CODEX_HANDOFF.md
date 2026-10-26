# Windows desktop build and GPU benchmark handoff

Copy the prompt below into a new Codex session opened at the root of the cloned
repository on the Windows computer. It is intentionally written in English for
this public repository. Codex may reply in French.

The clone must contain these changes first. Local, uncommitted files from another
computer are not transferred by cloning. Do not substitute an older upstream
checkout when `DESKTOP_RELIABILITY.md`, `verification/`, or `benchmarks/asr/` is
missing. No commit, push, Windows build, or GPU benchmark is implied by this guide.

---

You are continuing work on Meetily, a Next.js 14 / React / Rust / Tauri 2 desktop
application. This computer runs Windows x64 with an NVIDIA RTX 4090 and 64 GB RAM.
Audit the checkout, rebuild and launch the native Windows desktop application,
validate its reliability changes, and run the supplied GPU comparison harness.
Fix reproducible compatibility defects within this scope, verify each fix, and
deliver the application artifacts and an evidence-based benchmark report.

## Working rules

- Read applicable `AGENTS.md` files and follow the audit, implementation,
  verification, backlog, and retrospective workflow. Inspect Graphify only if
  available. Preserve pre-existing changes and unrelated containers.
- Reply in French. Write every repository document, report, and code comment in
  English. This is a public repository: never put private addresses, personal
  names, account paths, credentials, meeting content, or infrastructure details
  into tracked files. Use environment variables and neutral placeholders.
- Do not stage, commit, push, deploy, reset databases, remove Docker volumes,
  prune Docker, stop unrelated services, or delete existing user recordings.
  Updating global instructions or persistent memory requires separate approval.
- Run Node/Python package installation and automated tests inside containers.
  Native Windows compilation and launching Tauri are required. Install native
  operating-system build prerequisites as needed, explaining elevated actions.
- Use PowerShell syntax for the Windows terminal. Prefer argument arrays and
  `--mount` for Docker paths. Shell scripts passed to Linux containers are POSIX
  shell, not PowerShell. Check `$LASTEXITCODE` after native commands; PowerShell's
  `$ErrorActionPreference = 'Stop'` alone does not catch their failures.
- Work through all independent steps. If GPU access, gated-model permissions,
  reference audio, or interactive desktop access is unavailable, report that
  specific limitation and finish the remaining work. Do not invent results.

## Read and establish the baseline

Read `DESKTOP_RELIABILITY.md`, `verification/README.md`,
`benchmarks/asr/README.md`, `AGENT_BACKLOG.md`, `frontend/package.json`,
`frontend/pnpm-lock.yaml`, `frontend/src-tauri/Cargo.toml`,
`frontend/src-tauri/tauri.conf.json`, `frontend/src-tauri/build.rs`, and
`backend/docker-compose.yml` before running setup commands.

The desktop summary pipeline is Rust in `frontend/src-tauri/src/summary/`.
The legacy Python summary HTTP pipeline does not have the same reliability
changes. Do not use it to validate the desktop changes.

Already implemented:

- Archiving locks against recording/conversion/retranscription, uploads the final
  audio, verifies remote bytes by size/SHA-256, persists confirmation, then
  removes application-managed audio/checkpoints without following symlinks.
  Failed cleanup remains resumable. The explicit manual cleanup button inventories
  archived meetings and deletes local audio without calling a webhook or decoder.
  It preserves documents and does not claim remote verification. Restoration of
  historical audio without an original digest still requires a complete FFmpeg
  decode. External import sources, transcripts, summaries, and metadata remain.
- Copying produces plain-text speaker/timestamp lines for the complete selected
  transcript version, including renamed speakers and times beyond one hour.
  Invalid formats and denied clipboard access produce explicit errors.
- Summary profiles persist per server/provider/model. Initial defaults are
  32,768 context tokens, **30,000 characters per chunk**, 4,096 output tokens,
  and at most 1,000 overlap characters. Budgeting includes instructions,
  template, custom context, and output; chunks shrink further when necessary.
  Existing saved profiles retain their values and must be edited explicitly.
- Chunk analysis, recursive consolidation, checkpoints, progress, and retries
  are implemented. A failed chunk stops the summary after three total attempts.
  Ollama uses `/api/chat`, top-level `think: false`, and
  `options.num_ctx` / `options.num_predict`. Other providers do not receive these
  Ollama-specific parameters. A 100,000-character input typically needs about
  four overlapping chunks, depending on boundaries and prompt size.
- The esaxx-rs Cargo patch pins upstream commit
  `3c8ac57d245ab328f7c71953b7c116a8d1d5498f`. Preserve it unless a verified
  platform defect requires a change.

Previous verification: 38 Rust core tests passed with one optional real-Ollama
test ignored by default; five transcript-format tests, two WER tests, TypeScript,
the Next.js build, and a complete Linux `cargo check --lib --locked` passed.
Browser tests used simulated IPC. A synthetic real-Ollama smoke was also run.
None of this proves Windows capture, clipboard, audio conversion, filesystem
behavior, packaging, or ASR candidate performance on this GPU.

Start in the repository root:

```powershell
$ErrorActionPreference = 'Stop'
$RepoPath = (Get-Location).Path
git status --short
git log -1 --format='%h %s'
Get-Command docker, node, rustup, cargo, cmake, ffmpeg -ErrorAction SilentlyContinue
docker version
docker ps --format 'table {{.Names}}\t{{.Status}}\t{{.Ports}}'
wsl --status
nvidia-smi
docker run --rm --gpus all nvidia/cuda:12.8.1-base-ubuntu24.04 nvidia-smi
if ($LASTEXITCODE -ne 0) { throw 'Docker GPU access is not ready.' }
```

Avoid dumping environment variables, Compose-expanded secrets, authenticated
URLs, or full diagnostic logs into the conversation or public reports.

## Native Windows build prerequisites

Use native Windows PowerShell, preferably Developer PowerShell for Visual Studio
2022, for the desktop build. WSL/Linux cannot validate a native Windows Tauri
application merely by running `cargo build` there.

Verify Node.js 22 x64, stable Rust with `x86_64-pc-windows-msvc`, Visual Studio 2022
Build Tools with Desktop development with C++, the Windows 10/11 SDK, CMake,
WebView2 Runtime, and FFmpeg in the launched application's `PATH`. Install only
missing prerequisites through official installers or verified winget packages.
LLVM/libclang may be needed by native Rust dependencies; verify any required
`LIBCLANG_PATH` instead of guessing. Reopen Developer PowerShell after installing
build tools. Record actual toolchain versions privately.

For benchmark containers, use Docker Desktop's Linux-container engine with WSL2
GPU integration and a compatible Windows NVIDIA driver. Do not install Linux
GPU kernel drivers into WSL. A working `nvidia-smi` on Windows alone is
insufficient: the Docker GPU check above must also pass.

First build the CPU desktop variant. The remote GPU ASR/LLM services remain usable
from that application. Native local Whisper CUDA additionally requires the
Windows CUDA Toolkit, `nvcc`, and a compiler supported by that toolkit; Docker
CUDA libraries do not provide them to the Windows compiler. Then build the CUDA
variant if those prerequisites are present. The existing GPU auto-detection
wrapper can select CUDA just because `nvidia-smi` exists, so bypass it for the
baseline build.

### Install Windows Node dependencies inside Docker

The repository uses `pnpm-lock.yaml`, not `package-lock.json`. Preserve the
lockfile; do not generate an npm lock or install Linux dependencies and assume
that they work on Windows.

Use a fresh, ignored directory `verification/local/windows-frontend` as a native
build workspace. Copy the frontend sources there while excluding `node_modules`,
`.next`, `out`, `target`, `.git`, logs, `.env*`, and local credentials. Do not
overwrite an existing workspace without inspecting it. Package manifests and
the pnpm lockfile must be unchanged in this copy.

Set `$WindowsFrontendPath` to that workspace's absolute path. In a Linux Node
container, install the Windows x64 optional binaries with pnpm's platform
overrides and a hoisted layout. This avoids cross-OS pnpm symlinks. The following
installation method was checked in an isolated Linux container: the frozen
lockfile resolved successfully and included Windows Tauri and Next.js binaries.
Execution of those binaries still requires Windows validation.

```powershell
$WindowsFrontendPath = Join-Path $RepoPath 'verification\local\windows-frontend'
$InstallWindowsDeps = @'
npm install --global --no-audit --no-fund pnpm@10.15.1 &&
pnpm install --frozen-lockfile --ignore-scripts --node-linker=hoisted --os=win32 --cpu=x64
'@
docker run --rm --cpus 4 --memory 8g `
  --mount "type=bind,source=$WindowsFrontendPath,target=/work" `
  --workdir /work node:22-bookworm-slim sh -c $InstallWindowsDeps
if ($LASTEXITCODE -ne 0) { throw 'Windows dependency installation failed.' }
```

Install scripts are deliberately disabled because these dependencies target
Windows. If a dependency actually requires one, identify the exact package and
solve it in an appropriate container rather than enabling arbitrary Windows
binaries in Linux. Do not run another host package install as a shortcut.

On Windows, invoke the installed JavaScript entry points directly. This avoids
depending on `.cmd` wrappers produced by a Linux package manager:

```powershell
Push-Location $WindowsFrontendPath
try {
    node -p "require.resolve('@tauri-apps/cli-win32-x64-msvc')"
    node -p "require.resolve('@next/swc-win32-x64-msvc')"
    node .\node_modules\@tauri-apps\cli\tauri.js --version
    if ($LASTEXITCODE -ne 0) { throw 'The native Windows Tauri CLI failed.' }
} finally {
    Pop-Location
}
```

### Build, package, and launch

Use an ignored Tauri override inside the copied frontend's `src-tauri` directory,
for example `tauri.validation.conf.json`. Create it with the normal patch tool:

```json
{
  "productName": "Meetily Validation",
  "identifier": "com.meetily.validation",
  "build": {
    "beforeDevCommand": "node node_modules/next/dist/bin/next dev -p 3118",
    "beforeBuildCommand": "node node_modules/next/dist/bin/next build"
  },
  "bundle": {
    "resources": ["templates/*.json", "templates/fr/*.json"]
  }
}
```

The original hooks require host pnpm; these direct Node hooks avoid that
requirement. The original resource list also bundles `.env`: exclude it for this
build so secrets are never embedded in an installer. Inspect the merged config
and unpacked resources to confirm this. Keep real endpoints/tokens in process
environment variables or a private runtime configuration, not build resources.
Do not weaken the existing CSP or network policy to anonymize documentation.

The validation identifier isolates the SQLite/app-data location, but recording
preferences can still default to a shared Music/Documents folder. On first
launch, explicitly select a new synthetic-test recording directory before any
recording, archival, or cleanup tests. Confirm the actual paths rather than
assuming everything is isolated by the application identifier.

From Developer PowerShell, with the native workspace current directory:

```powershell
Push-Location $WindowsFrontendPath
try {
    node .\node_modules\@tauri-apps\cli\tauri.js build `
      --config .\src-tauri\tauri.validation.conf.json --bundles nsis
    if ($LASTEXITCODE -ne 0) { throw 'The native Windows desktop build failed.' }
    Get-ChildItem .\src-tauri\target -Recurse -File -Filter '*.exe' |
      Where-Object { $_.FullName -match '\\release\\(bundle\\nsis\\|meetily\.exe$)' } |
      Select-Object Name, Length
} finally {
    Pop-Location
}
```

Use `build --features cuda --config ... --bundles nsis` for the optional native
CUDA build. `--features cuda` is a Tauri CLI option; do not blindly copy legacy
scripts that pass it after an extra `--`. Use `dev --config ...` for development.
Record which artifact uses which features and calculate its SHA-256 with
`Get-FileHash`. A fresh application Cargo lockfile may need to be resolved because
the repository ignores it; preserve the verification crate's tracked lockfile.
Do not arbitrarily upgrade native dependencies to hide a build failure.

Launch the built executable first and verify it. Verify the NSIS installer with
the validation identifier if interactive access permits; do not replace an
existing application installation without preserving its configuration/data.
Inspect the installed resources for secrets. This build is for local validation,
not a signed public release. Record signing status and any installer limitations.

## Service configuration and summary smoke

Inspect existing services before deciding that any backend needs rebuilding.
The Rust desktop reliability changes alone do not require restarting every
backend service. Existing Ollama and ASR services may already be available.
Read Compose `env_file`, profiles, and port mappings before targeted operations;
`enhanced-asr` loads the root private `.env`, and services use the `default`
profile. Check logs only for the service actually changed.

Ask for missing private endpoints through local input, without putting their
values into repository files or reports. Relevant variables include
`MEETILY_BACKEND_URL`, `MEETILY_OLLAMA_URL`, `MEETILY_WHISPER_URL`,
`MEETILY_ARCHIVE_WEBHOOK_URL`, `MEETILY_RESTORE_WEBHOOK_URL`,
`MEETILY_N8N_X_API_KEY`, `MEETILY_FFMPEG_PATH`, and `MEETILY_LANGUAGE`.
Enhanced ASR is a separate service; inspect the application's settings and
integration route instead of confusing its endpoint with whisper.cpp.

Packaged applications load private configuration independently of their launch
folder. Create `%APPDATA%\Meetily\.env` on Windows, or
`~/Library/Application Support/Meetily/.env` on macOS, before launching the app.
The fallback `~/.meetily.env` is also supported. Never copy private configuration
into the app bundle. Restart the application after changing this file. Existing
process environment variables take precedence, followed by the user configuration
and then legacy bundled files. Working-directory `.env` discovery is limited to
debug builds. The normal bundle configuration excludes `.env` resources.

For Windows, create and open the runtime file without printing its contents:

```powershell
$RuntimeConfigDir = Join-Path $env:APPDATA 'Meetily'
New-Item -ItemType Directory -Force $RuntimeConfigDir | Out-Null
$RuntimeConfigFile = Join-Path $RuntimeConfigDir '.env'
if (-not (Test-Path $RuntimeConfigFile)) {
    New-Item -ItemType File $RuntimeConfigFile | Out-Null
}
notepad $RuntimeConfigFile
```

For a same-computer Ollama, check `/api/tags` at `http://localhost:11434`.
A container reaches a host Ollama via `http://host.docker.internal:11434` only
when that listener is reachable; do not expose it publicly or disable firewalls.
Preserve current service settings. If the selected model is absent, download it
through the existing Ollama installation and report download/disk requirements.

Configure `qwen3.5:9b` in the desktop with the explicit **32768 / 30000 / 4096**
profile. Verify actual requests and `/api/ps` when available; theoretical model
metadata is not evidence that Ollama loaded the requested context. Test a short
meeting, more than 100,000 characters, late decision corrections, boundary facts,
custom instructions, error reporting, interruption/resume, and checkpoint
invalidation after instruction changes. Use synthetic data for initial tests.

Thirty thousand characters is the current conservative default, not a proven
quality optimum. Compare 30k and 50k on identical reviewed inputs if practical,
measuring omissions, inventions, owners, deadlines, runtime, and peak VRAM.
Do not equate successful chunk coverage with factual correctness.

## Repeat automated verification in containers

Translate the exact commands in `verification/README.md` to PowerShell. Build
`meetily-verification:core` using `verification/Dockerfile`, mount the repo
read-only at `/src`, and copy the required sources into the isolated work volume.
Preserve `--locked` for the verification crate. Run its test binary as `nobody`
so deletion-denied tests do not accidentally pass as root. Use single-quoted
PowerShell here-strings for shell code containing `$test_binary`.

In a separate Linux Node container, install from the same frozen pnpm lockfile
and run:

```text
node --test tests/transcript-format.test.cjs
node node_modules/typescript/bin/tsc --noEmit
node node_modules/next/dist/bin/next build
```

Keep this Linux dependency tree separate from the Windows build workspace.
Run `python -m unittest discover -s /bench -p test_score.py` in a Python container
with `benchmarks/asr` mounted read-only at `/bench`. Repeat targeted checks after
fixes; do not count previous results as tests run on this checkout.

The real-Ollama test is opt-in:
`integration::real_ollama_long_summary_smoke -- --ignored --exact --nocapture`.
Pass `MEETILY_SMOKE_OLLAMA_URL` and `MEETILY_LANGUAGE=fr` through the Docker
environment. It uses synthetic inputs and must not consume real meeting audio.

Use Playwright MCP for browser flows and capture console/network failures and a
synthetic-data screenshot under `verification/local/`. If unavailable, explicitly
state that and give manual steps. Browser IPC simulation cannot validate Tauri
clipboard, native capture, or filesystem operations.

## Native Windows acceptance checks

Run these in the rebuilt app using only test copies and the isolated recording
directory. Check both normal completion and explicit errors:

1. Microphone capture, Windows system audio when supported, stop/save/playback;
   document any existing platform limitation rather than inventing support.
2. Import, conversion, retranscription, cancellation, retry, and crash recovery;
   external sources remain intact and operation WAVs/checkpoints are cleaned.
3. Archive a synthetic meeting only after the remote restore webhook returns the
   same binary `audio.mp4` bytes. A JSON/HTML success or redirect is insufficient.
   Verify no managed audio remains, while transcripts/summaries/metadata remain;
   restore and play the verified file.
4. With a dedicated test webhook, simulate unavailable/incomplete remote data
   and denied local deletion. Originals must survive unverifiable uploads;
   confirmed uploads with failed cleanup must resume safely. Never disrupt the
   user's real webhook service just to inject an error.
5. Inventory historical archives before manual cleanup. Limit experiments to
   synthetic legacy fixtures. Confirm that manual cleanup deletes managed audio
   with no webhook or decoder calls, preserves transcripts and summaries, and
   refuses active recording/conversion/retranscription. Retry denied deletion.
   Historical restoration still decodes remote audio when no digest exists;
   include a MP4 with `moov` after `mdat`. Decode verification buffers at most
   512 MiB in RAM. Confirm its loopback reader stops after completion or failure.
   Do not bulk-clean existing archives as part of this benchmark session.
6. Rename a synthetic speaker, copy raw/enhanced versions, apply a visual filter,
   and confirm copying still includes the complete selected version. Paste into
   Notion if available or a plain editor, checking speaker names and `[01:12:34]`
   timestamps. Test invalid JSON and denied clipboard access.
7. Verify the summary scenarios and profile persistence above through native IPC.
   No partial summary may be marked complete after a chunk fails.

## GPU ASR and diarization comparison

Preserve the application's configured engines until results justify a switch.
Run the adapters provided in `benchmarks/asr/`; they have not been GPU-validated
on this computer. If upstream APIs fail, record exact versions and errors, make
the smallest verified adapter fix, and rerun. Do not silently substitute models.

Prepare identical mono 16 kHz WAV extracts named neutrally: clear conversation,
technical terminology/names, noise, interruptions/overlap, French-English
switching, and a complete multi-hour recording. Store authorized audio in
`benchmarks/asr/data`, complete corrected `.txt` references in `references`, and
speaker references in RTTM with matching stems. Do not invent ground truth.
If references are absent, report speed/memory only and request annotations before
claiming WER or DER. These directories and `results` are ignored by Git.

The 4090 has 24 GB VRAM; 64 GB system RAM does not replace GPU memory. Measure
idle GPU use first, run candidates sequentially, and distinguish cold downloads
from warmed inference. Keep other services untouched. Coordinate any necessary
Ollama model unloading rather than stopping services unilaterally.

Accept gated model terms for pyannote 3.1, segmentation 3.0, and Community-1 in
the Hugging Face account before those runs. Obtain a read-only token privately.
The token is passed at runtime, never as a Docker build argument or in an image.

Run in PowerShell from the repository root:

```powershell
$BenchPath = Join-Path $RepoPath 'benchmarks\asr'
Push-Location $BenchPath
try {
    New-Item -ItemType Directory -Force data, references, results | Out-Null
    $BenchDataPath = (Resolve-Path .\data).Path
    $BenchReferencesPath = (Resolve-Path .\references).Path
    $BenchResultsPath = (Resolve-Path .\results).Path
    if (-not (Get-ChildItem .\data -Filter '*.wav' -File)) {
        throw 'Add authorized WAV inputs before running inference.'
    }
    foreach ($Engine in @('whisper', 'qwen', 'canary', 'pyannote31', 'community', 'nemotron-diarization')) {
        docker build --build-arg "ENGINE=$Engine" --tag "meetily-bench:$Engine" .
        if ($LASTEXITCODE -ne 0) { throw "Image build failed: $Engine" }
        $RunArgs = @(
            'run', '--rm', '--gpus', 'all', '--shm-size=2g',
            '-e', 'HF_TOKEN', '-e', 'HF_HUB_DISABLE_TELEMETRY=1',
            '--mount', "type=bind,source=$BenchDataPath,target=/data,readonly",
            '--mount', "type=bind,source=$BenchResultsPath,target=/results",
            '--mount', "type=volume,source=meetily-bench-$Engine-cache,target=/root/.cache",
            "meetily-bench:$Engine", $Engine
        )
        docker @RunArgs
        if ($LASTEXITCODE -ne 0) { throw "Inference failed: $Engine" }
    }
} finally {
    Pop-Location
    Remove-Item Env:HF_TOKEN -ErrorAction SilentlyContinue
}
```

Before that block, set `$env:HF_TOKEN` from private local input if needed. For
PowerShell 7, `Read-Host -MaskInput` avoids displaying it. Do not put a real token
in a command literal. Start with `@('whisper', 'qwen')` if a staged comparison is
more practical, then add Canary and the diarizers. Preserve results in separate
ignored run directories for repeats; the harness overwrites same-named results.

For WER scoring, while the path variables above still exist:

```powershell
docker run --rm --entrypoint python `
  --mount "type=bind,source=$BenchResultsPath,target=/results,readonly" `
  --mount "type=bind,source=$BenchReferencesPath,target=/references,readonly" `
  meetily-bench:qwen /bench/score.py /results /references
if ($LASTEXITCODE -ne 0) { throw 'WER scoring failed.' }

docker build --build-arg ENGINE=scoring --tag meetily-bench:scoring $BenchPath
if ($LASTEXITCODE -ne 0) { throw 'The DER scoring image failed to build.' }
docker run --rm --entrypoint python `
  --mount "type=bind,source=$BenchResultsPath,target=/results,readonly" `
  --mount "type=bind,source=$BenchReferencesPath,target=/references,readonly" `
  meetily-bench:scoring /bench/score_diarization.py `
  /results/clear-community.rttm /references/clear.rttm
if ($LASTEXITCODE -ne 0) { throw 'DER scoring failed.' }
```

Replace the example RTTM stems with files actually present; score every
diarization candidate on the same references. The supplied scorer uses collars
of 0 and 0.25 seconds with overlap included and optimal speaker-label matching.
ASR-only WER does not evaluate speaker attribution: assess alignment of words to
speaker turns separately. Manually assess proper names, numbers, negations,
timestamp errors, language switching, and complete long-recording coverage.

Record loading time, warmed inference time, real-time factor, process RSS,
PyTorch allocation, and total GPU memory. The harness's process RSS and sampled
total GPU peaks are cumulative across files in one run and GPU totals include
other services: use one input per isolated run if a per-file peak is needed.
Whisper uses CTranslate2, so its memory is not represented by PyTorch allocation.
Report OOMs, truncation, diarization drift, and failed adapters as failures.
Record image identities, package snapshots, model revisions when obtainable,
driver/toolchain versions, and GPU sharing conditions for reproducibility.

The current reference candidates are Whisper large-v3, Qwen3-ASR 1.7B with
ForcedAligner 0.6B, Canary 1B v2, pyannote 3.1, Community-1, and Nemotron 3
Diarization. Parakeet is the existing live-transcription reference. Nemotron 3.5
ASR live integration is not implemented; server NeMo results do not establish
native ONNX/GGUF latency or accuracy in the application's capture pipeline.
The original Mac live comparison requires that platform separately.

## Deliverables and completion criteria

Continue until the native app builds and launches and the available acceptance
checks/benchmarks finish, or identify a concrete external prerequisite that
prevents a particular step. Do not stop after a plan or image download.

Deliver:

1. Windows executable and NSIS artifact locations with hashes, feature flags,
   signing status, startup instructions, required runtime dependencies, and
   confirmed private configuration location. Test the packaged installation
   separately from development mode when possible.
2. Actual command outcomes, targeted tests, native acceptance results, and
   synthetic screenshots. Clearly distinguish passed, failed, and untested.
3. A comparison table for accuracy, speaker attribution, timestamps, time,
   memory, and multi-hour stability, with a recommendation supported by these
   recordings. Do not choose a winner solely from published benchmark claims.
4. A small, sanitized source diff for any verified Windows/adapter fixes. Keep
   raw outputs/audio/references/tokens and identifying logs in ignored folders.
   Write private findings under `verification/local/`, not public documentation.
5. An updated backlog containing only remaining work. Existing items include
   native-platform validation, meeting content in logs, summary factuality, Yjs
   duplicate imports, Rust/Python pipeline divergence, and GPU comparison.
   Remove completed items only when their stated scope has actually been tested.

Before handoff, run `git diff --check`, inspect changed/untracked candidate files
for personal information and secrets, and verify no packaged `.env` or token is
included. Do not treat a clean Git diff as evidence that installer resources are
safe. Do not commit or push; provide a concrete reviewable result.
