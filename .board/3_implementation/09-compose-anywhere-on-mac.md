# Brief

Compose is useful but tied to Rex: it can only send to another Rex pane. Make a small Swift app that opens a movable window, and sends each message as a chunk into whatever app had focus when it opened, until dismissed with esc or ctrl-c. Its typing goes into the same profile. Alt-backspace deletes a whole word, and that should be ignored as a mistake: it's changing your mind about what to type.

---

## Goal

Compose works from any Mac app, and a deleted word no longer counts against you anywhere.

## Acceptance criteria

1. `very-normal compose --stdio` reads keys as JSON lines (`{"k":"char","c":"x"}`, `delete`, `word`, `clear`, `send`) and writes what to show after each (segments with fixed marks, wpm, corrected, nudge, and the message on send). It saves runs as they fall due and when stdin closes.
2. A word delete takes the word's keys out of keys, flow time, key timings, typos and the nudge window, in the open run and the totals. It's not a correction and marks nothing yellow. Backspace is unchanged.
3. `mac/build [--install]` builds Very Normal Compose.app: a floating, non-activating panel that sends each message to the app last in front by pasting (⌘V, then Return unless shift is held), restoring the clipboard, and closes on esc, ctrl-c, cmd-w or cmd-q.
4. README, `--help` and the changelog describe both. `cargo test` and `cargo clippy --all-targets -- -D warnings` pass.

## Out of scope

- Recounting older compose runs (their text was never kept). Mark's six, from before this change, were dropped from his profile.
- A global hotkey inside the app.

## Open questions

None.
