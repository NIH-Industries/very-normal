//! The TUI: the layout, warm-up, drill, report and help screens, and the dashboard of the whole plan.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, Paragraph, Wrap};

use crate::drill::{Drill, Mode, Rng, build};
use crate::gate;
use crate::journey;
use crate::layout;
use crate::music::Music;
use crate::store::{Progress, Run};
use crate::warmup::{self, Warmup};
use crate::{coach, oryx, report};

const PINK: Color = Color::Magenta;
const TEAL: Color = Color::Cyan;
/// A missed key: a soft lavender grey, a nudge rather than an alarm.
const MISS: Color = Color::Indexed(146);
/// Restarts this close together suggest frustration.
const ABANDON_WINDOW: std::time::Duration = std::time::Duration::from_secs(10 * 60);

/// Where you are in a session: the guided steps, then the dashboard.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    /// The layout on its own, before anything else.
    Layout,
    /// The breathing and stretches, every guided session.
    Warmup,
    /// Ten minutes of drills picked for you.
    Practice,
    /// How the session went, and how far off the next stage is.
    Report,
    /// Everything: stages, drills, the gate and Oryx.
    Dashboard,
}

/// Seconds of drill time in a guided practice block.
pub const BLOCK_SECS: f64 = 600.0;
/// A run under this accuracy gets a short breather after it. Two in a row
/// ease the block off to key recall, and two more after that end it early.
pub const BLOCK_ACCURACY: f64 = 95.0;
/// How long keys are ignored after a drill ends, so the letters you were still
/// typing when the clock ran out don't land on whatever screen comes next and
/// stop the music or quit.
pub const SETTLE: Duration = Duration::from_millis(1500);

/// The drill for the `i`th run of a practice block: real text while you're
/// fresh, then new keys and review in turn. On a plan that stays put, real
/// text and weak keys take turns.
pub fn practice_mode(i: usize) -> Mode {
    practice_mode_for(i, layout::staying())
}

fn practice_mode_for(i: usize, staying: bool) -> Mode {
    match i {
        i if staying && i % 2 == 0 => Mode::Real,
        _ if staying => Mode::Words,
        0 => Mode::Real,
        i if i % 2 == 1 => Mode::Words,
        _ => Mode::Review,
    }
}

/// A pause between drills, with something to do with your hands. Enter skips it.
#[derive(Debug, Clone, Copy)]
struct Rest {
    until: Instant,
    cue: &'static str,
}

impl Rest {
    /// After a run under [`BLOCK_ACCURACY`].
    fn rough(now: Instant) -> Rest {
        Rest {
            until: now + Duration::from_secs(20),
            cue: "Breathe out long and slow, and let your hands go soft.",
        }
    }

    /// Once the block passes five minutes.
    fn halfway(now: Instant) -> Rest {
        Rest {
            until: now + Duration::from_secs(30),
            cue: "Halfway. Shake your hands out and roll your shoulders back.",
        }
    }

    /// When two rough runs in a row ease the block off.
    fn ease(now: Instant) -> Rest {
        Rest {
            until: now + Duration::from_secs(45),
            cue: "Hands in your lap. Breathe in for 4, out for 6.",
        }
    }

    /// The seconds left, rounded up; 0 once it's over.
    fn left(&self, now: Instant) -> u64 {
        self.until
            .saturating_duration_since(now)
            .as_secs_f64()
            .ceil() as u64
    }
}

/// The runs of the practice block under way: their durations and accuracies.
#[derive(Debug, Default)]
struct PracticeBlock {
    started_ms: u64,
    /// Typing time and runs from earlier today, when this picks up a session.
    carried_secs: f64,
    carried_runs: usize,
    /// Runs since it started or was picked up.
    runs: Vec<(f64, f64)>,
    /// How many runs there were when two rough ones eased it off to key recall.
    eased_at: Option<usize>,
}

impl PracticeBlock {
    fn secs(&self) -> f64 {
        self.carried_secs + self.runs.iter().map(|(secs, _)| secs).sum::<f64>()
    }

    fn count(&self) -> usize {
        self.carried_runs + self.runs.len()
    }
}

pub struct App {
    pub progress: Progress,
    pub phase: Phase,
    path: PathBuf,
    /// The troublesome keys this drill's words lean on.
    leaning: Vec<char>,
    mode: Mode,
    drill: Drill,
    typing: bool,
    rng: Rng,
    /// The result of the last drill, or of the last thing you did.
    message: Option<(String, bool)>,
    /// The last `c` check against Oryx.
    check: Vec<String>,
    layout_id: String,
    /// Set when the coach said stop and you pressed enter anyway once.
    override_armed: bool,
    /// The warm-up, while it runs.
    warmup: Option<Warmup>,
    /// The guided practice block, while it runs.
    block: Option<PracticeBlock>,
    /// The end-of-session report, once practice is over.
    report: report::SessionReport,
    /// Background music, while it plays.
    music: Music,
    /// When you recently gave up on a drill with esc; several close together
    /// usually means frustration.
    abandons: Vec<Instant>,
    /// Keys before this are ignored: a drill just ended under your fingers.
    settle_until: Option<Instant>,
    /// The breather between drills, while it's on screen.
    rest: Option<Rest>,
    /// The accuracy of the last drill finished this time round.
    last_acc: Option<f64>,
    /// The help screen is open over whatever's showing.
    help: bool,
    /// How far the help screen is scrolled, and the most it can be at the last draw.
    help_scroll: u16,
    help_max_scroll: std::cell::Cell<u16>,
    pub should_quit: bool,
}

/// Each drill's own colour, the same on the practice screen, the help and the report.
pub fn mode_color(mode: Mode) -> Color {
    match mode {
        Mode::Real => Color::Yellow,
        Mode::Words => PINK,
        Mode::Review => TEAL,
        Mode::Recall => Color::Green,
        Mode::Links => Color::Blue,
        Mode::Mirror => Color::LightRed,
    }
}

/// `name` in a column `col` wide, indented two, with `text` wrapped beside it
/// in `width` columns.
fn hanging(name: &str, style: Style, col: usize, text: &str, width: usize) -> Vec<Line<'static>> {
    wrap(text, width.saturating_sub(2 + col))
        .into_iter()
        .enumerate()
        .map(|(i, l)| {
            let name = if i == 0 { name } else { "" };
            Line::from(vec![
                Span::styled(format!("  {name:col$}"), style),
                Span::raw(l),
            ])
        })
        .collect()
}

/// Words in lines of at most `width` columns (a longer word gets a line of its own).
fn wrap(text: &str, width: usize) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut line = String::new();
    for word in text.split_whitespace() {
        if !line.is_empty() && line.chars().count() + 1 + word.chars().count() > width {
            out.push(std::mem::take(&mut line));
        }
        if !line.is_empty() {
            line.push(' ');
        }
        line.push_str(word);
    }
    if !line.is_empty() {
        out.push(line);
    }
    out
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

impl App {
    pub fn new(progress: Progress, path: PathBuf, now: Instant, phase: Phase) -> App {
        let mut rng = Rng::new();
        let stage = progress.stage.min(layout::last());
        let leaning = crate::drill::leaning(Mode::Words, stage, &progress, false);
        let drill = Drill::new(
            Mode::Words,
            stage,
            build(Mode::Words, stage, &progress, false, &mut rng),
            now,
        );
        App {
            progress,
            phase,
            path,
            leaning,
            mode: Mode::Words,
            drill,
            typing: false,
            rng,
            message: None,
            check: Vec::new(),
            layout_id: oryx::layout_id(),
            override_armed: false,
            warmup: None,
            block: None,
            report: report::SessionReport::default(),
            music: Music::default(),
            abandons: Vec::new(),
            settle_until: None,
            rest: None,
            last_acc: None,
            help: false,
            help_scroll: 0,
            help_max_scroll: std::cell::Cell::new(0),
            should_quit: false,
        }
    }

    fn stage(&self) -> usize {
        self.progress.stage.min(layout::last())
    }

    fn save(&mut self) {
        if let Err(e) = self.progress.save(&self.path) {
            self.message = Some((format!("Couldn't save progress: {e}"), false));
        }
    }

    fn new_drill(&mut self, now: Instant) {
        // after a rough run, the next drill goes easy on the hard keys
        let gentle = self.last_acc.is_some_and(|a| a < BLOCK_ACCURACY);
        let text = build(
            self.mode,
            self.stage(),
            &self.progress,
            gentle,
            &mut self.rng,
        );
        self.leaning = crate::drill::leaning(self.mode, self.stage(), &self.progress, gentle);
        self.drill = Drill::new(self.mode, self.stage(), text, now);
    }

    fn set_stage(&mut self, stage: usize, now: Instant) {
        self.progress.stage = stage.min(layout::last());
        self.check.clear();
        self.message = None;
        self.save();
        self.new_drill(now);
    }

    /// Ends a timed drill once its minute is up.
    pub fn tick(&mut self, now: Instant) {
        if self.music.ended() {
            self.message = Some(("The music stream stopped.".into(), false));
        }
        if self.warmup.as_ref().is_some_and(|w| w.done(now)) {
            self.end_warmup(true, now);
        }
        if self.typing {
            self.drill.tick(now);
            if self.drill.done {
                self.finish(now);
            }
        }
    }

    fn end_warmup(&mut self, completed: bool, now: Instant) {
        self.warmup = None;
        if self.phase == Phase::Warmup {
            self.start_practice(now);
        }
        self.progress.warmed_up = Some(coach::today(now_ms()));
        self.save();
        let warm = if completed {
            "Warmed up. Fingers loose, shoulders down."
        } else {
            "Skipping the warm-up. That's fine; go gently for the first minute."
        };
        let next = self
            .resume_note()
            .unwrap_or_else(|| "Press enter to start.".into());
        self.message = Some((format!("{warm} {next}"), true));
    }

    fn toggle_music(&mut self) {
        if self.music.playing() {
            self.music.stop();
            self.message = None;
        } else {
            self.message = match self.music.start() {
                Ok(()) => None,
                Err(e) => Some((e, false)),
            };
        }
    }

