# RFC-001: Foundations: stack, engine rules and verification

**Status:** Accepted (2026-09-27) · **Author:** Will · **Notion:** https://app.notion.com/p/Foundations-stack-engine-rules-and-verification-3e83af969b6f81298c56d9de5f6029ba

## Summary

This RFC sets the foundations every later part of Uta builds on: the tech stack, how the pieces talk to each other, the rules the audio code must follow, where the project data lives, and how agents prove their work is correct. The proposal is a Rust audio engine with a web-based UI in a Tauri app, built and tested on macOS only for now, without closing off other platforms. Rust owns the project and every change is a "command", which makes undo, autosave and crash recovery straightforward later. The first deliverable is deliberately tiny: a Uta window where you press Play and hear a clean tone, with a volume control you can undo. That tiny slice exercises every layer and the checks that guard it.

## Motivation

Everything after this depends on these decisions, and they're the most expensive ones to change later. Changing the UI technology after building an arrangement view means rewriting it, and adding real-time safety rules after the engine is built means auditing all of it. Recording and autosave also depend on how project changes flow through the app.

It also sets up the way of working. You won't read the audio code, so the automated checks are how you (and reviewing agents) know it works. They need to exist before the first feature, not after.

## Proposal

### The stack

- **Audio engine: Rust.** Rust gives C++-level speed without C++'s memory bugs, and its compiler catches whole classes of mistakes before the code runs. That's valuable when agents write most of it. Audio input and output goes through **cpal** (pinned to 0.18.x), the standard Rust library for talking to sound hardware. It's actively maintained, and its latest release (0.18.0, Jun 2026) added proper handling of devices changing and disconnecting.
- **App and UI: Tauri 2 with a web front end.** Tauri wraps a web UI in a small native Mac app and connects it to Rust. It uses the Mac's built-in web view, not a bundled browser, so the app stays small. Pin Tauri 2.x. Tauri 2.12.0 is current, and 3.0 is in alpha.
- **Platforms: macOS only for building and testing, portable by default.** cpal and Tauri both support Windows and Linux, so nothing in the core ties us to the Mac. We only add Mac-specific code when there's a reason, and we don't test other platforms. The minimum is macOS 14.2, set by cpal.

### How the pieces fit together

```
  Web UI (Tauri window)
     │  commands ("set volume to -6 dB", "play")          ▲ state updates
     ▼                                                      │ meters/playhead ~60×/sec
  Project core (Rust) ── owns the project, applies commands, keeps undo history
     │  finished snapshot of "what to play"                ▲ used snapshots returned
     ▼                                                      │ for cleanup
  Audio thread (Rust) ── makes the sound, nothing else
```

