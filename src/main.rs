use std::io::{BufRead, IsTerminal, Read, Write};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use ratatui::DefaultTerminal;
use ratatui::crossterm::event::{self, Event, KeyEventKind};
use very_normal::app::{App, Phase};
use very_normal::compose::{self, Action, Composer, Screen};
use very_normal::layout;
use very_normal::optimise::{self, Options};
use very_normal::store::{self, Progress};
use very_normal::{charts, coach, corpus, keyboard, keymap, named, plans, profile, report};

const HELP: &str = "\
very-normal: move your keyboard to a layout made from your own typing

usage: very-normal                  a guided session: layout, warm-up, practice
       very-normal dashboard        the whole plan: where you are, roughly how long is left
       very-normal status           what the coach thinks of your practice today
       very-normal next             the keyboard edits for the next stage, and where to make them
       very-normal check            read your keyboard's layout and compare it with your stage
       very-normal stage <n>        set the stage you're practising (0-9)
       very-normal baseline <wpm>   type in a baseline instead of recording one
       very-normal import <file|->  merge a backup copied out of the web tutor
       very-normal plan             which plan you're on, and its stages
       very-normal make-plan [...]  make a layout and plan from your own typing
                                    (very-normal make-plan --help for its options)
       very-normal use <name>       switch plans; each plan keeps its own progress
       very-normal use qwerty       no new layout: just practise on QWERTY (or dvorak,
                                    colemak, colemak-dh, workman, norman)
       very-normal remove <name>    delete a plan you made, its summary and its progress
       very-normal rename <name> <new name>
                                    give a plan you made a better name
       very-normal share [name]     share a plan's summary (no text) on GitHub
       very-normal profiles         your profiles, with the one in use marked
       very-normal profile rename <name> <new name>
       very-normal profile remove <name>
                                    rename or delete a profile (not the one in use)
       very-normal keymap kanata|karabiner [stage] [--all-keyboards]
                                    a software keymap, for keyboards you can't reprogram
                                    (karabiner: built-in keyboard only, unless --all-keyboards)
       very-normal charts [file] [--no-open]
                                    write your progress as a page of charts and open it
       very-normal compose [--to <pane>] [-- <command>]
                                    type for real and still have it count: write a
                                    message in this rex pane and ctrl-enter sends it
                                    to another (-- claude starts claude above this one)
       very-normal compose --stdio  the same, for a window that draws itself (the Mac
                                    compose window): keys as JSON lines in, what to show out
       very-normal export           print your progress as JSON
       very-normal --help | --version

A profile is a folder holding your progress, plan and made plans, in
~/.local/share/very-normal/profiles/<name>. main opens unless --profile <name>
(before or after any command), $VERY_NORMAL_PROFILE=<name> or
$VERY_NORMAL_HOME=<any folder> says otherwise. Your keyboard's layout (a
layout.txt, or an Oryx layout id) comes from your plan ($VERY_NORMAL_LAYOUT);
files and URLs open with `open`, or `xdg-open` on
Linux ($VERY_NORMAL_OPEN, where {url} marks the address). `m` plays SomaFM's
Drone Zone with mpv or ffplay ($VERY_NORMAL_MUSIC: any stream URL or file).
";