    /// The music key, for the help lines that offer it.
    fn music_help(&self) -> &'static str {
        if self.music.playing() {
            "m stop music"
        } else {
            "m music"
        }
    }

    fn start_practice(&mut self, now: Instant) {
        self.phase = Phase::Practice;
        let block = match coach::unfinished(&self.progress.runs, now_ms()) {
            Some(u) => PracticeBlock {
                started_ms: u.started_ms,
                carried_secs: u.secs,
                carried_runs: u.runs,
                ..PracticeBlock::default()
            },
            None => PracticeBlock {
                started_ms: now_ms(),
                ..PracticeBlock::default()
            },
        };
        self.mode = practice_mode(block.count());
        self.block = Some(block);
        self.message = self.resume_note().map(|n| (n, true));
        self.new_drill(now);
    }

    /// Whether the coach lets a drill of your own start: when it says stop, the
    /// first enter explains and the second goes ahead anyway.
    fn coach_allows(&mut self) -> bool {
        let advice = self.advice();
        if advice.blocks() && !self.override_armed {
            self.override_armed = true;
            self.message = Some((
                format!(
                    "{} Press enter again to practise anyway, rebel.",
                    advice.message
                ),
                false,
            ));
            return false;
        }
        self.override_armed = false;
        true
    }

    /// Says so when practice picks up today's session instead of starting at 0:00.
    fn resume_note(&self) -> Option<String> {
        self.block.as_ref().filter(|b| b.carried_runs > 0)?;
        Some("Picking up today's session where you left off. Press enter to carry on.".into())
    }

    fn end_practice(&mut self, message: String, now: Instant) {
        let block = self.block.take();
        self.rest = None;
        self.phase = Phase::Dashboard;
        if let Some(b) = block.filter(|b| b.count() > 0) {
            self.phase = Phase::Report;
            self.report = report::session(&self.progress, self.stage(), b.started_ms);
        }
        self.message = Some((message, true));
        self.new_drill(now);
    }

    /// After a run in a practice block: the next drill, a breather, or the end of the block.
    fn practice_next(&mut self, secs: f64, wpm: f64, acc: f64, now: Instant) {
        let Some(block) = &mut self.block else {
            return;
        };
        let before = block.secs();
        block.runs.push((secs, acc));
        let since = block.eased_at.unwrap_or(0);
        let low: Vec<f64> = block.runs[since..]
            .iter()
            .rev()
            .take(2)
            .map(|(_, a)| *a)
            .collect();
        let two_low = low.len() == 2 && low.iter().all(|a| *a < BLOCK_ACCURACY);
        let (eased, after, count) = (block.eased_at.is_some(), block.secs(), block.count());
        if two_low && eased {
            return self.end_practice(
                format!(
                    "{:.1}% and then {:.1}%. That's the sign to stop for now. No rush: it'll come together after a rest.",
                    low[1], low[0]
                ),
                now,
            );
        }
        if after >= BLOCK_SECS {
            return self.end_practice(
                "That's ten minutes. Practice done for this session; let it settle.".into(),
                now,
            );
        }
        if two_low {
            block.eased_at = Some(block.runs.len());
            self.mode = Mode::Recall;
            self.new_drill(now);
            self.rest = Some(Rest::ease(now));
            self.message = Some((
                format!(
                    "{:.1}% and then {:.1}%. Let's go easy: next is key recall, one key at a time, as slow as you like.",
                    low[1], low[0]
                ),
                true,
            ));
            return;
        }
        let half = BLOCK_SECS / 2.0;
        if before < half && after >= half {
            self.rest = Some(Rest::halfway(now));
        }
        self.mode = practice_mode(count);
        self.new_drill(now);
        let result = format!("{wpm:.0} WPM · {acc:.1}% accuracy.");
        self.message = Some((
            if self.rest.is_some() {
                result
            } else {
                format!("{result} Press enter for the next drill.")
            },
            true,
        ));
    }

    fn finish(&mut self, now: Instant) {
        self.typing = false;
        self.settle_until = Some(now + SETTLE);
        let (stage, mode) = (self.drill.stage, self.drill.mode);
        // round once, so the number shown, the number saved and the number judged agree
        let wpm = (self.drill.wpm(now) * 10.0).round() / 10.0;
        let acc = (self.drill.accuracy() * 10.0).round() / 10.0;
        let previous_best = self
            .progress
            .runs_for(stage, mode.id())
            .iter()
            .map(|r| r.wpm)
            .fold(None, |best: Option<f64>, w| {
                Some(best.map_or(w, |b| b.max(w)))
            });
        let t = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        let run = Run {
            t,
            stage,
            mode: mode.id().into(),
            wpm,
            acc,
            secs: (self.drill.elapsed(now).as_secs_f64() * 10.0).round() / 10.0,
            letters: self.drill.letters().clone(),
            typos: self.drill.typos().clone(),
        };
        self.progress.record(run, self.drill.pairs());
        self.save();

        let mut good = true;
        let verdict = match mode {
            Mode::Words if stage > 0 => {
                good = acc >= gate::WORDS_ACCURACY;
                let hits = gate::words_hits(&self.progress, stage).min(gate::WORDS_NEEDED);
                if good && hits >= gate::WORDS_NEEDED {
                    "That one counts, and that's the accuracy part of the gate done!".to_string()
                } else if good {
                    format!("That one counts: {hits} of {}.", gate::WORDS_NEEDED)
                } else {
                    "Not one for the gate this time. Every rep still teaches your fingers; slow and smooth wins.".to_string()
                }
            }
            Mode::Real if stage == 0 => "Saved toward your baseline.".to_string(),
            Mode::Real => match self.progress.baseline() {
                Some(b) => {
                    let target = b * gate::SPEED_SHARE;
                    good = wpm >= target && acc >= gate::REAL_ACCURACY;
                    format!("Target is {target:.0} WPM at 95%+.")
                }
                None => "Record a baseline in stage 0 to get a target.".to_string(),
            },
            Mode::Recall => {
                let slow: Vec<String> = self
                    .drill
                    .slowest(3)
                    .iter()
                    .map(|(c, ms, miss)| {
                        if *miss > 0 {
                            format!("{c} {ms:.0}ms, {miss} missed")
                        } else {
                            format!("{c} {ms:.0}ms")
                        }
                    })
                    .collect();
                format!("Slowest: {}", slow.join(" · "))
            }
            _ => String::new(),
        };
        let record = match previous_best {
            Some(best) if wpm > best && acc >= 90.0 => " New speed record for this stage!",
            _ => "",
        };
        self.message = Some((
            format!("{wpm:.0} WPM · {acc:.1}% accuracy. {verdict}{record}"),
            good,
        ));
        self.rest = (acc < BLOCK_ACCURACY).then(|| Rest::rough(now));
        self.last_acc = Some(acc);
        if self.phase == Phase::Practice {
            let secs = (self.drill.elapsed(now).as_secs_f64() * 10.0).round() / 10.0;
            self.practice_next(secs, wpm, acc, now);
        }
    }

    pub fn handle_key(&mut self, key: KeyEvent, now: Instant) {
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            self.should_quit = true;
            return;
        }
        if self.settle_until.is_some_and(|t| now < t) {
            return;
        }
        if self.help {
            match key.code {
                KeyCode::Down | KeyCode::Char('j') => {
                    self.help_scroll = (self.help_scroll + 1).min(self.help_max_scroll.get())
                }
                KeyCode::Up | KeyCode::Char('k') => {
                    self.help_scroll = self.help_scroll.saturating_sub(1)
                }
                _ => self.help = false,
            }
            return;
        }
        if key.code == KeyCode::Char('?') && !self.typing {
            self.help = true;
            self.help_scroll = 0;
            return;
        }
        if key.code == KeyCode::Char('m')
            && matches!(self.phase, Phase::Layout | Phase::Warmup | Phase::Report)
        {
            self.toggle_music();
            return;
        }
        if self.phase == Phase::Layout {
            match key.code {
                KeyCode::Enter => {
                    self.phase = Phase::Warmup;
                    self.warmup = Some(Warmup::new(now));
                    self.message = None;
                }
                KeyCode::Char('d') => self.phase = Phase::Dashboard,
                KeyCode::Char('q') => self.should_quit = true,
                _ => {}
            }
            return;
        }
        if let Some(w) = &mut self.warmup {
            match key.code {
                KeyCode::Enter | KeyCode::Tab | KeyCode::Char(' ') => {
                    w.skip_step(now);
                    if w.done(now) {
                        self.end_warmup(true, now);
                    }
                }
                KeyCode::Char('s') => self.end_warmup(false, now),
                KeyCode::Esc if self.phase == Phase::Warmup => {
                    self.warmup = None;
                    self.start_practice(now);
                }
                KeyCode::Esc => {
                    self.warmup = None;
                    self.message = Some((
                        "Warm-up paused. Press enter when you're ready.".into(),
                        true,
                    ));
                }
                _ => {}
            }
            return;
        }
        if self.typing {
            match key.code {
                KeyCode::Esc => {
                    self.typing = false;
                    self.abandons
                        .retain(|t| now.duration_since(*t) < ABANDON_WINDOW);
                    self.abandons.push(now);
                    self.message = Some(if self.abandons.len() >= 3 {
                        (
                            "That's a few restarts in a row. Frustration makes fingers tense, and tense fingers miss. Take five and come back loose.".into(),
                            false,
                        )
                    } else {
                        ("Stopped. Nothing was saved.".into(), true)
                    });
                    self.new_drill(now);
                }
                KeyCode::Char(c)
                    if !key
                        .modifiers
                        .intersects(KeyModifiers::ALT | KeyModifiers::SUPER) =>
                {
                    self.drill.press(c, now);
                    if self.drill.done {
                        self.finish(now);
                    }
                }
                _ => {}
            }
            return;
        }
        if self.phase == Phase::Report {
            match key.code {
                KeyCode::Enter => {
                    self.phase = Phase::Dashboard;
                    self.message = None;
                }
                KeyCode::Char('q') => self.should_quit = true,
                _ => {}
            }
            return;
        }
        if self.phase == Phase::Practice {
            match key.code {
                KeyCode::Enter | KeyCode::Tab | KeyCode::Char(' ') => {
                    // a guided session is never stopped mid-way; a drill of your own can be
                    if self.block.is_none() && !self.coach_allows() {
                        return;
                    }
                    if self.drill.done || self.drill.started() {
                        self.new_drill(now);
                    }
                    self.rest = None;
                    self.message = None;
                    self.drill.touch(now);
                    self.typing = true;
                }
                KeyCode::Esc if self.block.is_none() => {
                    self.phase = Phase::Dashboard;
                    self.rest = None;
                    self.message = None;
                }
                KeyCode::Esc => {
                    self.end_practice("Practice stopped. Come back whenever you like.".into(), now)
                }
                KeyCode::Char('q') => self.should_quit = true,
                _ => {}
            }
            return;
        }
        let stage = self.stage();
        match key.code {
            KeyCode::Char('q') => self.should_quit = true,
            KeyCode::Left | KeyCode::Char('[') if stage > 0 => self.set_stage(stage - 1, now),
            KeyCode::Right | KeyCode::Char(']') if stage < layout::last() => {
                self.set_stage(stage + 1, now)
            }
            KeyCode::Char(d @ '1'..='6') => {
                self.mode = Mode::ALL[d as usize - '1' as usize];
                self.block = None;
                self.phase = Phase::Practice;
                self.message = None;
                self.new_drill(now);
            }
            KeyCode::Enter => {
                self.phase = Phase::Layout;
                self.message = None;
            }
            KeyCode::Char('c' | 'o') if layout::staying() => {}
            KeyCode::Char('c') => {
                self.check = match oryx::fetch(&self.layout_id) {
                    Ok(snap) => report::check(&snap, stage),
                    Err(e) => vec![format!("Couldn't read Oryx: {e}")],
                };
            }
            KeyCode::Char('g') => {
                let out = crate::charts::default_path(&self.path);
                let now_ms = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_millis() as u64)
                    .unwrap_or(0);
                self.message = Some(
                    match crate::charts::write(&self.progress, &out, now_ms)
                        .and_then(|()| crate::open_url(&out.display().to_string()))
                    {
                        Ok(()) => (format!("Charts opened from {}", out.display()), true),
                        Err(e) => (format!("Couldn't make the charts: {e}"), false),
                    },
                );
            }
            KeyCode::Char('o') => {
                let url = oryx::edit_url(&self.layout_id);
                self.message = Some(match crate::open_url(&url) {
                    Ok(()) => (format!("Opened {url}"), true),
                    Err(e) => (format!("Couldn't open {url}: {e}"), false),
                });
            }
            KeyCode::Char(c) => {
                if let Some(tick) = gate::items(&self.progress, stage)
                    .iter()
                    .filter_map(|i| i.tick)
                    .find(|t| t.key() == c)
                {
                    tick.toggle(&mut self.progress, stage);
                    self.save();
                }
            }
            _ => {}
        }
    }

    pub fn render(&self, frame: &mut Frame, now: Instant) {
        self.render_screen(frame, now);
        if self.help {
            self.render_help(frame);
        }
    }

    /// The help screen's lines at `width` columns: sections with room around
    /// them, and names in a column of their own.
    fn help_lines(&self, width: usize) -> Vec<Line<'static>> {
        let stage = self.stage();
        let head = |t: String| Line::styled(t, Style::new().fg(PINK).add_modifier(Modifier::BOLD));
        let para = |lines: &mut Vec<Line<'static>>, text: &str| {
            for l in wrap(text, width.saturating_sub(2)) {
                lines.push(Line::raw(format!("  {l}")));
            }
            lines.push(Line::raw(""));
        };
        // a name in its own column, its text wrapped beside it
        let bold = Style::new().add_modifier(Modifier::BOLD);
        let mut lines = vec![
            Line::raw(""),
            head("The guided session".into()),
            Line::raw(""),
        ];
        para(
            &mut lines,
            if layout::staying() {
                "Layout, warm-up, ten minutes of practice, then a report. Practice starts with real text, then weak keys and real text take turns."
            } else {
                "Layout, warm-up, ten minutes of practice, then a report. Practice starts with real text, then takes new keys and review in turn."
            },
        );
        para(
            &mut lines,
            &format!(
                "The clock counts typing time only, so it moves between drills, not while you rest. A run under {BLOCK_ACCURACY:.0}% gets a short breather after it, and so does the halfway mark; enter skips either. Two runs in a row under {BLOCK_ACCURACY:.0}% ease off to key recall with a longer one, and two more after that end practice for now. Otherwise it ends after the drill that passes 10:00. Quit before 10:00 and the next practice today picks up where you left off."
            ),
        );
        lines.push(head(format!("The drills, at stage {stage}")));
        lines.push(Line::raw(""));
        let col = Mode::ALL
            .iter()
            .map(|m| m.label().chars().count())
            .max()
            .unwrap_or(0)
            + 3;
        for m in Mode::ALL {
            lines.extend(hanging(
                m.label(),
                bold.fg(mode_color(m)),
                col,
                &m.describe(stage),
                width,
            ));
            lines.push(Line::raw(""));
        }
        lines.push(head("Music".into()));
        lines.push(Line::raw(""));
        para(
            &mut lines,
            &format!(
                "m plays {} on the layout, warm-up and report screens. Set VERY_NORMAL_MUSIC to any stream URL or file to play something else.",
                crate::music::source()
            ),
        );
        lines.push(head("Keys".into()));
        lines.push(Line::raw(""));
        for (k, what) in [
            ("enter", "start a drill; on the dashboard, today's session"),
            (
                "1-6",
                "on the dashboard, a drill of your own: no clock, esc comes back",
            ),
            (
                "esc",
                "stop a drill without saving, or end practice between drills",
            ),
            ("m", "music, on the layout, warm-up and report"),
            ("q", "quit, between drills"),
            ("ctrl-c", "quit, anywhere"),
            ("?", "this help, anywhere but mid-drill"),
        ] {
            lines.extend(hanging(k, bold, 9, what, width));
        }
        lines
    }

    /// What the session is, what each drill is for, and the keys, over the current screen.
    fn render_help(&self, frame: &mut Frame) {
        let area = frame.area();
        let width = area.width.min(84);
        let rect = Rect::new(
            area.x + (area.width - width) / 2,
            area.y,
            width,
            area.height,
        );
        let lines = self.help_lines(width.saturating_sub(4) as usize);
        let inner = rect.height.saturating_sub(2);
        let max = (lines.len() as u16).saturating_sub(inner);
        self.help_max_scroll.set(max);
        let title = if max > 0 {
            " help · ↑/↓ scroll · any other key closes "
        } else {
            " help · any key closes "
        };
        frame.render_widget(Clear, rect);
        frame.render_widget(
            Paragraph::new(lines)
                .block(
                    Block::bordered()
                        .border_style(Style::new().fg(PINK))
                        .title(title)
                        .padding(ratatui::widgets::Padding::horizontal(1)),
                )
                .scroll((self.help_scroll.min(max), 0)),
            rect,
        );
    }

    fn render_screen(&self, frame: &mut Frame, now: Instant) {
        match self.phase {
            Phase::Layout => return self.render_layout(frame, now),
            Phase::Warmup => {
                let [main, footer] = Layout::vertical([Constraint::Fill(1), Constraint::Length(1)])
                    .areas(frame.area());
                if let Some(w) = &self.warmup {
                    self.render_warmup(frame, main, w, now);
                }
                frame.render_widget(
                    Paragraph::new(format!(
                        "enter next step · s skip the warm-up · {} · esc start practising · ? help",
                        self.music_help()
                    ))
                    .style(dim()),
                    footer,
                );
                return;
            }
            Phase::Practice => return self.render_practice(frame, now),
            Phase::Report => return self.render_report(frame),
            Phase::Dashboard if layout::staying() => return self.render_staying(frame),
            Phase::Dashboard => {}
        }
        let s = self.stage();
        let journey = journey::journey(&self.progress, now_ms());
        let passed = gate::passed(&self.progress, s);
        // the gate, or once it's passed the edits for the next stage
        let lower = if passed {
            report::next_stage_compact(s, "").len()
                + if self.check.is_empty() {
                    0
                } else {
                    self.check.len() + 1
                }
        } else {
            gate::items(&self.progress, s)
                .iter()
                .map(|i| 1 + usize::from(!i.detail.is_empty()))
                .sum::<usize>()
        } + 2;
        let real_use = report::real_use(&self.progress, now_ms());
        let [
            header,
            _,
            journey_area,
            lower_area,
            real_area,
            message,
            footer,
        ] = Layout::vertical([
            Constraint::Length(2),
            Constraint::Length(1),
            Constraint::Fill(1),
            Constraint::Length(lower as u16),
            Constraint::Length(u16::from(real_use.is_some())),
            Constraint::Length(u16::from(self.message.is_some())),
            Constraint::Length(1),
        ])
        .areas(frame.area());
        if let Some(line) = real_use {
            frame.render_widget(Paragraph::new(line).style(dim()), real_area);
        }
        self.render_header(frame, header, &journey);
        self.render_journey(frame, journey_area, &journey);
        if passed && s < layout::last() {
            self.render_next(frame, lower_area);
        } else {
            self.render_gate(frame, lower_area);
        }
        if let Some((msg, good)) = &self.message {
            let color = if *good { Color::Green } else { Color::Yellow };
            frame.render_widget(
                Paragraph::new(msg.clone()).style(Style::new().fg(color)),
                message,
            );
        }
        frame.render_widget(
            Paragraph::new(
                "enter session · 1-6 drill · ←/→ stage · g charts · o/c oryx · ? help · q quit",
            )
            .style(dim()),
            footer,
        );
        let _ = now;
    }

    /// The dashboard for a plan that stays on the layout you have: how you're
    /// getting on, with no stages, gate or Oryx.
    fn render_staying(&self, frame: &mut Frame) {
        let [header, _, main, message, footer] = Layout::vertical([
            Constraint::Length(2),
            Constraint::Length(1),
            Constraint::Fill(1),
            Constraint::Length(u16::from(self.message.is_some())),
            Constraint::Length(1),
        ])
        .areas(frame.area());
        frame.render_widget(
            Paragraph::new(vec![
                Line::from(
                    [
                        Span::styled(
                            format!("{}  ", layout::plan().name.to_lowercase()),
                            Style::new().fg(PINK).add_modifier(Modifier::BOLD),
                        ),
                        Span::styled("no new layout", Style::new().add_modifier(Modifier::BOLD)),
                    ]
                    .into_iter()
                    .chain(profile_span())
                    .collect::<Vec<_>>(),
                ),
                Line::styled("Just getting better at the one you have.", dim()),
            ]),
            header,
        );
        let mut lines = vec![Line::raw("")];
        for (i, section) in staying_lines(
            &report::standing(&self.progress),
            &self.advice(),
            report::real_use(&self.progress, now_ms()),
        )
        .into_iter()
        .enumerate()
        {
            if i > 0 {
                lines.push(Line::raw(""));
            }
            let (title, body) = section;
            lines.push(Line::styled(
                title.to_string(),
                Style::new().fg(PINK).add_modifier(Modifier::BOLD),
            ));
            lines.push(Line::raw(""));
            for l in body {
                lines.push(Line::raw(format!("  {l}")));
            }
        }
        frame.render_widget(
            Paragraph::new(lines)
                .block(
                    Block::bordered()
                        .border_style(Style::new().fg(TEAL))
                        .title(" how you're getting on ")
                        .padding(ratatui::widgets::Padding::horizontal(1)),
                )
                .wrap(Wrap { trim: false }),
            main,
        );
        if let Some((msg, good)) = &self.message {
            let color = if *good { Color::Green } else { Color::Yellow };
            frame.render_widget(
                Paragraph::new(msg.clone()).style(Style::new().fg(color)),
                message,
            );
        }
        frame.render_widget(
            Paragraph::new("enter session · 1-6 drill · g charts · ? help · q quit").style(dim()),
            footer,
        );
    }

    /// The end of a guided session.
    fn render_report(&self, frame: &mut Frame) {
        let [main, footer] =
            Layout::vertical([Constraint::Fill(1), Constraint::Length(1)]).areas(frame.area());
        let width = main.width.saturating_sub(4) as usize;
        let r = &self.report;
        let head = |t: &str| {
            Line::styled(
                t.to_string(),
                Style::new().fg(PINK).add_modifier(Modifier::BOLD),
            )
        };
        let bold = Style::new().add_modifier(Modifier::BOLD);
        let mut lines = vec![Line::raw("")];
        if let Some((msg, _)) = &self.message {
            for l in wrap(msg, width) {
                lines.push(Line::styled(l, Style::new().fg(Color::Green)));
            }
        }
        let advice = self.advice();
        lines.push(Line::styled(
            format!(
                "Today: {} of {} sessions.",
                advice.sessions_today,
                coach::SESSIONS_PER_DAY
            ),
            dim(),
        ));
        lines.push(Line::raw(""));
        if !r.drills.is_empty() {
            lines.push(head("Drills"));
            lines.push(Line::raw(""));
            let col = r
                .drills
                .iter()
                .map(|d| d.mode.label().chars().count())
                .max()
                .unwrap_or(0)
                + 3;
            for d in &r.drills {
                let mut first = hanging(
                    d.mode.label(),
                    bold.fg(mode_color(d.mode)),
                    col,
                    &d.stats(),
                    width,
                );
                lines.append(&mut first);
                lines.push(Line::styled(format!("  {:col$}{}", "", d.compared), dim()));
            }
            lines.push(Line::raw(""));
        }
        if !r.slowest.is_empty() {
            lines.push(head("Slowest keys"));
            lines.push(Line::raw(""));
            for k in &r.slowest {
                lines.push(Line::raw(format!("  {k}")));
            }
            lines.push(Line::raw(""));
        }
        if !layout::staying() || !r.to_come.is_empty() {
            lines.push(head(if layout::staying() {
                "Still to do"
            } else {
                "Before the next stage"
            }));
            lines.push(Line::raw(""));
            if r.to_come.is_empty() {
                lines.push(Line::raw("  Every gate item for this stage is done."));
            }
            for item in &r.to_come {
                lines.extend(hanging("·", Style::new(), 2, item, width));
            }
            lines.push(Line::raw(""));
        }
        if let Some(t) = &r.trend {
            lines.push(head("Real text"));
            lines.push(Line::raw(""));
            for l in wrap(t, width.saturating_sub(2)) {
                lines.push(Line::raw(format!("  {l}")));
            }
        }
        frame.render_widget(
            Paragraph::new(lines).block(
                Block::bordered()
                    .border_style(Style::new().fg(TEAL))
                    .title(" this session ")
                    .padding(ratatui::widgets::Padding::horizontal(1)),
            ),
            main,
        );
        frame.render_widget(
            Paragraph::new(format!(
                "enter dashboard · {} · ? help · q quit",
                self.music_help()
            ))
            .style(dim()),
            footer,
        );
    }

    /// The practice block: the map, the text and the time left. The map is always there and
    /// never changes with a pause, so nothing on the screen moves while you type.
    fn render_practice(&self, frame: &mut Frame, now: Instant) {
        let [keyboard, main, footer] = Layout::vertical([
            Constraint::Length(5),
            Constraint::Fill(1),
            Constraint::Length(1),
        ])
        .areas(frame.area());
        self.render_keyboard(frame, keyboard, now, false);
        let done = self.block.as_ref().map_or(0.0, PracticeBlock::secs);
        let done = done.min(BLOCK_SECS) as u64;
        let total = BLOCK_SECS as u64;
        let mut lines = vec![
            Line::from(vec![
                Span::styled(
                    self.mode.label().to_string(),
                    Style::new()
                        .fg(mode_color(self.mode))
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    if self.block.is_some() {
                        format!(
                            "   {}:{:02} of {}:{:02} done",
                            done / 60,
                            done % 60,
                            total / 60,
                            total % 60
                        )
                    } else {
                        String::new()
                    },
                    dim(),
                ),
            ]),
            Line::raw(""),
        ];
        lines.extend(self.text_lines());
        lines.push(Line::raw(""));
        match &self.message {
            Some((msg, _)) if !self.typing => {
                lines.push(Line::styled(msg.clone(), Style::new().fg(Color::Green)))
            }
            _ if !self.typing => lines.push(Line::styled("Press enter to start.", dim())),
            _ => {}
        }
        if let Some(rest) = self.rest.filter(|_| !self.typing) {
            let left = rest.left(now);
            let line = if left > 0 {
                format!("{}  {}:{:02} · enter skips", rest.cue, left / 60, left % 60)
            } else {
                "Ready when you are. Press enter to go on.".into()
            };
            lines.push(Line::styled(line, Style::new().fg(TEAL)));
        }
        let border = if self.typing {
            Style::new().fg(PINK)
        } else {
            Style::new()
        };
        frame.render_widget(
            Paragraph::new(lines)
                .block(Block::bordered().border_style(border).title(" practice "))
                .wrap(Wrap { trim: false }),
            main,
        );
        let help = if self.typing {
            "type the text · esc stops this drill without saving"
        } else {
            if self.block.is_some() {
                "enter start · esc end practice · ? help · q quit"
            } else {
                "enter start · esc back to the dashboard · ? help · q quit"
            }
        };
        frame.render_widget(Paragraph::new(help).style(dim()), footer);
    }

    /// The guided session's first screen: just the layout and what's new in it.
    fn render_layout(&self, frame: &mut Frame, now: Instant) {
        let s = self.stage();
        let [title, _, keyboard, _, new_keys, scene, message, _, footer] = Layout::vertical([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(5),
            Constraint::Length(1),
            Constraint::Length(2),
            Constraint::Length(2),
            Constraint::Length(2),
            Constraint::Fill(1),
            Constraint::Length(1),
        ])
        .areas(frame.area());
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(
                    format!("{}  ", layout::plan().name.to_lowercase()),
                    Style::new().fg(PINK).add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    if layout::staying() {
                        "no new layout".to_string()
                    } else {
                        format!("stage {s} of {} · {}", layout::last(), layout::name(s))
                    },
                    Style::new().add_modifier(Modifier::BOLD),
                ),
            ])),
            title,
        );
        self.render_keyboard(frame, keyboard, now, true);
        let moved: Vec<String> = layout::changes(s)
            .iter()
            .map(|c| c.to.to_string())
            .collect();
        let text = if layout::staying() {
            "Nothing moves: this is the layout you type on. Settle in and type.".to_string()
        } else if moved.is_empty() {
            "Nothing moves this stage. Settle in and type.".to_string()
        } else {
            format!(
                "Moving this stage: {}. Take a look at where they live.",
                moved.join(", ")
            )
        };
        frame.render_widget(
            Paragraph::new(text)
                .style(Style::new().fg(PINK))
                .wrap(Wrap { trim: true }),
            new_keys,
        );
        frame.render_widget(
            Paragraph::new(
                "Find a quiet place where you won't be interrupted. Headphones on, something gentle playing. There's no rush.",
            )
            .style(Style::new().fg(TEAL))
            .wrap(Wrap { trim: true }),
            scene,
        );
        if let Some((msg, _)) = &self.message {
            frame.render_widget(
                Paragraph::new(msg.clone())
                    .style(Style::new().fg(Color::Yellow))
                    .wrap(Wrap { trim: true }),
                message,
            );
        }
        frame.render_widget(
            Paragraph::new(format!(
                "enter warm up · {} · d dashboard · ? help · q quit",
                self.music_help()
            ))
            .style(dim()),
            footer,
        );
    }

    fn render_header(&self, frame: &mut Frame, area: Rect, journey: &[journey::StageLine]) {
        let s = self.stage();
        let mut title = Line::from(vec![
            Span::styled(
                format!("{}  ", layout::plan().name.to_lowercase()),
                Style::new().fg(PINK).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("stage {s} of {} · {}", layout::last(), layout::name(s)),
                Style::new().add_modifier(Modifier::BOLD),
            ),
        ]);
        title.spans.extend(profile_span());
        let current_left = journey.iter().find_map(|l| match l.status {
            journey::Status::Current { left, .. } => Some(left),
            _ => None,
        });
        let ahead_known = journey
            .iter()
            .any(|l| matches!(l.status, journey::Status::Ahead(Some(_))));
        let outlook = match (journey::remaining(journey), current_left.flatten()) {
            (Some(days), _) => format!(
                "At your recent pace, the rest of the plan is likely {}.",
                journey::vague(days)
            ),
            (None, Some(_)) => {
                "Stages ahead get a time guess once this one is behind you, from your own pace."
                    .to_string()
            }
            (None, None) => format!(
                "Time guesses come after {} real-text sessions at 95%+{}.",
                report::TREND_SESSIONS,
                if ahead_known {
                    ""
                } else {
                    ", and once a stage is done"
                }
            ),
        };
        frame.render_widget(
            Paragraph::new(vec![title, Line::styled(outlook, dim())]),
            area,
        );
    }

    /// Every stage of the plan: what it moves, how long the finished ones took,
    /// and a rough guess at the rest once there's data for one.
    fn render_journey(&self, frame: &mut Frame, area: Rect, journey: &[journey::StageLine]) {
        let name_w = journey
            .iter()
            .map(|l| l.name.chars().count())
            .max()
            .unwrap_or(0);
        let keys = |l: &journey::StageLine| {
            l.keys
                .iter()
                .map(char::to_string)
                .collect::<Vec<_>>()
                .join(" ")
        };
        let keys_w = journey
            .iter()
            .map(|l| keys(l).chars().count())
            .max()
            .unwrap_or(0);
        let top = journey.iter().map(|l| l.share).fold(0.0, f64::max).max(1.0);
        let baseline = self.progress.baseline();
        let mut lines = Vec::new();
        for l in journey {
            let (mark, style) = match l.status {
                journey::Status::Done(_) => ("✓", Style::new().fg(Color::Green)),
                journey::Status::Current { .. } => {
                    ("▶", Style::new().fg(PINK).add_modifier(Modifier::BOLD))
                }
                journey::Status::Ahead(_) => (" ", Style::new()),
            };
            let bar = if l.share > 0.0 {
                let n = ((l.share / top) * 10.0).round().max(1.0) as usize;
                format!("{:<10} {:>3.0}%", "█".repeat(n), l.share)
            } else {
                " ".repeat(15)
            };
            let when = match &l.status {
                journey::Status::Done(_) if l.stage == 0 => {
                    baseline.map_or("done".to_string(), |b| format!("qwerty at {b:.0} WPM"))
                }
                journey::Status::Done(Some((d, m))) => journey::spent(*d, *m),
                journey::Status::Done(None) => "done".to_string(),
                journey::Status::Current {
                    days,
                    minutes,
                    left,
                } => {
                    let so_far = format!("{} so far", journey::spent(*days, *minutes));
                    match left {
                        Some(d) => format!("{so_far} · likely {} more", journey::vague(*d)),
                        None => so_far,
                    }
                }
                journey::Status::Ahead(Some(d)) => format!("~{}", journey::vague(*d)),
                journey::Status::Ahead(None) => String::new(),
            };
            let name_style = if matches!(l.status, journey::Status::Ahead(_)) {
                dim()
            } else {
                style
            };
            lines.push(Line::from(vec![
                Span::styled(format!(" {mark} {:>2} ", l.stage), style),
                Span::styled(format!("{:<name_w$}  ", l.name), name_style),
                Span::styled(format!("{:<keys_w$}  ", keys(l)), Style::new().fg(TEAL)),
                Span::styled(format!("{bar}   "), Style::new().fg(PINK)),
                Span::styled(when, dim()),
            ]));
        }
        frame.render_widget(
            Paragraph::new(lines).block(Block::bordered().title(" the plan ").title_bottom(
                Line::styled(" bars: how much of your typing each stage moves ", dim()),
            )),
            area,
        );
    }

    pub fn advice(&self) -> coach::Advice {
        coach::assess(&self.progress.runs, now_ms())
    }

    /// The map of the current stage. With `hints`, the key you want lights up when you pause.
    fn render_keyboard(&self, frame: &mut Frame, area: Rect, now: Instant, hints: bool) {
        let s = self.stage();
        let keys = layout::keys(s);
        let mirror_mode = self.mode == Mode::Mirror;
        let hint = if hints && self.typing && self.drill.hesitating(now) {
            self.drill.expected()
        } else {
            None
        };
        let hinted = |index: usize| -> bool {
            let Some(h) = hint else { return false };
            if mirror_mode {
                index % 10 < 5 && (keys[index] == h || keys[layout::mirror(index)] == h)
            } else {
                keys[index] == h
            }
        };
        let mut lines = Vec::new();
        for row in 0..3 {
            let mut spans = vec![Span::styled(
                format!("{:>4} ", layout::OUTER_LEFT[row]),
                dim(),
            )];
            for col in 0..10 {
                let i = row * 10 + col;
                if col == 5 {
                    spans.push(Span::raw("     "));
                }
                let mut style = if layout::changed_now(s, i) {
                    Style::new().fg(PINK).add_modifier(Modifier::BOLD)
                } else if layout::moved(s, i) {
                    Style::new().fg(TEAL)
                } else {
                    Style::new()
                };
                if hinted(i) {
                    style = Style::new()
                        .fg(Color::Black)
                        .bg(Color::Yellow)
                        .add_modifier(Modifier::BOLD);
                }
                if mirror_mode && col < 5 {
                    spans.push(Span::styled(format!(" {}", keys[i]), style));
                    spans.push(Span::styled(
                        format!("{} ", keys[layout::mirror(i)]),
                        Style::new().fg(PINK),
                    ));
                } else {
                    spans.push(Span::styled(format!(" {} ", keys[i]), style));
                }
            }
            spans.push(Span::styled(
                format!(" {}", layout::outer_right(row)),
                dim(),
            ));
            lines.push(Line::from(spans));
        }
        let legend = if layout::staying() {
            Line::styled("yellow: the key you're looking for", dim())
        } else {
            Line::from(vec![
                Span::styled("pink", Style::new().fg(PINK)),
                Span::styled(" moves this stage · ", dim()),
                Span::styled("teal", Style::new().fg(TEAL)),
                Span::styled(" moved earlier · yellow: the key you're looking for", dim()),
            ])
        };
        let block = Block::bordered().title(" voyager ").title_bottom(legend);
        frame.render_widget(Paragraph::new(lines).block(block), area);
    }

    fn render_next(&self, frame: &mut Frame, area: Rect) {
        let s = self.stage();
        let url = oryx::edit_url(&self.layout_id);
        let mut lines: Vec<Line> = report::next_stage_compact(s, &url)
            .into_iter()
            .map(Line::from)
            .collect();
        if !self.check.is_empty() {
            lines.push(Line::raw(""));
            lines.extend(
                self.check
                    .iter()
                    .map(|l| Line::styled(l.clone(), Style::new().fg(TEAL))),
            );
        }
        let block = Block::bordered().title(format!(
            " gate passed: oryx edits for stage {} (o open · c check) ",
            (s + 1).min(layout::last())
        ));
        frame.render_widget(
            Paragraph::new(lines)
                .block(block)
                .wrap(Wrap { trim: false }),
            area,
        );
    }

    fn render_gate(&self, frame: &mut Frame, area: Rect) {
        let s = self.stage();
        let mut lines = Vec::new();
        for item in gate::items(&self.progress, s) {
            let mark = if item.ok {
                Span::styled("[✓] ", Style::new().fg(Color::Green))
            } else {
                Span::styled("[ ] ", dim())
            };
            let key = item
                .tick
                .map_or(String::new(), |t| format!(" ({})", t.key()));
            lines.push(Line::from(vec![
                mark,
                Span::raw(item.label),
                Span::styled(key, Style::new().fg(PINK)),
            ]));
            if !item.detail.is_empty() {
                lines.push(Line::styled(format!("    {}", item.detail), dim()));
            }
        }
        if gate::passed(&self.progress, s) {
            let msg = if s < layout::last() {
                format!(
                    "Gate passed. Make the stage {} edits, flash, then →.",
                    s + 1
                )
            } else {
                format!("Every gate passed. You type {}.", layout::plan().name)
            };
            lines.push(Line::styled(msg, Style::new().fg(Color::Green)));
        }
        let block = Block::bordered().title(format!(
            " gate before stage {} ",
            (s + 1).min(layout::last())
        ));
        frame.render_widget(
            Paragraph::new(lines)
                .block(block)
                .wrap(Wrap { trim: false }),
            area,
        );
    }

    fn render_warmup(&self, frame: &mut Frame, area: Rect, w: &Warmup, now: Instant) {
        let current = w.current(now);
        let mut lines = vec![
            Line::styled(
                format!(
                    "About {} seconds. Gentle: nothing should hurt.",
                    Warmup::total()
                ),
                dim(),
            ),
            Line::raw(""),
        ];
        for (i, step) in warmup::STEPS.iter().enumerate() {
            let line = match current {
                Some((c, left)) if c == i => Line::from(vec![
                    Span::styled(
                        format!(" ▶ {} ", step.title),
                        Style::new()
                            .fg(Color::Black)
                            .bg(PINK)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(format!("  {left}s  "), Style::new().fg(PINK)),
                    Span::raw(step.how),
                ]),
                Some((c, _)) if c > i => {
                    Line::styled(format!(" ✓ {}", step.title), Style::new().fg(Color::Green))
                }
                _ => Line::styled(format!("   {} ({}s)", step.title, step.secs), dim()),
            };
            lines.push(line);
        }
        if let Some(b) = w.breath(now) {
            lines.push(Line::raw(""));
            lines.push(Line::styled(
                format!("        {b}"),
                Style::new().fg(TEAL).add_modifier(Modifier::BOLD),
            ));
        }
        let block = Block::bordered()
            .border_style(Style::new().fg(TEAL))
            .title(" warm-up ");
        frame.render_widget(
            Paragraph::new(lines)
                .block(block)
                .wrap(Wrap { trim: false }),
            area,
        );
    }

    fn text_lines(&self) -> Vec<Line<'static>> {
        let d = &self.drill;
        if d.done {
            return vec![Line::styled("Done. Press enter for another round.", dim())];
        }
        if d.mode == Mode::Recall {
            let cur = d.expected().unwrap_or(' ');
            let next: String = d
                .text
                .iter()
                .skip(d.pos + 1)
                .take(3)
                .flat_map(|c| [*c, ' '])
                .collect();
            let style = if d.error {
                Style::new()
                    .fg(Color::Black)
                    .bg(MISS)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::new()
                    .fg(Color::Black)
                    .bg(PINK)
                    .add_modifier(Modifier::BOLD)
            };
            return vec![Line::from(vec![
                Span::raw("      "),
                Span::styled(format!("   {cur}   "), style),
                Span::styled(format!("    next: {next}"), dim()),
            ])];
        }
        let mut start = d.pos.saturating_sub(40);
        while start > 0 && d.text[start - 1] != ' ' {
            start -= 1;
        }
        let end = (start + 240).min(d.text.len());
        let mut spans = Vec::new();
        for i in start..end {
            let c = d.text[i];
            let style = if i < d.pos {
                Style::new()
            } else if i == d.pos {
                let base = Style::new().add_modifier(Modifier::UNDERLINED | Modifier::BOLD);
                if d.error {
                    base.bg(MISS).fg(Color::Black)
                } else {
                    base.fg(PINK)
                }
            } else {
                dim()
            };
            let shown = if c == ' ' && i == d.pos { '·' } else { c };
            spans.push(Span::styled(shown.to_string(), style));
        }
        vec![Line::from(spans)]
    }
}

