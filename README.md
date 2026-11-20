# very-normal

A terminal typing tutor for moving your keyboard from the layout you type on
now (QWERTY, Dvorak, Colemak or anything else) to a new one a few keys at a
time, with a bossy coach and checks against your ZSA Oryx layout.

It ships with **Very Normal**, a layout optimised on 318k characters of my
own typing to Claude Code, with Q, W, Z, X, C and V locked so every Mac
shortcut stays where it was:

```
q w y d g   b h a u j
i l s t m   p n o e r  '
z x c v k   ; f , . /
```

## Install

```sh
brew install nih-industries/tap/very-normal
```

Or, on macOS or Linux without Homebrew:

```sh
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/NIH-Industries/very-normal/releases/latest/download/very-normal-installer.sh | sh
```

Or from source: `cargo install very-normal`.

Releases ship prebuilt binaries for Apple Silicon and Intel Macs and for
Linux on x86_64 and ARM, so none of these need Rust except the last.

## Make your own layout

very-normal can build a layout from your own typing, and a plan of stages
that gets you there from the layout you type on now. The first time you run
`very-normal`, it asks what that is and offers to do this from the messages
you've typed to Claude Code. It only
reads your messages: never Claude's replies, tool output or pastes. Any time
after that:

```sh
very-normal make-plan --name "My Layout"
```

Anything else you've written makes the layout fit you better. `--text` adds
files, folders of .md, .txt and .org files, or `-` for stdin. `--no-claude`
leaves your Claude Code messages out:

```sh
very-normal make-plan --name "My Layout" --text ~/notes ~/Documents/emails.txt
```

- `--from` is the layout you type on now: `qwerty` (the default), `dvorak`,
  `colemak`, `colemak-dh`, `workman` or `norman`; any plan, by its name or
  its `.toml` file, which means the layout it ends at (`take-two@3` or
  `plan.toml@3` for its stage 3); or your own 30 keys written top row first,
  like `"qwertyuiop asdfghjkl; zxcvbnm,./"`. The stages start there. Since a
  plan file is the same format `make-plan` writes, any layout you can write
  as a plan can be a starting point.
- `--lock` takes keys that stay where they are now. The default keeps Q, W,
  Z, X, C and V where they are, for their Cmd shortcuts, and the punctuation
  (`, . /` and `'`). Each locked key costs the layout a little, and
  `--lock ""` locks nothing.
- `--oryx-layout <id>` reads your home row holds from Oryx, if you have a ZSA
  board.
- `very-normal make-plan --help` lists the rest.

It takes a few seconds, and nothing leaves your machine. It writes two files
to `~/.config/very-normal/plans/`, named after your layout:

- `<name>.toml`: your layout and its stages, with drill words from your own
  writing. Keep it private, and look through the words for names you'd rather
  not see on screen.
- `<name>.md`: your layout and how it scores on your typing against where
  you started and QWERTY, Dvorak, Colemak, Colemak-DH, Workman, Norman and
  Very Normal. It has no words or text in it.

Making a plan never touches the plan you're practising or your progress, so
rerun it as often as you like with other text, locks or names. Your first
plan becomes the active one. After that, switch with `very-normal use
<name>` (`very-normal use very-normal` goes back to the built-in plan). Each
plan keeps its own progress, so you can switch back and pick up where you
left off. A plan whose stages are exactly the same as the one you're
practising takes your progress with it. `very-normal plan` lists the plans
you've made, and `very-normal remove <name>` deletes one.

**Share your layout.** I'm curious how different everyone's layouts turn
out. `very-normal share` opens a GitHub issue with the summary of the plan
you made most recently filled in (`very-normal share <name>` for another).
You can read it, and change it, before you press Submit. The summary also
says how much the search agreed with itself. It runs 20 times from random
starting points, and many layouts score almost the same, so some of the
difference between two people's layouts is chance. The summary shows how
much.

See [MAKE_YOUR_OWN.md](MAKE_YOUR_OWN.md) for how the layout is scored and
searched for.

## Just practise, with no new layout

very-normal is also a plain typing tutor for the layout you already have.
Choose "no new layout" the first time you run it, or any time after:

```sh
very-normal use qwerty       # or dvorak, colemak, colemak-dh, workman, norman
```

That makes a plan with one stage, your layout, and nowhere to go. Practice
takes turns between real text and **weak keys**: everyday words leaning on
the keys you've been slowest on or missed most lately. Key recall drills the
whole board. The dashboard shows how you're getting on instead of stages:
where you started (your first three real-text runs), your last few runs
against that, your best, and your weakest keys. There's nothing to change in
Oryx, so `next` and `check` have nothing to say. Switch to a plan that moves
keys whenever you're curious, and back again: each keeps its own progress.

