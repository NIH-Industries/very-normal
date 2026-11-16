# Changelog

## Unreleased

## 0.5.0 - 2026-11-16

- `rename <plan> <new name>` gives a plan you made a better name: in the plan, its stage names, its summary, its files and any progress kept for it, even while you're practising it.
- `make-plan --from` (and the first run's question) also takes a plan, by name or `.toml` file: where it ends, or `name@N` for its stage N. Any layout written as a plan can be a starting point.
- `--profile <name>` and `VERY_NORMAL_HOME=<dir>` keep a whole separate profile (progress, plan, made plans, charts) in one folder, for trying the first run or showing someone without touching your own progress. Named profiles live in `~/.local/share/very-normal/profiles/`.
- The dashboard is the long view of the plan and nothing else: every stage with the keys it moves, a bar for how much of your typing that is, how long finished stages took, how far into this one you are, and the gate (or, once it's passed, the next Oryx edits). The trainer, keyboard map and coach line are gone from it.
- Time guesses come only from your own practice: the current stage's from its real-text trend after three sessions at 95%+, stages ahead from your pace on stages you've finished. Shown as ranges in words, never dates.
- `1`–`6` on the dashboard open a drill of your own on the drill screen, with no session clock; `esc` comes back. `enter` starts today's guided session. `n` and `u` are gone.
- The report says how many of today's two sessions you've done.
- The end-of-session report is laid out in sections (drills, slowest keys, what's left before the next stage, the real-text outlook) with room between them, and each drill type has its own colour there, in the help and on the practice screen.
- Quit practice before 10:00 and the next practice that day picks up where you left off: the clock, the drill order and the report carry on. A session that reaches 10:00 is finished, so the next one starts at 0:00, as does each new day.
- The practice clock reads "7:00 of 10:00 done" instead of counting down.
- The help screen and `--help` say what `m` plays (SomaFM's Drone Zone, or your `VERY_NORMAL_MUSIC`), and the no-player message stops suggesting `VERY_NORMAL_MUSIC`, which only picks the stream.
- `?` opens a help screen on every screen except mid-drill: how the guided session runs, what each drill is for at your stage, the music and the keys, spaced out in sections. ↑/↓ scroll it on a short terminal.
- Keys are ignored for a moment after a drill ends, so the letters you were still typing when the clock ran out can't stop the music or quit from the report.

## 0.4.0 - 2026-11-14

- Plain `very-normal` starts a guided session. It shows the layout on its own, then the warm-up, then ten minutes of practice it picks for you, real text first. It stops early after two sloppy runs. The old screen moves to `very-normal dashboard`.
- The guided session ends with a report: this session against recent ones, your slowest keys, and an unhurried look at the next stage.
- The guided session suggests a quiet place and headphones, and `m` plays an ambient stream in the background.
- The coach counts two calm 10-minute sessions a day instead of nagging toward 20 minutes.
- The practice screen shows the key map the whole time and nothing on it changes when you pause, so the text never moves while you type.
- Keys you type while the music plays reach the drill. mpv used to read the terminal itself and swallow some of them.
- Exits quietly when a pipe closes early instead of panicking on a broken pipe.

## 0.3.0 - 2026-11-13

- Start from the layout you type on now: `make-plan --from` takes QWERTY, Dvorak, Colemak, Colemak-DH, Workman, Norman or your own 30 keys, and the first run asks which one you use.
- Locks keep keys where they are on your current layout, summaries compare against all the known layouts, and keymaps send the quote key to whatever the 30 keys leave out.

## 0.2.1 - 2026-11-13

- Prebuilt binaries for macOS and Linux, installable from the Homebrew tap or with a shell installer.

## 0.2.0 - 2026-11-13

- `make-plan` builds your own layout and plan inside the binary, with no Python, into `plans/` without touching the plan you're practising.
- The first run offers to make a plan from your Claude Code messages and shows how to add other writing.
- `use` switches plans and keeps each plan's progress. It carries your progress over to a plan whose stages are exactly the same.
- `remove` deletes a plan you made, along with its summary and progress.
- `share` opens a GitHub issue with your layout, its stages from QWERTY, and how much the search's restarts agreed.
- Links open with `xdg-open` on Linux.
- Words drills lean on your trickiest keys. Words that use them come up as much as 3x as often, and the drill says which keys are getting extra reps.
- Charts: a self-contained page of speed, accuracy, per-stage climbs, key heatmaps, typos, slow pairs and practice habits.
- A kinder coach: it wants 97% in 3 of the last 5 runs, rounds accuracy the same way for showing and judging, offers encouragement and speed records instead of resets, and suggests a break when a session goes backwards.
- Karabiner keymaps only touch the built-in keyboard unless you pass `--all-keyboards`, and the kanata config explains device filtering.

## 0.1.0 - 2026-11-12

- A terminal tutor for moving onto the Very Normal layout on a Voyager in stages, with a bossy practice coach.
- Stages, words and Oryx settings load from a plan file, with Very Normal as the built-in example.
- A guided one-minute warm-up before the first drill of the day.
- `very-normal keymap` turns any stage into a kanata config or a Karabiner rule, for keyboards without Oryx.
- The header and coach stay visible in short terminals.
