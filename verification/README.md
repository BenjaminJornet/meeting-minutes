# Desktop reliability verification

For a native Windows desktop rebuild and PowerShell benchmark commands, use the
[Windows Codex handoff](../docs/WINDOWS_CODEX_HANDOFF.md). Linux verification does
not replace the native acceptance checks in that guide.

This crate compiles production archiving, locking, LLM client, summary processing,
SQLite repository, and template sources without the native audio or GUI runtime.
Only API boundary DTOs use fixtures. Core tests do not validate Tauri macros or
native clipboard, capture, or audio conversion APIs.

## Core tests

Run from the repository root:

```bash
docker build -f verification/Dockerfile -t meetily-verification:core verification
docker run --rm --cpus 2 --memory 4g \
  -v "$PWD:/src:ro" -v meetily-verify-rust:/work \
  -v meetily-verify-cargo:/usr/local/cargo -w /work meetily-verification:core sh -c '
    mkdir -p verification frontend/src-tauri
    cp /src/verification/Cargo.toml /src/verification/Cargo.lock /src/verification/lib.rs /src/verification/integration.rs verification/
    cp -R /src/frontend/src-tauri/src /src/frontend/src-tauri/templates /src/frontend/src-tauri/migrations frontend/src-tauri/
    cargo test --manifest-path verification/Cargo.toml --locked --lib --no-run --message-format=json > core-build.json &&
    test_binary=$(sed -n '\''s/.*"executable":"\([^"]*\)".*/\1/p'\'' core-build.json | tail -1) &&
    test -n "$test_binary" &&
    runuser -u nobody -- "$test_binary" --test-threads=2
  '
```

Running as `nobody` exercises deletion denied by filesystem permissions, which
root would bypass. Webhooks and Ollama are local HTTP fixtures; core tests never
send real meeting audio. Long-transcript scenarios cover more than 100,000
characters and recursive consolidation. Simulated model responses verify the
mechanism, not semantic quality.

Copy only verification sources: `verification/local/` can contain large private
Windows build artifacts. Select the executable reported by Cargo rather than
the first matching file, which can be stale in a reused build volume.

The `default_thirty_thousand_character_budget_covers_a_long_meeting` scenario
checks that all synthetic speaker turns reach the simulated Ollama requests,
with at most 30,000 characters per source chunk. SQLite tests also verify that a
saved custom profile is not overwritten by changed defaults.

Run frontend formatting tests with `node --test tests/transcript-format.test.cjs`
inside the frontend container after installing dependencies with the lockfile.
Also run `tsc --noEmit` and `next build`; the existing Next.js build checks types.

Validation completed in containers: 38 core Rust tests, five transcript-formatting
tests, two WER scoring tests, TypeScript checking, and the Next.js production
build. The real-server test is excluded from the default suite. Browser scenarios
with a simulated IPC boundary cover transcript copying, analysis progress, chunk
errors, and saving the 32768/30000/4096 profile. Browser testing does not establish
native clipboard or filesystem correctness.

Keep screenshots, console logs, raw test reports, and local configuration outside
version control. `.playwright-mcp/`, `verification/local/`, and benchmark input and
result directories are ignored. The browser scenario reported an existing Yjs
duplicate-import warning and a missing model-list key in its fixture.

## Packaged runtime configuration regression

`runtime_env` tests verify release startup independently of the launch directory,
configuration precedence, and rejection of partially malformed files. Explicit
process variables take precedence over private user configuration; new bundles
exclude `.env`. See the Windows handoff for platform-specific runtime locations.

A native macOS cleanup smoke used an isolated duplicate of a legacy residual WAV
and the existing restoration webhook. The application streamed and decoded the
remote MP4, persisted its proof, removed the fixture audio, and preserved the
transcript hash. This validates one legacy cleanup path, not every archive or
platform. Real endpoints, keys, filenames, and recordings are kept out of the
repository.

Historical restoration and verified archive operations support MP4 indexes
located after the media data. A private
loopback HTTP endpoint serves byte ranges from a RAM buffer so FFmpeg can seek
without writing a temporary recording. The buffer is capped at 512 MiB; exceeding
that cap or failing decoding preserves every local audio file. The listener and
buffer are released on completion, failure, or cancellation. New checkpoint
merges use `+faststart` to place the MP4 index before media data.

A second native macOS smoke reproduced a legacy MP4 rejected by the pipe decoder
because its `moov` index followed `mdat`. Seekable verification decoded the same
remote bytes successfully and removed the isolated duplicate WAV. Transcript and
original WAV hashes were unchanged. Automated regressions cover the trailing
index, byte ranges, bounded memory, and preservation of invalid archives.

## Full application check

The former `esaxx-rs` fork is unavailable. Its exact commit,
`3c8ac57d245ab328f7c71953b7c116a8d1d5498f`, remains available in the upstream
`Narsil/esaxx-rs` repository. Cargo now pins that commit explicitly, preserving
the existing patch and MSVC configuration. A program calling `esaxx_rs::suffix`
was compiled and executed with this source. The local application lockfile,
ignored by the existing repository configuration, was aligned with the new URL.