fn main() -> Result<()> {
    // Rust ignores SIGPIPE, which turns `very-normal plan | head` into a
    // "Broken pipe" panic; the default just ends the program quietly.
    #[cfg(unix)]
    unsafe {
        libc::signal(libc::SIGPIPE, libc::SIG_DFL);
    }
    let home = std::path::PathBuf::from(std::env::var_os("HOME").unwrap_or_default());
    let moved = profile::migrate(&home).context("moving your profile into profiles/main")?;
    if !moved.is_empty() {
        eprintln!(
            "Moved your progress and plans into {}, where every profile lives now.",
            profile::root(&home).join(profile::DEFAULT).display()
        );
    }
    for gone in ["VERY_NORMAL_DATA", "VERY_NORMAL_PLAN"] {
        if std::env::var_os(gone).is_some_and(|v| !v.is_empty()) {
            eprintln!(
                "${gone} isn't used any more: progress and plans live in the profile folder. Use --profile <name>, $VERY_NORMAL_PROFILE or $VERY_NORMAL_HOME."
            );
        }
    }
    let (args, dir) = profile::resolve(
        std::env::args().skip(1).collect(),
        &home,
        std::env::var_os("VERY_NORMAL_HOME"),
        std::env::var_os("VERY_NORMAL_PROFILE"),
    )?;
    std::fs::create_dir_all(&dir)
        .with_context(|| format!("making the profile folder {}", dir.display()))?;
    // SAFETY: still single-threaded, and nothing has read the environment yet
    unsafe { std::env::set_var("VERY_NORMAL_HOME", &dir) };
    let path = store::default_path();
    if args.is_empty()
        && !layout::plan_path().exists()
        && !path.exists()
        && std::io::stdin().is_terminal()
        && !welcome(&path)?
    {
        return Ok(());
    }
    let plan_file = layout::install()?;
    let words: Vec<&str> = args.iter().map(String::as_str).collect();
    match words.as_slice() {
        [] | ["dashboard"] => {
            let phase = if words.is_empty() {
                Phase::Layout
            } else {
                Phase::Dashboard
            };
            let progress = Progress::load(&path)?;
            let mut terminal = ratatui::init();
            let result = run(
                &mut terminal,
                App::new(progress, path, Instant::now(), phase),
            );
            ratatui::restore();
            result?;
        }
        ["-h" | "--help" | "help"] => print!("{HELP}"),
        ["-V" | "--version"] => println!("very-normal {}", env!("CARGO_PKG_VERSION")),
        ["next" | "check"] if layout::staying() => println!(
            "You're practising on {} with no new layout, so there's nothing to change on your keyboard.",
            layout::plan().name
        ),
        ["next"] => {
            let p = Progress::load(&path)?;
            for line in report::next_stage(p.stage, &keyboard::configured().place()) {
                println!("{line}");
            }
        }
        ["check"] => {
            let p = Progress::load(&path)?;
            let snap = keyboard::configured().read()?;
            println!("You're practising stage {}.", p.stage);
            for line in report::check(&snap, p.stage) {
                println!("{line}");
            }
        }
        ["stage", n] => {
            let n: usize = n.parse().context("the stage is a number")?;
            if n > layout::last() {
                bail!("there are only stages 0 to {}", layout::last());
            }
            let mut p = Progress::load(&path)?;
            p.stage = n;
            p.save(&path)?;
            println!("Now practising stage {n}.");
        }
        ["baseline", wpm] => {
            let wpm: f64 = wpm
                .parse()
                .context("the baseline is a number of words per minute")?;
            let mut p = Progress::load(&path)?;
            p.manual_mut(0).wpm = wpm;
            p.save(&path)?;
            println!("Baseline set to {wpm:.0} WPM.");
        }
        ["import", file] => {
            let text = if *file == "-" {
                let mut s = String::new();
                std::io::stdin().read_to_string(&mut s)?;
                s
            } else {
                std::fs::read_to_string(file).with_context(|| format!("reading {file}"))?
            };
            let incoming =
                Progress::parse(text.trim()).context("that isn't a backup from the tutor")?;
            let mut p = Progress::load(&path)?;
            let added = p.merge(incoming);
            p.save(&path)?;
            println!("Imported {added} new run(s). Practising stage {}.", p.stage);
        }
        ["status"] => {
            let p = Progress::load(&path)?;
            let a = coach::assess(&p.runs, now_ms()?);
            println!("{}", a.message);
            println!(
                "today {}/{} sessions · {:.0} min · {}-day streak · practising stage {}",
                a.sessions_today,
                coach::SESSIONS_PER_DAY,
                a.today_minutes,
                a.streak,
                p.stage
            );
        }
        ["plan"] => {
            let plan = layout::plan();
            match &plan_file {
                Some(p) => println!("Plan: {} ({})", plan.name, p.display()),
                None => println!(
                    "Plan: {} (built in; make your own at {})",
                    plan.name,
                    layout::plan_path().display()
                ),
            }
            for s in 0..=layout::last() {
                println!("  {s}: {}  {}", layout::keys_string(s), layout::name(s));
            }
            let active = plans::active_slug();
            let others: Vec<String> = plans::list().into_iter().filter(|p| *p != active).collect();
            if !others.is_empty() {
                println!(
                    "\nOther plans: {} (switch with `very-normal use <name>`)",
                    others.join(", ")
                );
            }
        }
        ["use", name] => println!("{}", plans::switch(name, &path)?),
        ["profiles"] => {
            for line in profile::listing() {
                println!("{line}");
            }
        }
        ["profile", "rename", old, new] => println!("{}", profile::rename(old, new)?),
        ["profile", "remove", name] => println!("{}", profile::remove(name)?),
        ["remove", name] => println!("{}", plans::remove(name, &path)?),
        ["rename", old, new @ ..] if !new.is_empty() => {
            println!("{}", plans::rename(old, &new.join(" "), &path)?)
        }
        ["make-plan", "-h" | "--help"] => print!("{}", optimise::USAGE),
        ["make-plan", rest @ ..] => {
            let made = optimise::make(&Options::parse(rest)?)?;
            report_made(&made);
        }
        ["share"] => share(&latest_summary()?)?,
        ["share", name] => {
            let file = std::path::Path::new(name);
            if file.is_file() {
                share(file)?
            } else {
                share(&plans::summary_of(&plans::made_path(name)))?
            }
        }
        ["keymap", kind, rest @ ..] => {
            let all = rest.contains(&"--all-keyboards");
            let stage = match rest
                .iter()
                .filter(|a| **a != "--all-keyboards")
                .collect::<Vec<_>>()[..]
            {
                [] => Progress::load(&path)?.stage,
                [n] => n.parse().context("the stage is a number")?,
                _ => bail!("usage: very-normal keymap kanata|karabiner [stage] [--all-keyboards]"),
            };
            if stage > layout::last() {
                bail!("there are only stages 0 to {}", layout::last());
            }
            match *kind {
                "kanata" => print!("{}", keymap::kanata(stage)),
                "karabiner" => print!("{}", keymap::karabiner(stage, all)),
                other => bail!("no {other} keymap; try kanata or karabiner"),
            }
        }
        ["charts", rest @ ..] => {
            let open = !rest.contains(&"--no-open");
            let out = match rest
                .iter()
                .filter(|a| **a != "--no-open")
                .collect::<Vec<_>>()[..]
            {
                [] => charts::default_path(&path),
                [f] => std::path::PathBuf::from(f),
                _ => bail!("usage: very-normal charts [file] [--no-open]"),
            };
            let p = Progress::load(&path)?;
            charts::write(&p, &out, now_ms()?)?;
            println!("Charts written to {}", out.display());
            if open {
                very_normal::open_url(&out.display().to_string())?;
            }
        }
        ["compose", rest @ ..] => compose(rest, &path)?,
        ["export"] => println!("{}", serde_json::to_string_pretty(&Progress::load(&path)?)?),
        _ => {
            eprint!("{HELP}");
            bail!("didn't understand: {}", args.join(" "));
        }
    }
    Ok(())
}