The drill words are your own when there's enough of your typing to Claude
Code (read on your machine, and kept in the plan file, so keep it private).
Otherwise they're 500 everyday English words built into very-normal, counted
from the English sentences of [Tatoeba](https://tatoeba.org) under
[CC BY 2.0 FR](https://creativecommons.org/licenses/by/2.0/fr/).

## Use

```sh
very-normal                  # a guided session: the layout, a warm-up, then practice
very-normal dashboard        # the whole plan: where you are and roughly how long is left
very-normal status           # what the coach thinks of your practice today
very-normal next             # the Oryx edits for the next stage, and the link to make them
very-normal check            # read the layout from Oryx and compare it with your stage
very-normal plan             # which plan you're on, and its stages
very-normal make-plan        # make a layout and plan from your own typing
very-normal use <name>       # switch plans; each keeps its own progress
very-normal use qwerty       # no new layout: just practise on QWERTY (or dvorak, colemak…)
very-normal remove <name>    # delete a plan you made, with its summary and progress
very-normal rename <name> <new name>   # give a plan you made a better name
very-normal share [name]     # share a plan's summary (no text) on GitHub
very-normal keymap kanata    # this stage as a kanata config (or karabiner), for any keyboard
very-normal stage 2          # set the stage you're practising
very-normal baseline 35      # type in a baseline instead of recording one
very-normal import backup.json   # merge a backup copied out of the web tutor
very-normal charts           # a page of charts from your progress, opened in the browser
very-normal export           # print progress as JSON
```

Find a quiet place where you won't be interrupted, and put headphones on.
On the layout, warm-up and report screens, `m` plays gentle music in the
background: SomaFM's Drone Zone, an ambient Icecast stream, unless
`VERY_NORMAL_MUSIC` names another stream or file. It needs `mpv`
(`brew install mpv`) or `ffplay`, and stops when the tutor does. Any gentle
music you like works just as well.

A guided session has four steps, one screen each:

1. **Layout**: your layout on its own, with the keys that move this stage in
   pink. Press enter when you've had a look (`d` skips straight to the
   dashboard).
2. **Warm-up**: a minute of slow breathing and hand stretches.
3. **Practice**: ten minutes of drills picked for you, real text first while
   you're fresh, then new keys and review, with the keyboard shown the whole
   time. The clock counts typing time only and moves between drills. A miss
   shows in soft lavender, not red. A run under 95% gets a 20-second
   breather after it, and the halfway mark a 30-second stretch; `enter`
   skips either. Two runs in a row under 95% ease off to key recall, one key
   at a time, after a longer breather. Two more after that end practice:
   that's the time to rest. New keys, review and key recall give extra reps
   to the keys you've been slowest on or missed most over your last couple
   of sessions, spread out so they never bunch up, except straight after a
   run under 95%, when the next drill is easy wins. Real text is never
   weighted. If you quit before 10:00, the next practice today
   picks up where you left off; once a session reaches 10:00, the next one
   starts at 0:00, and so does tomorrow's.
4. **Report**: this session against your last few, your slowest keys, what's
   left before the next stage, and roughly how many sessions the real-text
   trend says it'll take. There's no rush.

The dashboard is the long view: every stage of the plan, with the keys it
moves and a bar for how much of your typing that is (by English letter
frequency, so the early stages are the big ones), how long finished stages
took, and how far into this one you are. Below it, the gate for the next
stage, or once that's passed, the Oryx edits to make.

Time guesses only come from your own practice, never a made-up average. The
stage you're on gets one after three real-text sessions at 95%+, from its
trend; stages ahead get one once a stage is behind you, from the minutes per
point of typing moved that it took you. They're ranges in words ("2–4
weeks"), not dates.

- `enter` starts today's session. `1`–`6` open one drill of your own on the
  drill screen, with no clock: key recall, new keys, review, 60 seconds of
  real text, markdown links, or one-handed mirror practice. `esc` comes back.
- `←` `→` change stage. `c` checks Oryx and `o` opens the layout there. `g`
  opens your charts.
- `k`, `v` and `w` tick the gate's manual boxes.

## The coach

- There's no rush. Aim for two 10-minute sessions a day, one in the morning
  and one later, at least an hour apart. The gap is part of how it sticks.
  Ten minutes in a day keeps your streak going, and a day off is a rest day.
- After 15 minutes in a row it tells you to wrap up. After 25, or 45 minutes
  in a day, it won't start a drill unless you press enter twice.
- A five-minute pause counts as a break.
- If a session starts going backwards (accuracy a few points under your best
  earlier in the session, speed 15% down, three gate misses in a row, or
  three restarts in ten minutes), it suggests a break. That's usually
  frustration or tired fingers, not you getting worse.

## Charts

`very-normal charts` (or `g` on the dashboard) writes
`~/.local/share/very-normal/charts.html`, one self-contained page that works
offline, and opens it. It shows:

- speed and accuracy run by run, with stages shaded and the gate's lines
- how fast each stage climbs (a trend per stage) and where each stage
  started and got to against your QWERTY baseline
- the board as a heatmap of slow or missed keys, the slowest and most missed
  keys, and each key's speed over time
- what you typed instead of the key you wanted, tagged when your finger went
  to where that key used to be, and the slowest pairs of keys
- a practice calendar, and whether you slow down late in a session or at
  certain times of day

Runs saved before charts existed have no per-run key timings, typos or key
pairs, so those charts fill in as you practise.

## The warm-up

The first drill of the day starts with a guided one-minute warm-up: slow
breaths, then gentle hand and wrist stretches. `enter` moves to the next step,
`s` skips it (the coach will judge you), and `u` runs it any time.

## The gate

Before climbing a stage:

1. New-keys runs at 97%+ accuracy in 3 of your last 5.
2. Real text back to 90% of your baseline speed at 95%+ accuracy.
3. That stage's vim commands used in real editing without fumbling.
4. A full workday without thinking about the moved keys.

## Plans

A plan is a TOML file listing every stage's 30 keys, with a name, blurb,
stats, vim commands and drill words for each. See
[examples/very-normal.toml](examples/very-normal.toml).

very-normal loads `~/.config/very-normal/plan.toml` (or `$VERY_NORMAL_PLAN`),
and falls back to Very Normal when there isn't one.

## Any keyboard

Oryx is only one way to get a stage onto a keyboard. `very-normal keymap
kanata` prints the current stage as a [kanata](https://github.com/jtroo/kanata)
config (macOS, Linux, Windows), and `very-normal keymap karabiner` prints a
[Karabiner-Elements](https://karabiner-elements.pqrs.org) complex modification
for `~/.config/karabiner/assets/complex_modifications/`. Add a stage number to
get a different one.

Both remap in software and build the whole layout themselves, whatever you
started from, so set the computer to the U.S. (QWERTY) layout while they're
on. If your plan's 30 keys take in the quote (as Dvorak's do), the quote key
beside the home row types what they leave out. They expect the keyboard to
send QWERTY, so a keyboard
whose firmware already has the stage must be left out, or its keys move twice.
The Karabiner rule only applies to the Mac's built-in keyboard (add
`--all-keyboards` to change that), so a remapped Voyager and a Very Normal
laptop can live side by side. The kanata config includes a commented
`macos-dev-names-include` line that does the same on macOS.

## Where things live

- Progress: `~/.local/share/very-normal/progress.json`, or `$VERY_NORMAL_DATA`.
  The format matches the web tutor's backup text. Progress on the plans
  you're not practising waits in `plans/` next to it.
- Plans you've made: `~/.config/very-normal/plans/`, next to the active
  `plan.toml`.
- Oryx layout: the plan's `oryx_layout`, or `$VERY_NORMAL_LAYOUT`. Checking
  uses the GraphQL endpoint Oryx's own page uses, which is undocumented and
  needs a public layout link.
- URLs open with `open` (`xdg-open` on Linux), or `$VERY_NORMAL_OPEN` (`{url}`
  marks the address).

### A separate profile

`$VERY_NORMAL_HOME` keeps a whole profile in one folder: `progress.json` and
`plan.toml` there, with made plans and charts beside them. It's a clean slate
for trying the first run or showing someone, and your own progress is never
touched. `$VERY_NORMAL_DATA` and `$VERY_NORMAL_PLAN` still win if they're set.

```fish
very-normal --profile demo                   # a named profile you can come back to
very-normal --profile demo dashboard         # works with any command
VERY_NORMAL_HOME=(mktemp -d) very-normal     # a throwaway one, anywhere
```

Named profiles live in `~/.local/share/very-normal/profiles/<name>`; delete
the folder to start that one over.

The first run's offer to build a plan from your Claude Code messages still
reads them from `~/.claude`, so pick the built-in plan if you're showing
someone else.

## Build

```sh
mise run test
mise run lint
cargo install --path .
```

## Release

```sh
mise run publish 0.3.0 --dry-run   # build everything and show what would happen
mise run publish 0.3.0             # bump, tag, GitHub release, crates.io, Homebrew tap
```

It builds the macOS and Linux binaries on this machine with
[dist](https://opensource.axo.dev/cargo-dist/), cross-compiling Linux with
cargo-zigbuild. It needs `brew install cargo-dist zig cargo-zigbuild` and
`rustup target add x86_64-apple-darwin x86_64-unknown-linux-gnu
aarch64-unknown-linux-gnu`. Release notes go in `tmp/release-notes.md`.
