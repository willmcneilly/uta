# Uta

Uta is a DAW: a Rust audio engine with a React UI in a Tauri 2 app. Built and tested on macOS only (minimum 14.2), but nothing in the core should tie it to the Mac. The foundations are in [RFC-001](docs/rfcs/rfc-001-foundations.md); read it before touching the engine or the project core.

## Layout

```
Cargo.toml            workspace: crates/* and app/src-tauri
crates/uta-core       the project, commands and undo (no audio, no UI)
crates/uta-engine     the audio engine: control side + audio-thread processor
crates/uta-cli        the `uta` binary: offline render, terminal playback
app/                  Tauri app: React + TypeScript, Vite, npm
app/src-tauri         the app's Rust side (crate `uta-app`)
docs/rfcs             RFCs (Notion is the source of truth once accepted)
docs/plans            project plans: tickets, order, checkpoints
examples              command lists for `uta render/play --commands`
.github/workflows     CI
```

How the pieces talk:
- The UI never changes project data. It sends **commands** to Rust as Tauri calls. The project core applies them, records the inverse for undo, and returns the new state.
- Each change reaches the UI once. A command's reply carries it, and nothing else does. A change the UI didn't ask for (Undo and Redo from the menu bar) emits `project-changed` with **no payload**, and the UI fetches the project with `get_project`.
- **No large data in an event.** Rust pushes an event by having the web view run JavaScript: fine for a small message, slow for a big one. Send an ID or nothing, and let the UI fetch the rest with an ordinary call.
- Commands are serde-serialisable, versioned, and refer to things by permanent IDs, so replaying them always rebuilds the same project.
- The engine builds a complete "what to play" snapshot from the core's project (`Snapshot::from(&Project)`). The audio thread swaps it in at the start of a block and sends the old one back to be freed.
- Fast data (meters, playhead, dropouts) streams to the UI through a Tauri channel, batched once per frame. Busy views draw on a canvas.

## Commands

Run everything from the repository root unless it says `app/`.

```bash
# Rust
cargo fmt --all                                     # format
cargo fmt --all --check                             # CI check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo run -p uta-cli -- render out.wav --commands examples/demo-loop.json   # the `uta` command: offline render
cargo run -p uta-cli -- play --commands examples/demo-loop.json --buffer 64 # loop it on the default output until Ctrl-C
RTSAN_ENABLE=1 cargo test -p uta-engine             # tests under RealtimeSanitizer (downloads its runtime)
cargo test -p uta-engine --release --test timing -- --ignored --nocapture   # block timing report
cargo test -p uta-core --release --test timing -- --ignored --nocapture     # copy-on-write timing report
UTA_GOLDEN=1 cargo test -p uta-engine --test sound  # regenerate golden WAVs

# Frontend (in app/)
npm ci
npm run typecheck                                   # tsc -b
npm run lint                                        # eslint
npm test                                            # vitest, jsdom
npm run tauri dev                                   # open the app with hot reload
npm run tauri build -- --ci                         # release build: target/release/bundle/macos/Uta.app
```

`cargo build`/`clippy`/`test` compile the app crate in dev mode, so they don't need `app/dist`. Only `tauri build` needs the frontend built, and it does that itself.

**Before opening a PR, run all of the above checks and fix everything they report.** CI runs the same set.

## The audio thread rules

These aren't negotiable. The audio thread is called hundreds of times a second, and anything slow or unpredictable is an audible click. Code that runs on it (the process path, and the cpal error callback, which cpal can call from the audio thread) must never:

- **wait** for anything: no `Mutex`, `RwLock`, channels that block, `thread::sleep`, or anything that locks internally;
- **allocate or free memory**: no `Vec::push` past capacity, `Box::new`, `String`, `format!`, `clone()` of heap data, dropping a `Box`/`Vec`/`Arc` whose count hits zero, or `println!`/logging;
- **do I/O**: no files, network or other system calls;
- **do unbounded work**: every loop is bounded by the block size or a fixed capacity.