/// The first run, with no plan and no progress yet: offers to make a layout
/// from your own typing. Says whether to carry on into the app.
fn welcome(progress: &std::path::Path) -> Result<bool> {
    let projects = corpus::claude_projects();
    let typed = corpus::typed_messages(&projects).len();
    println!("Welcome to very-normal.\n");
    println!("very-normal can make you a keyboard layout from your own typing, and a plan");
    println!("that moves you there from the layout you type on now, a few keys at a time.");
    println!("Or it can just be a typing tutor for the layout you already have.\n");
    let enough = typed >= optimise::ENOUGH;
    if enough {
        println!(
            "It found {typed} messages you typed to Claude Code in {}.",
            projects.display()
        );
        println!("(Only your messages: never Claude's replies, tool output or pastes.)\n");
    } else {
        println!(
            "It found {typed} messages you typed to Claude Code in {}; it needs {} or more.\n",
            projects.display(),
            optimise::ENOUGH
        );
    }
    println!("Anything else you've written makes the layout fit you better. Add files or");
    println!("folders of .md, .txt or .org files (and `--no-claude` to leave Claude Code out):\n");
    println!("  very-normal make-plan --from dvorak --text ~/notes ~/Documents/emails.txt\n");
    if enough {
        println!("  1  make my layout from my Claude Code messages now (about a minute)");
    }
    println!("  2  use Very Normal, the built-in layout, for now (it starts from QWERTY)");
    println!("  3  no new layout: just practise on the one I type on now");
    println!("  q  quit, and come back with make-plan\n");
    let choice = ask(if enough {
        "Choose [1]: "
    } else {
        "Choose [2]: "
    })?;
    let choice = match choice.as_str() {
        "" if enough => "1",
        "" => "2",
        c => c,
    };
    match choice {
        "1" if enough => {
            let mut o = Options {
                from: ask_start()?,
                ..Options::default()
            };
            let name = ask("What should your layout be called? [My Layout]: ")?;
            if !name.is_empty() {
                o.name = name;
            }
            println!();
            let made = optimise::make(&o)?;
            report_made(&made);
            if ask("\nShare your summary on GitHub? It has no text or words, just the layout and its scores. [y/N]: ")?
                .eq_ignore_ascii_case("y")
            {
                share(&made.summary_path)?;
            }
            Ok(ask("\nPress enter to start training, or q to quit: ")? != "q")
        }
        "2" => {
            Progress::default().save(progress)?;
            Ok(true)
        }
        "3" => {
            let (name, keys) = plans::start(&ask_start()?)?;
            let name = if named::find(&name).is_some() {
                name
            } else {
                "My Layout".to_string()
            };
            let (made, words) = plans::stay(&name, &keys)?;
            plans::activate(&made)?;
            Progress::default().save(progress)?;
            println!(
                "\nPractising on {name}, with no new layout. {words} `very-normal make-plan` any time you're curious."
            );
            Ok(ask("\nPress enter to start training, or q to quit: ")? != "q")
        }
        _ => Ok(false),
    }
}