/// The profile in use, for the dashboard's title, unless it's main: so you
/// don't practise into a test profile by accident.
fn profile_span() -> Option<Span<'static>> {
    crate::profile::label()
        .map(|p| Span::styled(format!("   profile {p}"), Style::new().fg(Color::Yellow)))
}

/// The staying dashboard's sections: a title and its lines.
fn staying_lines(
    st: &report::Standing,
    advice: &coach::Advice,
    real_use: Option<String>,
) -> Vec<(&'static str, Vec<String>)> {
    let mut speed = Vec::new();
    match st.baseline {
        Some(b) => speed.push(format!("Started at {b:.0} WPM.")),
        None => speed.push("Your first three real-text runs set where you started.".into()),
    }
    if let Some((wpm, acc, n)) = st.recent {
        let vs = st.baseline.map_or(String::new(), |b| {
            format!(", {:+.0} on where you started", wpm - b)
        });
        speed.push(format!(
            "Lately {wpm:.0} WPM at {acc:.1}% (your last {n} real-text runs){vs}."
        ));
    }
    if let Some(best) = st.best {
        speed.push(format!(
            "Best {best:.0} WPM at {:.0}%+.",
            gate::REAL_ACCURACY
        ));
    }
    let weak = if st.weak.is_empty() {
        vec!["Not enough typing yet to tell. A few sessions and they'll show.".to_string()]
    } else {
        st.weak
            .iter()
            .map(|(c, ms, miss)| format!("{c}   {ms:.0}ms · {miss:.0}% missed"))
            .collect()
    };
    let mut practice = vec![format!(
        "Today: {} of {} sessions · {}-day streak.",
        advice.sessions_today,
        coach::SESSIONS_PER_DAY,
        advice.streak
    )];
    practice.extend(real_use.map(|l| format!("{l}.")));
    vec![
        ("Speed", speed),
        ("Weak keys lately", weak),
        ("Practice", practice),
    ]
}