Instead:
- Messages go through fixed-size lock-free queues (`rtrb`) in both directions: commands in; status (position, peak, dropout count) out; used snapshots back to the control side.
- Changes to what's playing are made off the audio thread as whole new snapshots, swapped in at a block boundary. The old snapshot goes back through a queue; the audio thread never frees it.
- State the audio thread keeps outside the snapshot (voices, the bookmark, the playhead) holds only plain values, such as note IDs, pitches and positions, never its own reference (`Arc`, `Box`, borrowed slice) to snapshot data. Snapshots share data through `Arc`s, and whoever drops the last reference frees it, so the audio thread must never hold one of its own.
- The cpal error callback only pushes an error code into a queue and returns. cpal 0.18's own error reporting allocates there; don't build on it.
- Preallocate everything at stream setup, sized for the largest block.

How it's enforced (from ticket UTA-2 on):
- The process path is marked `#[nonblocking]` (`rtsan-standalone`), and a CI job runs the tests under RealtimeSanitizer.
- Tests drive the real process path under `assert_no_alloc` (nih-plug's fork), including snapshot swaps and simulated device errors.
- Both only catch what the tests exercise, so tests must drive the real audio path, not a copy.

## Proving audio code works

Nobody listens to every change, so the tests are the evidence:
- Every audio test uses **offline render**: the same processor, driven without a device, written to a buffer or WAV (`uta render`). CI never uses a sound device.
- **Sound tests**: measure pitch from the waveform, check levels within tolerance, check there's no sample-to-sample jump above a limit across Play/Stop/volume changes, and check that block sizes 32, 128 and 1024 give the same audio.
- **Golden WAVs**: compared within about 1e-5, not bit for bit. Regenerate with `UTA_GOLDEN=1`. A human approves every change to a golden file; say so in the PR.
- **Timing**: block time against the deadline at p50/p99 is reported, not gated.
- Things you can't check (hearing, unplugging an interface) go in the PR as steps for Will, with what they should hear or see. Never claim you verified something you didn't.

## Code conventions

- Rust 2024 edition. `cargo fmt` defaults. Clippy warnings are errors in CI.
- The Rust version is pinned in `rust-toolchain.toml`, and CI installs the same one; rustup fetches it on first use. To upgrade, bump the version in its own PR and fix any new lints there, not in a feature PR.
- Tests live next to the code (`#[cfg(test)] mod tests`), integration tests in `crates/<name>/tests/`.
- Keep `uta-core` free of audio and UI dependencies (`uta-engine` depends on it, never the reverse), and `uta-engine` free of Tauri.
- Frontend: React function components, strict TypeScript, no `any`. Tests next to the component (`Foo.test.tsx`). UI tests run against Tauri's mocked back end (`@tauri-apps/api/mocks`), never a real one.
- No project state in the UI: it renders what Rust sends.
- Pin major versions: Tauri 2.x (not the 3.0 alphas), cpal 0.18.x.

## Working on tickets

Work is planned in Notion (Will's personal workspace, never Stora): RFCs → Projects → Tickets. Use the `notion` skill and its `ntn.sh` wrapper for all Notion access. The `work-ticket` skill has the full process.

- Tickets have IDs `UTA-n` and a status: Backlog · Ready · In Progress · Blocked · In Review · Done. Only start a `Ready` ticket whose dependencies are `Done` and merged.
- Branch: `uta-<n>-<short-slug>`, e.g. `uta-2-engine-core`, from an up-to-date `main`.
- PR title: `UTA-<n>: <ticket name>`. The body has what changed and why, the acceptance criteria with evidence, "For Will to try" steps, and follow-ups.
- Stay in scope. Anything else worth doing goes under follow-ups in the PR, not in the diff.
- Ideas Will has along the way that aren't about the current PR go in the scratchpad (`/note`, the `scratchpad` skill), and they're reviewed between projects.
- New concepts go in the Notion **Glossary** so Will can follow how Uta is built: an RFC lists its new terms for review, and a ticket adds any that came up during the work (see the `rfc`, `work-ticket` and `notion` skills).
- Before In Review: tick the met criteria on the ticket and fill in its Verification notes.
- Comments you post to Notion start with `🤖 Claude:`, because the CLI posts as Will.
- Never merge your own PR. Merging happens after review and Will's go-ahead.
- Don't write temp files to `/tmp`; use the session scratchpad.