/// Which layout you type on now: one of the usual ones, or your own 30 keys.
fn ask_start() -> Result<String> {
    println!("\nWhich layout do you type on now?\n");
    for (i, name) in named::STARTS.iter().enumerate() {
        println!("  {}  {name}", i + 1);
    }
    println!("  or type your own 30 keys, top row first, like qwertyuiop asdfghjkl; zxcvbnm,./");
    println!("  or a plan's name or .toml file, for where it ends (add @N for its stage N)\n");
    loop {
        let answer = ask("Layout [1]: ")?;
        let pick = match answer.as_str() {
            "" => named::STARTS[0].to_string(),
            n if n
                .parse::<usize>()
                .is_ok_and(|n| (1..=named::STARTS.len()).contains(&n)) =>
            {
                named::STARTS[n.parse::<usize>()? - 1].to_string()
            }
            other => other.to_string(),
        };
        match plans::start(&pick) {
            Ok((name, keys)) => {
                println!("\n{}\n", optimise::pretty(&keys));
                println!("Starting from {name}.");
                return Ok(pick);
            }
            Err(e) => println!("{e}"),
        }
    }
}

fn ask(prompt: &str) -> Result<String> {
    print!("{prompt}");
    std::io::stdout().flush()?;
    let mut line = String::new();
    std::io::stdin().lock().read_line(&mut line)?;
    Ok(line.trim().to_string())
}