The core image does not contain Tauri's GUI and audio development headers.
Build the additional Linux image after `meetily-verification:core`, then check
an isolated copy of the application sources:

```bash
docker build -f verification/Dockerfile.tauri -t meetily-verification:tauri verification
docker run --rm --cpus 2 --memory 4g \
  -v "$PWD:/src:ro" -v meetily-verify-rust:/work \
  -v meetily-verify-cargo:/usr/local/cargo -w /work meetily-verification:tauri sh -c '
    mkdir -p native
    tar -C /src/frontend/src-tauri --exclude=./target --exclude=./logs -cf - . | tar -C native -xf -
    cargo check --manifest-path native/Cargo.toml --locked --lib -j 2
  '
```

The complete `cargo check --lib --locked` passed under Linux. Three unused-import
warnings remain in `audio/devices/fallback.rs`. This check does not validate macOS
APIs or produce a verified desktop release. Build and test each target platform
before distributing an application.

## Optional real-server smoke test

An explicitly enabled test exercises the production Ollama client and summary
processor on more than 100,000 synthetic characters. It checks the corrected
budget, task owner, and two known unsupported additions. It contains no real
meeting data.

Set `MEETILY_SMOKE_OLLAMA_URL` privately to an endpoint reachable from Docker.
Pass it into an isolated test container through the environment:

```bash
export MEETILY_SMOKE_OLLAMA_URL="http://ollama.example.test:11434"
docker run --rm --cpus 2 --memory 4g \
  -e MEETILY_SMOKE_OLLAMA_URL -e MEETILY_LANGUAGE=fr \
  -v "$PWD:/src:ro" -v meetily-verify-rust:/work \
  -v meetily-verify-cargo:/usr/local/cargo -w /work meetily-verification:core sh -c '
    mkdir -p verification frontend/src-tauri
    cp /src/verification/Cargo.toml /src/verification/Cargo.lock /src/verification/lib.rs /src/verification/integration.rs verification/
    cp -R /src/frontend/src-tauri/src /src/frontend/src-tauri/templates /src/frontend/src-tauri/migrations frontend/src-tauri/
    cargo test --manifest-path verification/Cargo.toml --locked --lib \
      integration::real_ollama_long_summary_smoke -- --ignored --exact --nocapture
  '
```

The `.test` hostname is a documentation placeholder, not a running service.
Configure the actual endpoint in the shell or an ignored environment file.
The test uses `qwen3.5:9b` and the 32768/30000/4096 defaults. Synthetic French
dialogue contains a proposed budget, a later correction, and a task assigned to
an anonymous speaker. Controlled repetition exercises chunking; this is not a
general benchmark of meeting-summary quality.

A real-server run completed five chunk analyses and final rendering. It retained
the corrected budget, owner, and deadline. An initial run had added an action to
the speaker announcing a decision and invented compliance duties. Instructions
were strengthened at each stage, and the subsequent run passed checks for those
two additions. These assertions do not validate every sentence or establish that
30,000 characters produces better summaries than 50,000. Evaluate additions,
omissions, owner attribution, and deadlines on a varied reference corpus before
making quality claims.

## Explicit local-only cleanup

`cleanup_archived_audio` inventories and removes audio only from an owned archived
meeting or a pending archive cleanup. It acquires the shared audio operation lock,
records a resumable state before deletion, and preserves documents and existing
remote proofs. It does not call webhooks, require configuration, or decode audio.
Metadata records `audio_cleanup_mode: local_only` without claiming remote
verification. Tests cover document preservation, idempotence, active operations,
non-archived meetings, denied deletion, and retry.
Historical `recording` or `processing` metadata does not establish live activity:
cleanup relies on the shared operation lock and the native recording-state check.
Regression tests cover unlocked archived meetings with both stale statuses and
confirm that a genuinely held lock still prevents deletion.

## Native smoke checklist

After building on the target platform:

1. Import a test copy, retranscribe, cancel, and retry. Confirm that the external
   source remains intact and `.audio-temp` is empty.
2. Archive a test meeting. The restoration webhook must return the same binary
   `audio.mp4` bytes as the upload. JSON acknowledgments and redirects do not
   count as verification. Check the meeting folder and checkpoints for residual
   audio, then restore and play the recording.
3. Make the webhook unavailable, simulate denied deletion, and retry. Unverifiable
   archives must retain local originals. Failed cleanup must remain resumable.
4. Inventory legacy archives before manual cleanup. The explicit cleanup action
   deletes managed audio locally without contacting a webhook or running FFmpeg.
   Confirm documents remain unchanged, active operations block cleanup, and denied
   deletion can be retried. Remote proof must not be fabricated by manual cleanup.
   Restoration without an original digest still requires a full remote decode;
   its seekable RAM buffer is capped at 512 MiB. Decode failure preserves local
   files and prevents restoration from publishing an unverified file.
5. Rename a synthetic speaker, copy each transcript version, and paste into the
   destination editor. Test denied clipboard access and invalid JSON.
6. Generate a summary through a real Ollama server. Observe progress, interrupt
   the application, and retry with identical inputs to reuse saved steps. Change
   the custom instructions to verify that old steps are not reused.

Automated verification never deletes existing user archives or installs a new
desktop application over a user's installation.