fn dim() -> Style {
    Style::new().fg(Color::DarkGray)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use ratatui::crossterm::event::KeyEventKind;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new_with_kind(code, KeyModifiers::NONE, KeyEventKind::Press)
    }

    fn app() -> (App, PathBuf) {
        let path = std::env::temp_dir().join(format!(
            "very-normal-app-{}-{:?}.json",
            std::process::id(),
            std::thread::current().id()
        ));
        let p = Progress {
            stage: 1,
            warmed_up: Some(coach::today(now_ms())),
            ..Default::default()
        };
        (
            App::new(p, path.clone(), Instant::now(), Phase::Dashboard),
            path,
        )
    }

    fn screen(app: &App) -> String {
        let mut t = Terminal::new(TestBackend::new(200, 40)).unwrap();
        t.draw(|f| app.render(f, Instant::now())).unwrap();
        t.backend()
            .buffer()
            .content()
            .iter()
            .map(|c| c.symbol())
            .collect::<String>()
    }

    #[test]
    fn shows_the_whole_plan_and_the_gate_not_a_trainer() {
        let (app, _) = app();
        let s = screen(&app);
        assert!(s.contains("stage 1 of 9"), "{s}");
        for stage in 0..=layout::last() {
            assert!(s.contains(layout::name(stage)), "{s}");
        }
        assert!(s.contains("gate before stage 2"), "{s}");
        assert!(
            !s.contains("voyager"),
            "the map lives on the layout screen:\n{s}"
        );
        assert!(!s.contains("trainer"), "{s}");
        assert!(
            !s.contains("configure.zsa.io"),
            "edits wait for the gate:\n{s}"
        );
        assert!(
            s.contains("Time guesses come after 3 real-text sessions"),
            "{s}"
        );
    }

    #[test]
    fn a_passed_gate_shows_the_next_edits() {
        let (mut app, _) = app();
        app.progress.stage = 0;
        app.progress.manual_mut(0).oryx = true;
        app.progress.manual_mut(0).wpm = 35.0;
        let s = screen(&app);
        assert!(s.contains("Stage 1: E and T come home"), "{s}");
        assert!(s.contains("configure.zsa.io/voyager/layouts/"), "{s}");
    }

    #[test]
    fn typing_a_whole_drill_records_a_run() {
        let (mut app, path) = app();
        let now = Instant::now();
        app.handle_key(key(KeyCode::Char('2')), now);
        app.handle_key(key(KeyCode::Enter), now);
        let text: Vec<char> = app.drill.text.clone();
        for c in text {
            app.handle_key(key(KeyCode::Char(c)), now);
        }
        assert!(!app.typing);
        assert_eq!(app.progress.runs.len(), 1);
        assert_eq!(app.progress.runs[0].mode, "words");
        assert_eq!(app.progress.runs[0].acc, 100.0);
        assert!(Progress::load(&path).unwrap().runs.len() == 1);
        std::fs::remove_file(path).ok();
    }

    #[test]
    fn letters_typed_outside_a_drill_tick_boxes_instead() {
        let (mut app, path) = app();
        app.handle_key(key(KeyCode::Char('v')), Instant::now());
        assert!(app.progress.manual(1).vim);
        app.handle_key(key(KeyCode::Right), Instant::now());
        assert_eq!(app.progress.stage, 2);
        std::fs::remove_file(path).ok();
    }

    #[test]
    fn escape_abandons_a_drill_without_saving() {
        let (mut app, path) = app();
        let now = Instant::now();
        app.handle_key(key(KeyCode::Char('1')), now);
        app.handle_key(key(KeyCode::Enter), now);
        assert!(app.typing);
        app.handle_key(key(KeyCode::Char('x')), now);
        app.handle_key(key(KeyCode::Esc), now);
        assert!(!app.typing);
        assert!(app.progress.runs.is_empty());
        assert_eq!(
            app.phase,
            Phase::Practice,
            "the first esc just stops the drill"
        );
        app.handle_key(key(KeyCode::Esc), now);
        assert_eq!(
            app.phase,
            Phase::Dashboard,
            "the second goes back to the plan"
        );
        std::fs::remove_file(path).ok();
    }

    #[test]
    fn a_drill_of_your_own_has_no_session_clock() {
        let (mut app, _) = app();
        app.handle_key(key(KeyCode::Char('6')), Instant::now());
        assert_eq!(app.mode, Mode::Mirror);
        let s = screen(&app);
        assert!(s.contains(Mode::Mirror.label()), "{s}");
        assert!(!s.contains("of 10:00 done"), "{s}");
        assert!(s.contains("esc back to the dashboard"), "{s}");
    }
}