fn report_made(made: &optimise::Made) {
    println!("{}", made.summary);
    println!(
        "Your plan ({} stages) is in {}. Its drill words come from your own writing,",
        made.stages,
        made.plan_path.display()
    );
    println!("so keep it private, and look through them for names you'd rather not see on screen.");
    println!(
        "The summary in {} has no words or text in it.\n",
        made.summary_path.display()
    );
    if !made.name_ideas.is_empty() {
        println!(
            "Name ideas you can type on your new home row: {}",
            made.name_ideas.join(", ")
        );
        println!("(Run make-plan again with --name; the same seed gives the same layout.)\n");
    }
    if made.activated {
        println!("It's the plan you're practising now.");
    } else {
        println!("Your current plan and progress are untouched. To practise this one:");
        println!("  very-normal use {}", made.slug);
        println!("Each plan keeps its own progress, so you can switch back any time.\n");
    }
    println!("Next: `very-normal plan` to see the stages, `very-normal` to start training,");
    println!("and `very-normal share` to add your layout to the collection. I'm curious how");
    println!("much everyone's layouts differ.");
}

/// The summary of the plan you made most recently.
fn latest_summary() -> Result<std::path::PathBuf> {
    std::fs::read_dir(plans::dir())
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "md"))
        .max_by_key(|p| std::fs::metadata(p).and_then(|m| m.modified()).ok())
        .ok_or_else(|| {
            anyhow::anyhow!("no summary to share; make a plan first with `very-normal make-plan`")
        })
}

/// Opens a new GitHub issue with the summary filled in, for you to check and post.
fn share(summary: &std::path::Path) -> Result<()> {
    let text = std::fs::read_to_string(summary).with_context(|| {
        format!(
            "reading {}; make a plan first with `very-normal make-plan`",
            summary.display()
        )
    })?;
    let url = optimise::share_url(&text);
    if url.len() > optimise::LONGEST_LINK {
        println!(
            "The summary is too long for a link. Open this page and paste in {}:",
            summary.display()
        );
        let short = optimise::share_url(text.lines().next().unwrap_or("# My layout"));
        println!("{short}");
        return very_normal::open_url(&short);
    }
    println!(
        "Opening a new issue with your summary filled in. Nothing is posted until you press Submit."
    );
    println!("{url}");
    very_normal::open_url(&url)
}

fn now_ms() -> Result<u64> {
    Ok(std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_millis() as u64)
}

/// Runs the TUI until it quits, redrawing often so timers and hints move.
fn run(terminal: &mut DefaultTerminal, mut app: App) -> Result<()> {
    while !app.should_quit {
        let now = Instant::now();
        app.tick(now);
        terminal.draw(|frame| app.render(frame, now))?;
        if event::poll(Duration::from_millis(100))?
            && let Event::Key(key) = event::read()?
            && key.kind == KeyEventKind::Press
        {
            app.handle_key(key, Instant::now());
        }
    }
    Ok(())
}

