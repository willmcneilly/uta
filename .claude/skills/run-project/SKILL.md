---
name: run-project
description: Keep a Uta project's tickets moving without Will starting each one. Use when Will says /run-project, "run the project", "keep the tickets going", "work through the Reverb tickets", or asks what the coordinator is doing. Starts each Ready ticket in its own session (two at a time), starts what each merge unblocks, notices stalled sessions, and tells Will when something is waiting on him.
effort: medium
---

# Running a project

Will's time is the project's critical path, and he's often away for days. Between his sittings, the work should keep moving: every ticket that can run is running, every PR that doesn't need him merges, and when he sits down, everything that does need him is waiting in one place (`scripts/ticket inbox`). This session is the coordinator that makes that happen. It doesn't write code, review or merge: each ticket's own session does that, with the `work-ticket` skill.

## 1. Start

1. **Which project.** Take it from Will's words, or ask. Read its plan in `docs/plans/`, especially "Order and checkpoints": which tickets can run alongside each other, which wait, and where Will's checkpoints are.
2. **Where things stand.** Run `scripts/ticket ready` and `scripts/ticket inbox`, and `list_sessions` (the `ccd_session_mgmt` one) for ticket sessions already running, so you don't start a ticket twice.
3. **Keep the Mac awake** while there's work: `request_keep_awake` (the `ccd_host` one) with `until: "session_idle"`. A closed lid still sleeps, and then nothing runs: say so to Will once.
4. **Start the heartbeat:** `CronCreate`, recurring, about every 30 minutes on an off minute (`"17,47 * * * *"`), with the prompt `run-project heartbeat`. It lasts 7 days. Note its ID to delete it at the end.
5. **Start tickets** (section 2), then tell Will in a few lines what's running, what's waiting, and what's in his inbox.

## 2. Starting tickets

**At most two tickets working at once.** This Mac is an M4 (10 cores, 24 GB). With five tickets building at once (18 Oct), `scripts/check` went from about 3 minutes to 8–10, and one session waited 10 minutes for another's check to finish. A ticket that's waiting on Will (its sessions are in the inbox, or its PR needs him) isn't working, and doesn't count. Run more only if Will asks.

For each free place, take the next startable ticket from `scripts/ticket ready`, in the plan's order (or the lowest ID), skipping any ticket already running, and start it with `start_session` (the `ccd_session` one):
- `initiation: "user_asked"` (Will asked you to run the project), `context: "fresh"`, `use_worktree: true`;
- `title`: `UTA-<n>: <ticket name>`, and `name`: `uta-<n>`;
- `prompt`: `/work-ticket UTA-<n> (coordinated)`;
- `background`: one or two sentences: that it was started by the project's coordinator, that Will may be away, so its listening sessions and handover go to `scripts/ticket inbox` rather than questions in chat, and that it merges on its own when the review says nothing needs Will.

Two tickets the plan runs alongside each other are fine together. Don't start two that the plan says change the same files at once (its "Shared numbers" notes).

## 3. When a ticket session reports back

A `<side-sessions-event>` message arrives when one ends a turn. Read its note, then check rather than trust it (`get_session` or `list_events`, and `scripts/ticket show UTA-n`):
- **Merged:** start whatever is now startable (section 2). If it was a checkpoint, note what Will can now try.
- **Waiting on Will** (listening sessions prepared, or a PR whose review says it needs him): nothing for you to do but tell Will (section 5). Its place is free for another ticket.
- **Blocked, or stopped with an error:** tell Will (section 5), with the ticket and the reason in a line.
- **Still working** (it ended a turn waiting on a background job): nothing yet. The heartbeat watches it.

Never answer a ticket session's question for Will, and never tell it to merge: it merges on its own through `scripts/ticket merge --auto`, or on Will's word. When Will gives you a go-ahead for a ticket ("merge UTA-105"), send it to that ticket's session with `SendMessage`, quoting his words.

## 4. The heartbeat

On each `run-project heartbeat`:
1. `list_sessions` with `linked: true`. For each ticket session that's idle or working, look at its last activity (`get_session`).
2. **Stalled:** no activity for over an hour, and not waiting on Will. That's a session that ended a turn waiting for a background job or a notification that never came (on 29 Oct a ticket sat like that for 9 days). Send it: "Status check from the coordinator: what are you waiting for? If a background job has finished or died, check and carry on." If it's still stalled at the next heartbeat, tell Will.
3. Start tickets if there's a free place (section 2), in case an event was missed.
4. If nothing is running, nothing is startable and nothing is waiting on Will, the project is done or stuck: section 6.

Say nothing to Will on a heartbeat where nothing changed.

## 5. Telling Will

He may be away, so a message here isn't enough. Notifications go through this one step, so changing how he's reached (Pushover, say) is a change here:
- **Send a `PushNotification`** when something new is waiting on him: a ticket's listening sessions, a PR that needs him, a Blocked ticket, a stall that a nudge didn't fix. One line, leading with what he'd do and how long it takes, from `scripts/ticket inbox`: "Uta: 3 listening questions (about 10 min) and UTA-105 needs a look. scripts/ticket inbox". At most one an hour; combine what arrives in between.
- **Don't notify** for a merge that needed nothing from him, a ticket starting, or a heartbeat.
- Also write it here, as a short message, so it's waiting when he opens this session.

## 6. Ending

- **Everything left is waiting on Will:** notify once (section 5) and stay idle. The heartbeat keeps watching.
- **The project is Done** (`scripts/ticket merge` sets it once every ticket is): delete the heartbeat (`CronDelete`), tell Will in a few lines what merged while he was away and what he can try, and stop.
- **Will says stop:** delete the heartbeat and leave the ticket sessions as they are. Say which are mid-work.

## When Will comes back

When he writes here, lead with his inbox (`scripts/ticket inbox`): what needs him, and roughly how long it takes. Then, in a few lines: what merged since he was last here, what's running, and anything stalled. He clears the inbox (`scripts/listen` from anywhere, a merge go-ahead in the ticket's session), and the ticket sessions carry on from there.
