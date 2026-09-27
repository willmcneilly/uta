# Project: Foundations

**Status:** Created · **RFC:** [RFC-001](../rfcs/rfc-001-foundations.md) · **Notion:** https://app.notion.com/p/Foundations-3e83af969b6f81e2896cd9c54b5cf4df

## Goal

Build milestone 0 from RFC-001: a Uta window where you press Play and hear a clean tone, change the volume and undo it, watch a level meter, and switch or unplug your output without Uta breaking. Underneath, every layer the rest of Uta builds on exists, and the automatic checks guard it on every PR.

## Tickets

### 1. [UTA-1] Set up the repository, agent guide and CI (Chore)

**Goal:** The repository has its real structure: Rust crates, an empty Tauri + React app, a `CLAUDE.md` for agents, and GitHub checks running on every PR. The leftover scaffold is replaced.

**Acceptance criteria**
- [ ] Cargo workspace with `crates/uta-core`, `crates/uta-engine` and `crates/uta-cli`, each building with a placeholder test.
- [ ] `app/` contains a Tauri 2 app (React + TypeScript, Vite, npm). `npm run tauri dev` opens an empty window titled "Uta".
- [ ] `CLAUDE.md` (under about 200 lines) with the layout, build and test commands, the audio-thread rules from RFC-001, and the ticket/branch/PR conventions.
- [ ] GitHub Actions on `macos-15`, on PRs only, cancelling superseded runs, with Rust caching. Checks:
  - `cargo fmt --check`
  - `cargo clippy -- -D warnings`
  - `cargo test`
  - frontend typecheck, lint and test
  - `tauri build`
- [ ] This ticket's own PR shows the checks green.

**Out of scope:** Any audio or UI behaviour. The RTSan job arrives with the audio code in ticket 2.

**Depends on:** Nothing.

**Context:** RFC-001, "The stack" and "Repository and checks".

### 2. [UTA-2] Engine core: tone, play/stop and volume, proven offline (Feature)

**Goal:** The audio engine exists and is proven correct without any sound hardware. `uta render out.wav` writes a few seconds of the tone. The real-time rules are enforced by tools, not by trust.

**Acceptance criteria**
- [ ] The engine is split into a control side and an audio-thread processor, connected by fixed-size `rtrb` queues:
  - commands in;
  - status out (position, peak level, dropout count);
  - used snapshots returned to the control side for cleanup.
- [ ] Snapshot swap works: a new "what to play" snapshot is swapped in at the start of a block, and the old one is freed off the audio thread.
- [ ] The processor produces a 440 Hz sine with a volume control. Play and Stop fade in and out briefly, and volume changes are smoothed, so none of them click.
- [ ] An offline render driver runs the same processor with no device. `uta render <file.wav> [--seconds N]` writes a WAV.
- [ ] Sound tests pass:
  - measured pitch is 440 Hz within tolerance;
  - level is within tolerance;
  - no sample-to-sample jump above a set limit across Play, Stop and volume changes;
  - blocks of 32, 128 and 1024 give the same audio within tolerance;
  - the golden WAV matches (regenerate with `UTA_GOLDEN=1`).