**1. The project core owns the project.** The UI never changes project data directly. It sends a **command**, a small description of one change, such as "set track 1 volume to -6 dB". The core applies it, records how to reverse it (that's undo), and tells the UI the new state. Commands are:
- **Saveable.** They can be written to disk and read back.
- **Versioned.** Old saved commands still load after the format changes.
- **Tied to permanent IDs.** They refer to things by IDs that never change, so replaying them always hits the right thing.

This is the foundation for "never lose a project". Once saving exists, the app can write each command to a log on disk as it happens and replay the log after a crash. We don't build that log now. We just make sure nothing would stop us, and we test it by checking that replaying a random list of commands always produces the same project. Undo uses the `undo` crate or a thin wrapper around the same pattern. Saving will use "write to a temporary file, flush it to disk, then swap it in", so a crash mid-save can't corrupt a project.

**2. The audio thread follows strict rules.** macOS asks the audio thread for the next few milliseconds of sound hundreds of times a second. If it's ever late, you hear a click or dropout. So the audio thread must never:
- **wait for anything**, such as another thread holding data it needs (a "lock");
- **ask the system for memory** (allocate) or give memory back, because both can stall unpredictably;
- **read or write files, or talk to the network;**
- **do work whose length depends on something unbounded**, such as looping over a list that can grow.

Instead:
- **Messages go through fixed-size queues** in both directions. We use `rtrb`, a lock-free "ring buffer": a fixed-size mailbox that one thread writes to and another reads from without either waiting.
- **Changes to what's playing are made off the audio thread.** The core builds a complete new snapshot, and the audio thread swaps it in at the start of its next block. The old snapshot is sent back to the core to be thrown away, so the audio thread never frees memory. Firewheel and FunDSP, two mature Rust audio engines, work the same way.
- **The code that reports device errors must follow the same rules**, because cpal can call it from the audio thread. There's a known bug in cpal 0.18 where its own error reporting allocates there, so Uta's error handler only drops a code into a queue and returns.

**3. The UI draws on "canvas" and receives streamed updates.**
- Commands go from the UI to Rust as ordinary Tauri calls.
- Fast-changing data, such as meters and the playhead, comes back through a Tauri **channel**. The Tauri docs recommend channels over "events" for fast, ordered updates. The data is batched into one message per screen frame.
- Large binary data, such as waveforms, is sent as raw bytes rather than text.
- Busy views (meters now, the timeline later) are drawn on an HTML canvas, like a game, rather than built from thousands of page elements.
- The UI is built with **React and TypeScript**. Busy views use the browser's built-in canvas drawing. Whether we need a graphics library on top (three.js or a 2D one) is decided when we build the timeline, based on what it actually needs.

### Our own small engine, borrowing proven patterns

We'll write our own engine rather than adopt an existing one:
- **Firewheel**, the most complete Rust audio engine, explicitly says it isn't built for DAWs, because keeping every sound in step with the song position conflicts with its game-focused design.
- **FunDSP** (a library of oscillators, filters and effects) is a good source of building blocks when we get to instruments and effects.
- For milestone 0, the engine is just a sine oscillator, a volume control and play/stop.

### Sound devices

- **Follows your Mac's output.** Uta plays to whatever output your Mac is set to use. When that changes (switching in System Settings, plugging in or unplugging your interface), a background "supervisor" rebuilds the audio connection and carries on from the same playback position, with a very short fade-in so the switch doesn't click.
- **When the device disappears.** If no output is available, Uta pauses and checks every second or two for one to return, because cpal can't yet announce new devices.
- **Buffer size.** This is how much sound is prepared in each block: smaller means lower latency but more risk of glitches. It defaults to 128 samples, and you can pick 64 or 32 if your device supports them. At 48 kHz, 128 samples is about 2.7 ms.
- **Sample rate.** Uta uses the device's own rate, so there's no conversion.

### How agents prove the audio code is correct

- **Offline render.** The engine can produce audio straight into a WAV file without a sound device, using exactly the same code that plays live. This is how every audio test works, and it's the `uta render` command.
- **Real-time safety checks.**
  - **RealtimeSanitizer (RTSan)** is an LLVM tool that stops the program if the audio code allocates, locks or calls the system. We use it via the `rtsan-standalone` crate, which works on macOS with stable Rust.
  - **`assert_no_alloc`** runs in tests as a simpler backstop. We use the fork nih-plug maintains, because the original hasn't had a release since 2021.
  - Both only catch problems in code paths the tests actually run, so the tests drive the real audio path, including device errors.
- **Sound tests**, in the style of Surge and FunDSP:
  - a tone at a frequency is that frequency, measured from its waveform;
  - levels are within tolerance;
  - Play and Stop produce no clicks, which a test checks by measuring sudden jumps;
  - rendering in small blocks and big blocks gives the same result.
- **Saved reference files ("golden" WAVs).** Tests compare new renders with saved reference renders within a small tolerance (about 0.00001). Exact bit-for-bit matches aren't reliable across machines. A human approves any change to a reference file.
- **Timing test.** This measures how long each block takes to compute against its deadline. It reports the numbers but doesn't fail on them yet, because the shared CI machines' timings are too noisy.

### Repository and checks

- **Layout.** One repository (`willmcneilly/uta`) holding:
  - `crates/uta-core`: project and commands
  - `crates/uta-engine`: audio
  - `crates/uta-cli`: offline render
  - `app/`: the Tauri app and web UI

  Meadowlark's author named splitting code across many repositories as one of its problems.
- **Instructions for agents.** A short `CLAUDE.md` (under about 200 lines) with the commands, the layout and these rules.
- **Existing scaffold.** The uncommitted leftover code from before this process gets replaced, not kept.
- **Checks on every PR (GitHub Actions):**
  - formatting
  - lint warnings treated as errors
  - all tests
  - an RTSan test run
  - a full app build

  The Mac job runs on `macos-15` to match your machine. GitHub's `macos-latest` has moved to macOS 26, which behaves differently in the web view. CI never relies on a sound device: the Mac test machines don't have a reliable one, so tests use offline render.

### The milestone 0 deliverable

- A Uta window showing the current output device and buffer size.
- **Play** and **Stop** buttons: Play starts a 440 Hz tone and Stop ends it, with no clicks.
- A **volume** control and a live **level meter**.
- **Undo and Redo** (⌘Z / ⇧⌘Z) for volume changes, handled in Rust.
- `uta render out.wav` in the terminal renders a few seconds of the same tone.
- All the checks above running on every PR.

The tone is throwaway. The layers under it aren't.

## What it looks like to you

You open Uta and see a plain window showing which output it's using. You press Play and hear a steady tone straight away. Stop ends it cleanly. Pressing Play and Stop quickly twenty times never clicks. You drag the volume down and the meter follows, and ⌘Z brings it back.

Play something, then:
- **switch outputs in System Settings:** the tone moves to the new output within a moment;
- **unplug your interface mid-tone:** Uta falls back to the Mac's speakers or pauses. It never crashes or goes permanently silent;
- **plug the interface back in:** Uta picks it up again.

Every PR shows green checks on GitHub. When a check fails, it says which rule was broken.

## Alternatives considered

- **A native Swift UI (SwiftUI/AppKit) on the Rust engine.** Only this option would get the full native Mac feel for free, and the Ghostty terminal and the Element X chat app show the structure works.
  - **Why not:** it locks the UI to the Mac, so any other platform later means a second UI. It's also a language and toolkit you don't know, and agents do measurably worse on Swift-specific code.
  - With "performant and responsive" as the bar rather than "native", web meets it for less. If the web UI can't meet the bar, we'd need platform-specific rendering anyway, so it's the fallback.
- **A Rust-native UI library (egui, iced, Slint, vizia, gpui).** These libraries are immature and their APIs change often. Meadowlark tried several and concluded they badly underestimate what a DAW's UI needs. Its author called the UI his biggest underestimate and moved the front end to Flutter. Agents also have little training data for these libraries.
- **C++ and JUCE for everything.** JUCE is the industry standard, and it's free for personal use. **Why not:** it throws away the Rust engine plan, C++ gives agents weaker safety nets, and JUCE's UI doesn't feel like a Mac app either.
- **Electron instead of Tauri.** Electron bundles Chrome instead of using the Mac's web view. That avoids the Mac web view's quirks, at the cost of a much bigger app. We haven't researched it in depth. It's the first fallback if the Mac's web view turns out to be the problem.
- **An existing Rust audio engine (Firewheel).** It says it isn't designed for DAWs (see Proposal). We borrow its patterns instead.
- **Driving CoreAudio directly instead of cpal.** That means redoing cpal's device-change work ourselves, and it would tie the engine to the Mac.

## Risks & unknowns

- **Web view performance is the biggest unknown.**
  - On your Mac (macOS 15), the web view is capped at 60 frames per second even on a 120 Hz screen. A community plugin says macOS 26 lifts the cap, but I couldn't confirm that from Apple.
  - One credible report describes graphics being smooth in Safari but jittery in a Tauri window, with no cause found.
  - There are no proper benchmarks.
  - **Mitigation:** we're deliberately not running a performance test up front. We find out when we build the first real timeline view; if it feels sluggish, that's the signal. Electron and native are the fallbacks.
- **Drag and drop.** With Tauri's file-drop feature on (needed to drag samples in from Finder with their file paths), the page's own drag and drop doesn't work on macOS. The plan is to move clips with our own pointer handling, which DAWs usually do anyway. It doesn't affect milestone 0.
- **Real-time safety tools on macOS CI.** `rtsan-standalone` claims macOS support, but I haven't confirmed its download works on GitHub's Mac machines. If it doesn't, we fall back to `assert_no_alloc` plus the Linux job and fix it as a follow-up.
- **cpal gaps.** cpal allocates when it reports errors on the audio thread (covered in the Proposal). It also has no "device plugged in" event, so Uta polls. Both are known, and there's an open issue for each.
- **Buffer size is shared (guess).** On macOS, setting the buffer size may apply to the whole device, so it could affect other apps using the same interface. We'll check this during implementation.
- **Agent quality on the real-time code is unproven.** The checks exist to catch it, but this is the part of the experiment we're testing.
- **CI cost.** The repository is public, so GitHub's standard Mac and Linux machines are free.

## How we'll verify it

**Automated, on every PR:**
- **Tone correctness:** a rendered 440 Hz tone measures 440 Hz, at the expected level, within tolerance.
- **No clicks:** Play, Stop and volume changes produce no sample-to-sample jump above a set limit.
- **Block-size independence:** rendering in blocks of 32, 128 and 1024 gives the same audio within tolerance.
- **Golden file:** the milestone 0 render matches the saved reference WAV.
- **Real-time safety:** the audio path runs under RTSan and `assert_no_alloc`, including a simulated device error and a snapshot swap. Any allocation, lock or system call fails the build.
- **Commands and undo:**
  - Applying a command and then undoing it returns the exact previous state.
  - Replaying a random series of commands always produces the same project.
  - Commands survive being saved and loaded.
- **App:** the Tauri app builds, and the UI's own tests pass against a fake Rust back end. Tauri provides this "mock" for tests.

**Reported but not gating:** block processing time against the deadline, at the 50th and 99th percentile.

**Manual, by you:**
1. Play and stop repeatedly, quickly. There should be no clicks.
2. Change the volume and undo or redo with ⌘Z and ⇧⌘Z.
3. While it's playing, switch outputs in System Settings, then unplug and replug your interface.
4. Set the buffer to 64, let it play for 10 minutes on your interface, and listen for glitches. The app shows a count of any dropouts.
5. Run `uta render` and open the WAV.

## Open questions

1. **Resolved: no up-front web view performance test.** Will's call: we accept the risk and find out when we build the timeline (see Risks).
2. **Resolved: React with TypeScript.** No canvas library yet. Canvas2D for milestone 0; revisit a graphics library for the timeline.
3. **Resolved: public repository.** GitHub's standard runners are free for public repositories.
4. **Resolved: keep CI.** Will's call on 2026-09-27. With a public repository the standard Mac machines are free. CI gives an independent check on a clean machine, and it's where the real-time safety run happens. It runs only on PRs and cancels superseded runs.