/// Compose mode: this rex pane is where you type, and each message goes to
/// another pane, while the typing is measured. The text is never saved.
fn compose(args: &[&str], path: &std::path::Path) -> Result<()> {
    if args == ["--stdio"] {
        return compose_stdio(path);
    }
    let (Ok(session), Ok(me)) = (std::env::var("REX_SESSION"), std::env::var("REX_BLOCK")) else {
        bail!("compose sends what you type to another rex pane, so run it inside rex");
    };
    let usage = "usage: very-normal compose [--to <pane>] [-- <command>...] | --stdio";
    let mut to = None;
    let mut command: &[&str] = &[];
    let mut i = 0;
    while i < args.len() {
        match args[i] {
            "--to" => {
                to = Some(*args.get(i + 1).context(usage)?);
                i += 2;
            }
            "--" => {
                command = &args[i + 1..];
                break;
            }
            _ => bail!("{usage}"),
        }
    }
    let (target, label) = if let Some(program) = command.first() {
        let cwd = std::env::current_dir()?;
        let mut rex_args: Vec<String> = [
            "run",
            "--split=above",
            "--ratio",
            "75",
            "--focus=false",
            "--json",
            "-s",
            &session,
            "-b",
            &me,
            "--label",
            program,
            "-c",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        rex_args.push(cwd.display().to_string());
        rex_args.push("--".into());
        rex_args.extend(command.iter().map(|s| s.to_string()));
        let made = rex_json(&rex_args)?;
        let id = made["block_ids"][0]
            .as_str()
            .context("rex didn't say which pane it made")?;
        (id.to_string(), program.to_string())
    } else {
        let listed = rex_json(&["block", "ls", "--json", "-s", &session].map(String::from))?;
        let blocks = listed["blocks"].as_array().cloned().unwrap_or_default();
        let field = |b: &serde_json::Value, k: &str| b[k].as_str().unwrap_or("").to_string();
        let window = blocks
            .iter()
            .find(|b| field(b, "block_id") == me)
            .map(|b| field(b, "window_id"));
        let named = |b: &serde_json::Value| {
            let l = field(b, "label");
            if l.is_empty() {
                field(b, "block_id")
            } else {
                l
            }
        };
        match to {
            Some(to) => {
                let label = blocks
                    .iter()
                    .find(|b| field(b, "block_id") == to || field(b, "block_id").ends_with(to))
                    .map_or(to.to_string(), named);
                (to.to_string(), label)
            }
            None => {
                let others: Vec<&serde_json::Value> = blocks
                    .iter()
                    .filter(|b| field(b, "block_id") != me)
                    .filter(|b| window.is_none() || Some(field(b, "window_id")) == window)
                    .collect();
                match others.as_slice() {
                    [one] => (field(one, "block_id"), named(one)),
                    [] => bail!(
                        "there's no other pane in this window to send to: start one with `very-normal compose -- claude`"
                    ),
                    many => {
                        let list: Vec<String> = many
                            .iter()
                            .map(|b| format!("  {}  {}", field(b, "block_id"), field(b, "label")))
                            .collect();
                        bail!("which pane? pick one with --to:\n{}", list.join("\n"))
                    }
                }
            }
        }
    };

    let progress = Progress::load(path)?;
    let stage = progress.stage;
    // ctrl-enter only reaches the app when the terminal reports modifiers on
    // enter (the kitty keyboard protocol, which Rex speaks)
    let enhanced = ratatui::crossterm::terminal::supports_keyboard_enhancement().unwrap_or(false);
    let send_key = if enhanced { "ctrl-enter" } else { "alt-enter" };
    let mut screen = Screen::new(
        Composer::new(compose::usual(&progress.freestyle)),
        label,
        send_key,
    );
    let mut saved = Vec::new();
    let mut terminal = ratatui::init();
    if enhanced {
        ratatui::crossterm::execute!(
            std::io::stdout(),
            event::PushKeyboardEnhancementFlags(
                event::KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES
            )
        )?;
    }
    let result = (|| -> Result<()> {
        loop {
            terminal.draw(|frame| screen.render(frame))?;
            if event::poll(Duration::from_millis(250))?
                && let Event::Key(key) = event::read()?
                && key.kind == KeyEventKind::Press
            {
                match screen.handle_key(key, Instant::now()) {
                    Action::None => {}
                    Action::Quit => break,
                    Action::Send(bytes) => {
                        if let Err(e) = rex_send(&target, &bytes) {
                            screen.problem = Some(format!("couldn't send: {e}"));
                        }
                    }
                }
            }
            if screen.composer.due(Instant::now()) {
                save_freestyle(&mut screen.composer, stage, path, &mut saved)?;
            }
        }
        Ok(())
    })();
    if enhanced {
        let _ = ratatui::crossterm::execute!(std::io::stdout(), event::PopKeyboardEnhancementFlags);
    }
    ratatui::restore();
    save_freestyle(&mut screen.composer, stage, path, &mut saved)?;
    result?;
    let secs: f64 = saved.iter().map(|r: &store::Run| r.secs).sum();
    if secs > 0.0 {
        let wpm = saved.iter().map(|r| r.wpm * r.secs).sum::<f64>() / secs;
        let corrected = saved.iter().map(|r| (100.0 - r.acc) * r.secs).sum::<f64>() / secs;
        println!(
            "Saved {:.0} min of real typing: {wpm:.0} WPM, {corrected:.0}% corrected.",
            (secs / 60.0).ceil()
        );
    }
    Ok(())
}

/// Compose for a window that does its own drawing (the Mac compose window):
/// a key as a line of JSON on stdin, what to show as a line of JSON on stdout.
/// Runs are saved as they fall due, and when stdin closes.
fn compose_stdio(path: &std::path::Path) -> Result<()> {
    let progress = Progress::load(path)?;
    let stage = progress.stage;
    let mut composer = Composer::new(compose::usual(&progress.freestyle));
    let mut saved = Vec::new();
    let (keys, lines) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        for line in std::io::stdin().lock().lines() {
            let Ok(line) = line else { break };
            if keys.send(line).is_err() {
                break;
            }
        }
    });
    let mut out = std::io::stdout().lock();
    let mut show = |state: &compose::State| -> Result<()> {
        writeln!(out, "{}", serde_json::to_string(state)?)?;
        out.flush()?;
        Ok(())
    };
    show(&composer.state())?;
    loop {
        match lines.recv_timeout(Duration::from_millis(250)) {
            Ok(line) if line.trim().is_empty() => {}
            Ok(line) => match serde_json::from_str::<compose::Input>(&line) {
                Ok(input) => show(&composer.apply(input, Instant::now()))?,
                Err(e) => eprintln!("very-normal compose: didn't understand {line:?}: {e}"),
            },
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
        }
        if composer.due(Instant::now()) {
            save_freestyle(&mut composer, stage, path, &mut saved)?;
        }
    }
    save_freestyle(&mut composer, stage, path, &mut saved)?;
    Ok(())
}

