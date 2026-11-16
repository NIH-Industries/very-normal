# Brief

The default profile and named profiles are laid out differently. The default one is split across two places: the plan and made plans in `~/.config/very-normal/` (`plan.toml`, `plans/*.toml`, `plans/*.md`), and progress and charts in `~/.local/share/very-normal/` (`progress.json`, `charts.html`, with progress kept for other plans in `plans/*.json`). A named profile (`--profile <name>` or `VERY_NORMAL_HOME=<dir>`) keeps all of that in one folder, `~/.local/share/very-normal/profiles/<name>/`. So "where is my profile?" has two answers, and the default one isn't listed with the others.

Make every profile the same shape, a folder of its own, with the default as just another named profile. That probably means a default profile folder, and moving an existing split default into it the first time a new version runs, quietly and without losing progress or plans. Nothing should change for someone who has never touched profiles.

Things to keep in mind:

- Mark has already moved his own default by hand to `profiles/main` and set a universal fish variable `VERY_NORMAL_HOME` to it. That variable only exists in fish, so from zsh or bash the app falls back to the default location and starts the first-run onboarding. With one shape, choosing the profile that opens by default could become part of the app, not a shell variable.
- `VERY_NORMAL_DATA` and `VERY_NORMAL_PLAN` can point anywhere and take priority over everything else. Decide whether they stay, or whether profiles replace them.
- Nothing on screen says which profile you're in, so it's easy to practise into a test profile by accident. The dashboard header (one purpose: where you are in the long-term plan) could name the profile when it isn't the default.
- A way to list, rename and remove profiles may belong with this (today it's `ls`, `mv` and `rm -r`), alongside `very-normal plan`, `use`, `rename` and `remove` for plans.
- README "Where things live" and "A separate profile", `--help`, and the release notes need to describe the one shape.