- [ ] Real-time safety:
  - the process path is marked `#[nonblocking]` (`rtsan-standalone`);
  - tests run it under `assert_no_alloc` (nih-plug's fork), including a snapshot swap;
  - a CI job runs the tests with RTSan enabled. If RTSan can't run on GitHub's Mac machines, document why and open a follow-up ticket; `assert_no_alloc` stays gating.
- [ ] A timing test reports p50 and p99 block time against the deadline. It is not gating.

**Out of scope:** Playing through a real device (ticket 4) and the project/undo model (ticket 3). The engine takes simple commands directly here.

**Depends on:** 1.

**Context:** RFC-001, "The audio thread follows strict rules", "Our own small engine" and "How agents prove the audio code is correct".

### 3. [UTA-3] Project core: commands, undo and replay (Feature)

**Goal:** Rust owns the project. Every change is a saveable, versioned command that can be undone, and the tests prove that replaying commands always rebuilds the same project. This is the "never lose a project" foundation.

**Acceptance criteria**
- [ ] `uta-core` has a `Project` (for now: master volume, with a stable ID) and a `Command` enum that is serde-serialisable and carries a format version.
- [ ] Applying a command returns its inverse. Undo and redo work. Each applied command gets a monotonic sequence number.
- [ ] The core turns the project into the engine's snapshot type, so the engine gets its "what to play" from the core.
- [ ] Tests pass:
  - apply then undo gives the exact previous state;
  - property test: a random series of commands, replayed, always gives the same project;
  - commands round-trip through serialisation.

**Out of scope:** Saving projects to disk, the command journal and autosave. The design must allow them, but they come later.

**Depends on:** 1. It can be worked on alongside ticket 2; the snapshot conversion lands on whichever merges second.

**Context:** RFC-001, "The project core owns the project".

### 4. [UTA-4] Live playback that survives device changes (Feature)

**Goal:** The engine plays through your Mac's current output and carries on when you switch or unplug outputs. `uta play` in the terminal lets you try it before the window exists.

**Acceptance criteria**
- [ ] A cpal stream plays on the system default output at the device's native sample rate. The buffer size defaults to 128 and can be set to 64 or 32 when the device supports it.
- [ ] The cpal error callback only pushes an error code into a queue, and a test covers it under the real-time checks.
- [ ] A supervisor thread handles device changes:
  - rebuilds on `DeviceChanged` when the rate or buffer changes;
  - rebuilds on `StreamInvalidated`;
  - pauses on `DeviceNotAvailable` and polls for a device every 1–2 s;
  - keeps the playback position, and fades in after a rebuild.
- [ ] The supervisor's logic is tested with a fake stream: device changed, device gone, device back.
- [ ] `uta play [--buffer N]` plays the tone until Ctrl-C. It prints the device name, buffer size and a running dropout count.
- [ ] Verification notes record a manual run of switching outputs, unplugging and replugging.

**Out of scope:** The UI, choosing a specific non-default device, and audio input.

**Depends on:** 2.

**Context:** RFC-001, "Sound devices".

### 5. [UTA-5] The Uta window: play, stop, volume with undo, and a meter (Feature)

**Goal:** Milestone 0 as you'll use it: the window, wired to the Rust core and engine.

**Acceptance criteria**
- [ ] The window shows:
  - the current output device and buffer size, with a picker for 32, 64 and 128 limited to what the device supports;
  - Play and Stop;
  - a volume control;
  - a level meter drawn on canvas;
  - a dropout count.
- [ ] UI actions go to Rust as Tauri commands through the project core. The UI holds no project state of its own.
- [ ] Meter, position and dropout data stream through a Tauri channel, batched once per frame.
- [ ] Undo and Redo sit in the native Edit menu (⌘Z / ⇧⌘Z) and are handled in Rust.
- [ ] UI tests run against Tauri's mocked back end in CI.
- [ ] Verification notes cover RFC-001's manual checks 1–4, run on Will's machine.

**Out of scope:** Any timeline or session view, and saving.

**Depends on:** 3, 4.

**Context:** RFC-001, "The UI draws on canvas and receives streamed updates" and "The milestone 0 deliverable".

### 6. [UTA-6] Pin the Rust toolchain (Chore)

**Goal:** Local builds and CI use exactly the same Rust version, so a check that passes locally passes in CI. Upgrading Rust becomes a deliberate change in its own PR.

**Acceptance criteria**
- [ ] `rust-toolchain.toml` at the repository root pins a specific stable version with the `rustfmt` and `clippy` components.
- [ ] Every CI job that builds Rust uses the pinned version, and its log shows it.
- [ ] `cargo --version` in the repository picks up the pinned version locally.
- [ ] `CLAUDE.md` says where the toolchain is pinned and how to upgrade it.
- [ ] All checks pass on the pinned version, locally and in CI.

**Out of scope:** The Node version, other tools, and adopting new Rust features.

**Depends on:** Nothing. Added after ticket 2's PR, where CI's newer Rust failed a lint that passed locally.

**Context:** RFC-001, "Repository and checks".

## Order and checkpoints

```
1 ──► 2 ──► 4 ──┐
  └──► 3 ───────┴──► 5
```

- Ticket 1 goes first. Then 2 and 3 can run in parallel, 4 follows 2, and 5 needs both 3 and 4.
- Ticket 6 depends on nothing and can land at any point.
- **Checkpoint after 1:** run `npm run tauri dev` and see an empty Uta window. PRs show green checks.
- **Checkpoint after 2:** run `uta render tone.wav` and listen to the file.
- **Checkpoint after 4:** run `uta play`, then switch outputs, unplug and replug your interface while it plays.
- **Checkpoint after 5:** RFC-001's manual checks in the app. That's milestone 0 done.

## Coverage

| RFC-001 verification item | Ticket |
|---|---|
| Tone correctness (pitch, level) | 2 |
| No clicks on Play, Stop, volume | 2 |
| Block-size independence | 2 |
| Golden file | 2 |
| Real-time safety: snapshot swap | 2 |
| Real-time safety: simulated device error | 4 |
| Commands and undo: undo, replay, serialisation | 3 |
| App builds | 1 |
| UI tests against mocked back end | 5 |
| Timing report (not gating) | 2 |
| Manual 1: rapid Play/Stop | 5 |
| Manual 2: undo/redo volume | 5 |
| Manual 3: switch, unplug, replug outputs | 4 (terminal), 5 (app) |
| Manual 4: 64 buffer, 10 minutes, dropout count | 4, 5 |
| Manual 5: `uta render` | 2 |

## Not in this project

- **Saving, the command journal, autosave and crash recovery.** RFC-001 only requires that the design allows them. They need their own RFC once there's something worth saving.
- **Timeline performance.** Will chose to find out when we build the timeline.
- **Choosing a specific output device, and audio input and latency measurement.** These come with recording.
- **Other platforms.**

## Open questions

1. **Resolved: add `uta play`.** It's part of ticket 4.
2. **Resolved: first commit straight to `main`.** It holds the docs, the `.claude` skills and `.gitignore`; everything after goes through tickets.
3. **Resolved: commit the skills.** The email has been removed from the Notion skill's login check.
