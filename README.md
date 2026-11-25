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
to `plans/` in your profile
(`~/.local/share/very-normal/profiles/main/plans/`), named after your layout:

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
   left before the next stage, and your best real text so far this stage
   against where you started and the speed the gate asks for. A slow
   session never shows as a step back. There's no rush.

The dashboard is the long view: every stage of the plan, with the keys it
moves and a bar for how much of your typing that is (by English letter
frequency, so the early stages are the big ones), how long finished stages
took, and how far into this one you are. Below it, the gate for the next
stage, or once that's passed, the Oryx edits to make.

Time guesses only come from your own practice, never a made-up average. The
stage you're on gets one after three real-text sessions at 95%+, from its
trend through your best so far at each session, so a slow one barely moves
it; stages ahead get one once a stage is behind you, from the minutes per
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

## Typing for real

Drills only get you so far: the layout sticks when you use it for real. Inside
Rex, `very-normal compose -- claude` starts Claude Code in a pane above and
turns this one into a small compose box. Type your message there, with
`enter` for a newline, and `ctrl-enter` (or `alt-enter`) sends it to Claude
whole, as a single paste. There's no lag while you type.

- `ctrl-w` and `alt-backspace` delete a word, and `ctrl-u` throws the message
  away. Neither is a mistake: a deleted word is a change of mind, and it comes
  out of your stats as if you never typed it.
- With nothing typed, `esc` and the arrows go straight to the other pane, so
  you can interrupt Claude or scroll its history. Nothing else goes through,
  and a plain `enter` never sends. To answer Claude's questions (permissions,
  menus), click into its pane. That's also where to type anything you'd
  rather wasn't measured, or to take a break on your old layout.
- `very-normal compose --to <pane>` composes into a pane that's already open.
  With no `--to` and one other pane in the window, it uses that one.

Every press of backspace counts as a correction, and the character you
type in its place shows in yellow. The status row shows your speed (counting
only time spent typing, not pauses to think) and how much you corrected. If
corrections pile up well past your usual, or you've typed for 50 minutes
straight, the status row suggests a break. It changes colour in place, so
nothing moves while you type.

What you type is never saved. Only per-key timings and counts are kept, as
compose runs that sit apart from your drills. The gate, the coach and the
weak-key drills never see them. The dashboard sums them up in one line: time
spent this week, speed, how much you corrected, and where you started.

### Anywhere on a Mac

Very Normal Compose is a small floating window that does the same for any
app, Rex or not. It lives in the menu bar, and **hyper-c**
(ctrl-opt-cmd-shift-c: put a Hyper key on your keyboard in Oryx, or on caps
lock with Karabiner) opens and closes it from anywhere. It types into
whichever app you were in when it opened (or whichever you click into while
it's open, since it never takes the focus away from that app):

- `ctrl-enter` pastes the message there and presses Return, and
  `ctrl-shift-enter` pastes without pressing Return. The window stays open
  for the next one.
- `esc`, `ctrl-c`, `cmd-w` or hyper-c again closes it, and the run is saved.
- The other keys are the compose pane's, and `cmd-backspace` clears too.
- Drag it anywhere; it remembers where. Its menu bar menu has Open at Login,
  and Quit.

It counts into the same profile as everything else, through
`very-normal compose --stdio`.

Build it from a checkout with `mac/build --install`, which puts it in
`~/Applications`, then open it once with Spotlight. The first time, macOS
asks you to allow it under Privacy & Security › Accessibility, so it can
press ⌘V and Return for you. It sends via the clipboard and puts back
what was there. The copy stays on this Mac (no Universal Clipboard) and is
marked transient, so clipboard history apps that honour nspasteboard.org
(Raycast, Alfred, Maccy, Paste) don't keep it. It looks for very-normal in
`~/.local/bin`, `~/.cargo/bin` and Homebrew, then on your login shell's PATH
(`$VERY_NORMAL_BIN` overrides). `open -a "Very Normal Compose" --args
--profile <name>` counts into another profile.

## Charts

`very-normal charts` (or `g` on the dashboard) writes
`charts.html` in your profile, one self-contained page that works
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

very-normal loads `plan.toml` from your profile, and falls back to Very Normal when there isn't one.

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

- Everything of yours lives in a profile: one folder holding `progress.json`
  (in the web tutor's backup format), `plan.toml` (the plan you're
  practising), `charts.html`, and `plans/` (the plans you've made, their
  summaries, and progress kept for the ones you're not practising). Profiles
  live in `~/.local/share/very-normal/profiles/<name>`, and `main` is the one
  that opens.
- Oryx layout: the plan's `oryx_layout`, or `$VERY_NORMAL_LAYOUT`. Checking
  uses the GraphQL endpoint Oryx's own page uses, which is undocumented and
  needs a public layout link.
- URLs open with `open` (`xdg-open` on Linux), or `$VERY_NORMAL_OPEN` (`{url}`
  marks the address).

### Profiles

`main` opens unless something names another profile: `--profile <name>`
before or after any command, `$VERY_NORMAL_PROFILE=<name>`, or
`$VERY_NORMAL_HOME=<dir>` for a profile folder anywhere. Another profile is a
clean slate for trying the first run or showing someone, and `main` is never
touched. The dashboard names the profile whenever it isn't `main`, so you
don't practise into a test one by accident.

```fish
very-normal --profile demo                   # a named profile you can come back to
very-normal --profile demo dashboard         # works with any command
set -Ux VERY_NORMAL_PROFILE work             # open work instead of main
VERY_NORMAL_HOME=(mktemp -d) very-normal     # a throwaway one, anywhere
very-normal profiles                         # list them, with the one in use marked
very-normal profile rename demo try-dvorak
very-normal profile remove try-dvorak        # deletes its plans and progress
```

You can't rename or remove the profile you're using; do it from another with
`--profile`. Before 0.7, the default profile was split between
`~/.config/very-normal` and `~/.local/share/very-normal`. The first run of a
newer version moves it into `profiles/main` (unless `main` already exists).
`$VERY_NORMAL_DATA` and `$VERY_NORMAL_PLAN` are gone: very-normal says so if
they're still set.

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