#[cfg(test)]
mod coach_tests {
    use super::*;
    use ratatui::crossterm::event::KeyEventKind;

    #[test]
    fn the_coach_makes_you_ask_twice_after_a_long_session() {
        let path =
            std::env::temp_dir().join(format!("very-normal-coach-{}.json", std::process::id()));
        let end = now_ms();
        let runs = (0..13u64)
            .map(|i| Run {
                t: end - (12 - i) * 120_000,
                stage: 1,
                mode: "words".into(),
                wpm: 15.0,
                acc: 98.0,
                secs: 120.0,
                ..Run::default()
            })
            .collect();
        let p = Progress {
            stage: 1,
            runs,
            warmed_up: Some(coach::today(now_ms())),
            ..Default::default()
        };
        let mut app = App::new(p, path.clone(), Instant::now(), Phase::Dashboard);
        let enter =
            KeyEvent::new_with_kind(KeyCode::Enter, KeyModifiers::NONE, KeyEventKind::Press);
        let two =
            KeyEvent::new_with_kind(KeyCode::Char('2'), KeyModifiers::NONE, KeyEventKind::Press);
        app.handle_key(two, Instant::now());
        assert_eq!(app.phase, Phase::Practice, "1-6 open a drill of your own");
        app.handle_key(enter, Instant::now());
        assert!(!app.typing);
        assert!(app.message.as_ref().unwrap().0.contains("rebel"));
        app.handle_key(enter, Instant::now());
        assert!(app.typing);
        std::fs::remove_file(path).ok();
    }
}