/// Saves the typing since the last save as a compose run, if there's enough.
fn save_freestyle(
    composer: &mut Composer,
    stage: usize,
    path: &std::path::Path,
    saved: &mut Vec<store::Run>,
) -> Result<()> {
    if let Some(run) = composer.take_run(stage, now_ms()?) {
        // load fresh, so a drill finished in another pane meanwhile is kept
        let mut p = Progress::load(path)?;
        p.record_freestyle(run.clone());
        p.save(path)?;
        saved.push(run);
    }
    Ok(())
}

fn rex_json(args: &[String]) -> Result<serde_json::Value> {
    let out = std::process::Command::new("rex")
        .args(args)
        .output()
        .context("running rex")?;
    if !out.status.success() {
        bail!(
            "rex {}: {}",
            args[0],
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    serde_json::from_slice(&out.stdout).context("reading what rex said")
}

fn rex_send(block: &str, bytes: &[u8]) -> Result<()> {
    let mut child = std::process::Command::new("rex")
        .args(["send", "-b", block])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .context("running rex")?;
    child
        .stdin
        .take()
        .context("rex's input")?
        .write_all(bytes)?;
    let out = child.wait_with_output()?;
    if !out.status.success() {
        bail!("{}", String::from_utf8_lossy(&out.stderr).trim());
    }
    Ok(())
}