#[cfg(test)]
mod warmup_tests {
    use super::*;
    use ratatui::crossterm::event::KeyEventKind;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new_with_kind(code, KeyModifiers::NONE, KeyEventKind::Press)
    }

    #[test]
    fn enter_on_the_dashboard_starts_a_guided_session_with_a_warm_up() {
        let path =
            std::env::temp_dir().join(format!("very-normal-warmup-{}.json", std::process::id()));
        let mut app = App::new(
            Progress {
                stage: 1,
                ..Default::default()
            },
            path.clone(),
            Instant::now(),
            Phase::Dashboard,
        );
        let now = Instant::now();
        app.handle_key(key(KeyCode::Enter), now);
        assert_eq!(app.phase, Phase::Layout);
        app.handle_key(key(KeyCode::Enter), now);
        assert!(app.warmup.is_some() && !app.typing);

        app.handle_key(key(KeyCode::Char('s')), now);
        assert!(app.warmup.is_none());
        assert_eq!(app.phase, Phase::Practice);
        let skipped = &app.message.as_ref().unwrap().0;
        assert!(
            !skipped.contains("Bold") && !skipped.contains("wrists"),
            "{skipped}"
        );
        assert_eq!(app.progress.warmed_up, Some(coach::today(now_ms())));
        std::fs::remove_file(path).ok();
    }

    #[test]
    fn enter_steps_through_the_warm_up_to_the_end() {
        let path =
            std::env::temp_dir().join(format!("very-normal-warmup2-{}.json", std::process::id()));
        let mut app = App::new(
            Progress::default(),
            path.clone(),
            Instant::now(),
            Phase::Layout,
        );
        let now = Instant::now();
        app.handle_key(key(KeyCode::Enter), now);
        for _ in 0..warmup::STEPS.len() {
            app.handle_key(key(KeyCode::Enter), now);
        }
        assert!(app.warmup.is_none());
        assert!(app.message.as_ref().unwrap().0.contains("Warmed up"));
        std::fs::remove_file(path).ok();
    }
}

#[cfg(test)]
mod verdict_tests {
    use super::*;
    use crate::store::Run;

    fn words_run(acc: f64) -> Run {
        Run {
            t: 1,
            stage: 1,
            mode: "words".into(),
            wpm: 15.0,
            acc,
            secs: 60.0,
            ..Run::default()
        }
    }

    #[test]
    fn the_number_shown_is_the_number_judged() {
        // 96.96% shows as 97.0%, so it has to count as 97%
        let p = Progress {
            runs: vec![words_run((96.96f64 * 10.0).round() / 10.0)],
            ..Default::default()
        };
        assert_eq!(gate::words_hits(&p, 1), 1);
    }
}

#[cfg(test)]
mod frustration_tests {
    use super::*;
    use ratatui::crossterm::event::KeyEventKind;

    #[test]
    fn several_restarts_suggest_a_break() {
        let path = std::env::temp_dir().join(format!(
            "very-normal-frustration-{}.json",
            std::process::id()
        ));
        let p = Progress {
            stage: 1,
            warmed_up: Some(coach::today(now_ms())),
            ..Default::default()
        };
        let mut app = App::new(p, path.clone(), Instant::now(), Phase::Dashboard);
        let key = |c| KeyEvent::new_with_kind(c, KeyModifiers::NONE, KeyEventKind::Press);
        app.handle_key(key(KeyCode::Char('2')), Instant::now());
        for _ in 0..3 {
            app.handle_key(key(KeyCode::Enter), Instant::now());
            app.handle_key(key(KeyCode::Esc), Instant::now());
        }
        assert!(app.message.as_ref().unwrap().0.contains("Frustration"));
        std::fs::remove_file(path).ok();
    }
}

#[cfg(test)]
mod guided_tests {
    use super::*;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use ratatui::buffer::Buffer;
    use ratatui::crossterm::event::KeyEventKind;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new_with_kind(code, KeyModifiers::NONE, KeyEventKind::Press)
    }

    fn ctrl_c() -> KeyEvent {
        KeyEvent::new_with_kind(
            KeyCode::Char('c'),
            KeyModifiers::CONTROL,
            KeyEventKind::Press,
        )
    }

    fn guided(name: &str, warmed_up: Option<String>) -> App {
        let path = std::env::temp_dir().join(format!(
            "very-normal-guided-{name}-{}.json",
            std::process::id()
        ));
        let p = Progress {
            stage: 1,
            warmed_up,
            ..Default::default()
        };
        App::new(p, path, Instant::now(), Phase::Layout)
    }

    fn draw(app: &App) -> Buffer {
        draw_at(app, Instant::now())
    }

    fn draw_at(app: &App, now: Instant) -> Buffer {
        let mut t = Terminal::new(TestBackend::new(80, 24)).unwrap();
        t.draw(|f| app.render(f, now)).unwrap();
        t.backend().buffer().clone()
    }

    fn text(b: &Buffer) -> String {
        let w = b.area.width as usize;
        b.content()
            .chunks(w)
            .map(|row| row.iter().map(|c| c.symbol()).collect::<String>())
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn assert_quiet(s: &str, app: &App) {
        assert!(
            !s.contains(&app.advice().message),
            "coach message on screen:\n{s}"
        );
        assert!(!s.to_lowercase().contains("gate"), "gate on screen:\n{s}");
        assert!(!s.to_lowercase().contains("oryx"), "oryx on screen:\n{s}");
        for m in Mode::ALL {
            assert!(!s.contains(m.label()), "{:?} on screen:\n{s}", m.label());
        }
    }

    #[test]
    fn starts_with_the_layout_on_its_own() {
        let app = guided("layout", None);
        let b = draw(&app);
        let s = text(&b);
        assert!(s.contains(&layout::plan().name.to_lowercase()), "{s}");
        assert!(s.contains("stage 1"), "{s}");
        assert!(s.contains(layout::name(1)), "{s}");
        assert!(s.contains("voyager"), "{s}");
        for c in layout::changes(1) {
            assert!(s.contains("Moving this stage"), "{s}");
            assert!(s.contains(c.to), "{s}");
        }
        let help = s.lines().last().unwrap();
        for k in ["enter", "d ", "q "] {
            assert!(help.contains(k), "{help}");
        }
        assert_quiet(&s, &app);
    }

    #[test]
    fn the_keys_that_move_stand_out() {
        let app = guided("styles", None);
        let b = draw(&app);
        let keys = layout::keys(1);
        let changed: Vec<char> = (0..30)
            .filter(|&i| layout::changed_now(1, i))
            .map(|i| keys[i])
            .collect();
        let still = (0..30)
            .find(|&i| !layout::moved(1, i))
            .map(|i| keys[i])
            .unwrap();
        // the key cells sit inside the border, between the outer-column labels
        let mut changed_fg = Vec::new();
        let mut still_fg = Vec::new();
        for y in 3..6 {
            for x in 6..41 {
                let cell = &b[(x, y)];
                let ch = cell.symbol().chars().next().unwrap_or(' ');
                if changed.contains(&ch) {
                    changed_fg.push(cell.fg);
                }
                if ch == still {
                    still_fg.push(cell.fg);
                }
            }
        }
        assert!(
            !changed_fg.is_empty() && changed_fg.iter().all(|&c| c == PINK),
            "{changed_fg:?}"
        );
        assert!(
            !still_fg.is_empty() && still_fg.iter().all(|&c| c != PINK),
            "{still_fg:?}"
        );
    }

    #[test]
    fn enter_starts_the_warm_up_even_after_one_today() {
        let mut app = guided("enter", Some(coach::today(now_ms())));
        app.handle_key(key(KeyCode::Enter), Instant::now());
        assert_eq!(app.phase, Phase::Warmup);
        assert!(app.warmup.is_some());
        let s = text(&draw(&app));
        assert!(s.contains("warm-up"), "{s}");
        assert!(!s.contains("voyager"), "{s}");
        assert_quiet(&s, &app);
    }

    #[test]
    fn d_goes_to_the_dashboard_and_q_quits() {
        let mut app = guided("d", None);
        app.handle_key(key(KeyCode::Char('d')), Instant::now());
        assert_eq!(app.phase, Phase::Dashboard);

        let mut app = guided("q", None);
        app.handle_key(key(KeyCode::Char('q')), Instant::now());
        assert!(app.should_quit);
    }

    #[test]
    fn finishing_the_warm_up_starts_practice_without_another() {
        let mut app = guided("finish", None);
        let now = Instant::now();
        app.handle_key(key(KeyCode::Enter), now);
        app.tick(now + std::time::Duration::from_secs(Warmup::total() + 1));
        assert_eq!(app.phase, Phase::Practice);
        assert_eq!(app.progress.warmed_up, Some(coach::today(now_ms())));

        let later = now + std::time::Duration::from_secs(Warmup::total() + 2);
        app.handle_key(key(KeyCode::Enter), later);
        assert!(app.warmup.is_none());
        assert!(app.typing);
    }

    #[test]
    fn skipping_or_leaving_the_warm_up() {
        let mut app = guided("skip", None);
        app.handle_key(key(KeyCode::Enter), Instant::now());
        app.handle_key(key(KeyCode::Char('s')), Instant::now());
        assert_eq!(app.phase, Phase::Practice);
        assert_eq!(app.progress.warmed_up, Some(coach::today(now_ms())));

        let mut app = guided("esc", None);
        app.handle_key(key(KeyCode::Enter), Instant::now());
        app.handle_key(key(KeyCode::Esc), Instant::now());
        assert_eq!(app.phase, Phase::Practice);
        assert_eq!(app.progress.warmed_up, None);
    }

    fn practising(name: &str) -> App {
        let mut app = guided(name, Some(coach::today(now_ms())));
        app.handle_key(key(KeyCode::Enter), Instant::now());
        app.handle_key(key(KeyCode::Char('s')), Instant::now());
        assert_eq!(app.phase, Phase::Practice);
        app
    }

    #[test]
    fn staying_put_takes_turns_with_real_text_and_weak_keys() {
        let modes: Vec<Mode> = (0..4).map(|i| practice_mode_for(i, true)).collect();
        assert_eq!(modes, [Mode::Real, Mode::Words, Mode::Real, Mode::Words]);
    }

    #[test]
    fn the_staying_dashboard_says_how_youre_getting_on() {
        let advice = coach::assess(&[], now_ms());
        let empty = staying_lines(&report::Standing::default(), &advice, None);
        let text = format!("{empty:?}");
        assert!(!text.contains("for real"), "{text}");
        assert!(
            text.contains("first three real-text runs") && text.contains("Not enough typing"),
            "{text}"
        );
        let st = report::Standing {
            baseline: Some(50.0),
            recent: Some((56.0, 96.5, 5)),
            best: Some(61.0),
            weak: vec![('k', 420.0, 8.0)],
        };
        let real = Some("Typing for real: 10 min this week".to_string());
        let text = format!("{:?}", staying_lines(&st, &advice, real));
        assert!(
            text.contains("Typing for real: 10 min this week."),
            "{text}"
        );
        assert!(text.contains("Started at 50 WPM"), "{text}");
        assert!(
            text.contains("Lately 56 WPM at 96.5%") && text.contains("+6 on where you started"),
            "{text}"
        );
        assert!(
            text.contains("Best 61 WPM") && text.contains("k   420ms · 8% missed"),
            "{text}"
        );
    }

    #[test]
    fn picks_real_text_first_then_new_keys_and_review() {
        let modes: Vec<Mode> = (0..6).map(practice_mode).collect();
        assert_eq!(
            modes,
            [
                Mode::Real,
                Mode::Words,
                Mode::Review,
                Mode::Words,
                Mode::Review,
                Mode::Words
            ]
        );
        let app = practising("first");
        assert_eq!(app.mode, Mode::Real);
    }

    #[test]
    fn moves_through_the_drills_and_stops_at_ten_minutes() {
        let mut app = practising("ten");
        let now = Instant::now();
        app.practice_next(60.0, 12.0, 97.0, now);
        assert_eq!(app.mode, Mode::Words);
        assert!(
            app.message
                .as_ref()
                .unwrap()
                .0
                .to_lowercase()
                .contains("press enter for the next drill"),
            "{:?}",
            app.message
        );
        for _ in 0..2 {
            app.practice_next(180.0, 12.0, 98.0, now);
        }
        assert_eq!(app.phase, Phase::Practice, "540 seconds isn't ten minutes");
        app.practice_next(180.0, 12.0, 98.0, now);
        assert_eq!(app.phase, Phase::Report);
        assert!(app.block.is_none());
        assert!(app.message.as_ref().unwrap().0.contains("ten minutes"));
    }

    #[test]
    fn picks_up_todays_session_under_ten_minutes() {
        let mut app = guided("resume", Some(coach::today(now_ms())));
        let t = now_ms();
        for i in 0..3u64 {
            app.progress.runs.push(Run {
                t: t - (3 - i) * 140_000,
                stage: app.stage(),
                mode: "words".into(),
                wpm: 12.0,
                acc: 97.0,
                secs: 140.0,
                ..Run::default()
            });
        }
        app.handle_key(key(KeyCode::Enter), Instant::now());
        app.handle_key(key(KeyCode::Char('s')), Instant::now());
        assert_eq!(app.phase, Phase::Practice);
        assert_eq!(app.mode, practice_mode(3), "the drills carry on in turn");
        let s = text(&draw(&app));
        assert!(s.contains("7:00 of 10:00 done"), "{s}");
        assert!(s.contains("where you left off"), "{s}");

        app.practice_next(180.0, 12.0, 97.0, Instant::now());
        assert_eq!(app.phase, Phase::Report, "7:00 plus 3:00 finishes it");
    }

    #[test]
    fn question_mark_explains_the_drills_and_any_key_closes_it() {
        let mut app = practising("help");
        let now = Instant::now();
        app.handle_key(key(KeyCode::Char('?')), now);
        let s = text(&draw(&app));
        assert!(s.contains("The guided session"), "{s}");
        app.handle_key(key(KeyCode::Down), now);
        assert!(app.help, "arrows scroll");
        assert!(
            text(&draw(&app)) != s,
            "80x24 is too short for it all, so it scrolls"
        );
        for _ in 0..100 {
            app.handle_key(key(KeyCode::Down), now);
        }
        assert!(
            text(&draw(&app)).contains("ctrl-c"),
            "scrolls as far as the last line"
        );
        app.handle_key(key(KeyCode::Char('q')), now);
        assert!(
            !app.help && !app.should_quit,
            "the key that closes help does nothing else"
        );
        app.handle_key(key(KeyCode::Enter), now);
        assert!(app.typing);
        app.handle_key(key(KeyCode::Char('?')), now);
        assert!(!app.help, "? is just a key to type during a drill");
    }

    #[test]
    fn keys_still_flying_when_a_drill_ends_dont_stop_the_music_or_quit() {
        let mut app = practising("settle");
        let now = Instant::now();
        for _ in 0..3 {
            app.practice_next(60.0, 12.0, 90.0, now);
        }
        app.handle_key(key(KeyCode::Enter), now);
        app.handle_key(key(KeyCode::Char('~')), now);
        app.finish(now);
        assert_eq!(
            app.phase,
            Phase::Report,
            "two sloppy runs after easing off end the block"
        );
        for c in ['m', 'q', 'x'] {
            app.handle_key(key(KeyCode::Char(c)), now + SETTLE / 2);
        }
        assert!(!app.should_quit);
        assert_eq!(app.phase, Phase::Report);
        app.handle_key(key(KeyCode::Char('q')), now + SETTLE);
        assert!(app.should_quit);
    }

    #[test]
    fn two_sloppy_runs_ease_off_to_key_recall_and_two_more_end_it() {
        let mut app = practising("sloppy");
        let now = Instant::now();
        app.practice_next(60.0, 12.0, 94.0, now);
        app.practice_next(60.0, 12.0, 96.0, now);
        app.practice_next(60.0, 12.0, 93.5, now);
        assert_eq!(app.phase, Phase::Practice);
        app.practice_next(60.0, 12.0, 92.0, now);
        assert_eq!(app.phase, Phase::Practice, "the first two ease off instead");
        assert_eq!(app.mode, Mode::Recall);
        let msg = &app.message.as_ref().unwrap().0;
        assert!(
            msg.contains("93.5%") && msg.contains("92.0%") && msg.contains("key recall"),
            "{msg}"
        );
        assert!(app.rest.is_some_and(|r| r.left(now) == 45));
        app.practice_next(20.0, 12.0, 90.0, now);
        assert_eq!(
            app.phase,
            Phase::Practice,
            "one rough run after easing isn't two"
        );
        app.practice_next(20.0, 12.0, 91.0, now);
        assert_eq!(app.phase, Phase::Report);
        let msg = &app.message.as_ref().unwrap().0;
        assert!(msg.contains("90.0%") && msg.contains("91.0%"), "{msg}");
        assert!(msg.contains("No rush") && !msg.contains("Go"), "{msg}");
    }

    #[test]
    fn a_rough_run_gets_a_breather_that_enter_skips() {
        let mut app = practising("rough");
        let now = Instant::now();
        app.handle_key(key(KeyCode::Enter), now);
        let typed: String = app.drill.text.iter().collect();
        for c in typed.chars() {
            // a miss on every key: well under 95%
            app.handle_key(key(KeyCode::Char('~')), now);
            app.handle_key(key(KeyCode::Char(c)), now);
        }
        assert!(!app.typing);
        let rest = app.rest.expect("a breather after a rough run");
        assert!(rest.cue.contains("Breathe"));
        let later = now + SETTLE;
        let screen = text(&draw_at(&app, later));
        assert!(screen.contains("enter skips"), "{screen}");
        let screen = text(&draw_at(&app, later + Duration::from_secs(30)));
        assert!(screen.contains("Ready when you are"), "{screen}");
        app.handle_key(key(KeyCode::Enter), later);
        assert!(app.typing && app.rest.is_none());
    }

    #[test]
    fn a_clean_run_has_no_breather_but_halfway_does() {
        let mut app = practising("halfway");
        let now = Instant::now();
        app.practice_next(60.0, 12.0, 98.0, now);
        assert!(app.rest.is_none());
        app.practice_next(200.0, 12.0, 98.0, now);
        assert!(app.rest.is_none(), "4:20 isn't halfway");
        app.practice_next(60.0, 12.0, 98.0, now);
        assert!(app.rest.is_some_and(|r| r.cue.contains("Halfway")));
        assert!(!app.message.as_ref().unwrap().0.contains("Press enter"));
    }

    #[test]
    fn after_a_rough_run_the_next_drill_doesnt_lean_on_hard_keys() {
        let mut app = practising("gentle");
        let stage = app.progress.letters.entry("1".into()).or_default();
        for (c, ms) in [('a', 250.0), ('b', 260.0), ('c', 270.0), ('k', 2000.0)] {
            stage.insert(
                c.to_string(),
                crate::store::LetterStat {
                    n: 20,
                    miss: 0,
                    ms: ms * 20.0,
                },
            );
        }
        let now = Instant::now();
        app.mode = Mode::Words;
        app.new_drill(now);
        assert_eq!(app.leaning, vec!['k']);
        app.last_acc = Some(90.0);
        app.new_drill(now);
        assert!(app.leaning.is_empty(), "easy wins after a rough run");
        app.last_acc = Some(97.0);
        app.new_drill(now);
        assert_eq!(app.leaning, vec!['k']);
    }

    #[test]
    fn a_miss_is_soft_not_red() {
        let mut app = practising("soft");
        let now = Instant::now();
        app.handle_key(key(KeyCode::Enter), now);
        app.handle_key(key(KeyCode::Char('~')), now);
        assert!(app.drill.error);
        let b = draw(&app);
        assert!(
            b.content()
                .iter()
                .all(|c| c.bg != Color::LightRed && c.bg != Color::Red),
            "no red on a miss"
        );
        assert!(b.content().iter().any(|c| c.bg == MISS));
    }

    #[test]
    fn esc_abandons_a_drill_but_ends_the_block_when_idle() {
        let mut app = practising("esc");
        let now = Instant::now();
        app.handle_key(key(KeyCode::Enter), now);
        assert!(app.typing);
        app.handle_key(key(KeyCode::Esc), now);
        assert!(!app.typing);
        assert_eq!(app.phase, Phase::Practice);
        assert!(app.progress.runs.is_empty());
        app.handle_key(key(KeyCode::Esc), now);
        assert_eq!(app.phase, Phase::Dashboard);
    }

    #[test]
    fn the_coach_doesnt_stop_a_block() {
        let mut app = practising("cap");
        let now = now_ms();
        for i in 0..50 {
            app.progress.runs.push(Run {
                t: now - (50 - i) * 60_000,
                stage: 1,
                mode: "words".into(),
                wpm: 12.0,
                acc: 98.0,
                secs: 60.0,
                ..Run::default()
            });
        }
        assert!(app.advice().blocks());
        app.handle_key(key(KeyCode::Enter), Instant::now());
        assert!(app.typing);
    }

    #[test]
    fn the_practice_screen_shows_only_the_drill_and_the_map() {
        let mut app = practising("screen");
        let now = Instant::now();
        app.practice_next(75.0, 12.0, 97.0, now);
        let s = text(&draw(&app));
        assert!(s.contains("1:15 of 10:00 done"), "{s}");
        assert!(s.contains(Mode::Words.label()), "{s}");
        assert!(s.contains(app.drill.expected().unwrap()), "{s}");
        assert!(s.contains("voyager"), "{s}");
        assert!(!s.contains(&app.advice().message), "{s}");
        assert!(!s.to_lowercase().contains("gate"), "{s}");
        assert!(!s.to_lowercase().contains("oryx"), "{s}");
        assert!(!s.contains("stage 1 of"), "{s}");
        for m in [
            Mode::Recall,
            Mode::Review,
            Mode::Real,
            Mode::Links,
            Mode::Mirror,
        ] {
            assert!(!s.contains(m.label()), "{:?}:\n{s}", m.label());
        }
    }

    #[test]
    fn the_report_shows_after_practice_then_the_dashboard() {
        let mut app = practising("report");
        let now = Instant::now();
        app.handle_key(key(KeyCode::Enter), now);
        while let Some(c) = app.drill.expected() {
            app.handle_key(
                key(KeyCode::Char(c)),
                now + std::time::Duration::from_secs(1),
            );
            if !app.typing {
                break;
            }
        }
        app.handle_key(key(KeyCode::Esc), now + SETTLE * 2);
        assert_eq!(app.phase, Phase::Report);
        assert!(!app.report.is_empty());
        let s = text(&draw(&app));
        assert!(
            s.contains("Drills") && s.contains("Before the next stage"),
            "{s}"
        );
        assert!(s.contains(app.report.drills[0].mode.label()), "{s}");
        let help = s.lines().last().unwrap();
        assert!(help.contains("enter") && help.contains("q "), "{help}");
        for m in Mode::ALL {
            assert!(
                !s.contains(&format!(" {} ", m.label())) || m == Mode::Real,
                "{s}"
            );
        }
        app.handle_key(key(KeyCode::Enter), now + SETTLE * 2);
        assert_eq!(app.phase, Phase::Dashboard);

        let mut app = practising("report-q");
        app.practice_next(600.0, 12.0, 98.0, Instant::now());
        app.handle_key(key(KeyCode::Char('q')), Instant::now());
        assert!(app.should_quit);
    }

    #[test]
    fn the_layout_sets_the_scene_and_offers_music() {
        let app = guided("scene", None);
        let s = text(&draw(&app)).replace('\n', " ");
        assert!(s.contains("quiet place"), "{s}");
        assert!(s.contains("Headphones"), "{s}");
        assert!(s.contains("gentle"), "{s}");
        assert!(s.lines().last().unwrap().contains("m music"), "{s}");
    }

    #[test]
    fn m_is_just_a_letter_while_practising() {
        let mut app = practising("m");
        let now = Instant::now();
        app.handle_key(key(KeyCode::Enter), now);
        app.handle_key(key(KeyCode::Char('m')), now);
        assert!(!app.music.playing());
        assert!(app.typing);
    }

    #[test]
    fn practice_shows_the_map_the_whole_time_and_a_pause_changes_nothing() {
        let mut app = practising("still");
        let now = Instant::now();
        app.handle_key(key(KeyCode::Enter), now);
        let mut t = Terminal::new(TestBackend::new(80, 24)).unwrap();
        t.draw(|f| app.render(f, now)).unwrap();
        let before = t.backend().buffer().clone();
        assert!(text(&before).contains("voyager"));
        let later = now + crate::drill::HINT_AFTER + std::time::Duration::from_secs(2);
        t.draw(|f| app.render(f, later)).unwrap();
        assert_eq!(&before, t.backend().buffer());
    }

    #[test]
    fn ctrl_c_quits_in_every_phase() {
        for phase in [Phase::Layout, Phase::Warmup, Phase::Dashboard] {
            let mut app = guided("ctrlc", None);
            app.phase = phase;
            if phase == Phase::Warmup {
                app.warmup = Some(Warmup::new(Instant::now()));
            }
            app.handle_key(ctrl_c(), Instant::now());
            assert!(app.should_quit, "{phase:?}");
        }
    }
}
