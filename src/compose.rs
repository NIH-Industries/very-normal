//! Compose mode: type a whole message in a small pane and send it to another
//! one (Claude, mostly), measuring the typing as you go. A mistake is a press
//! of a delete key. The text itself only ever lives in memory: nothing here
//! writes it anywhere, and a run keeps only per-key timings and counts.

use std::collections::{BTreeMap, VecDeque};
use std::time::{Duration, Instant};

use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::{Constraint, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::store::{LetterStat, Run};

/// A gap between keys longer than this is thinking, not typing: it counts
/// this much towards flow time and isn't used to time the key.
pub const GAP_CAP: Duration = Duration::from_secs(2);
/// The last this-many keys (typed or deleting) decide the nudge.
pub const WINDOW: usize = 150;
/// The nudge waits for this many keys in the window.
pub const NUDGE_MIN_KEYS: usize = 80;
/// Corrections (as a % of keys) below this never nudge, however tidy you usually are.
pub const NUDGE_FLOOR: f64 = 10.0;
/// How far over your usual correction rate counts as a rough patch.
pub const NUDGE_FACTOR: f64 = 1.5;
/// Flow minutes in one sitting before suggesting a stretch.
pub const STRETCH_MINUTES: f64 = 50.0;
/// A pause this long ends a sitting.
pub const SITTING_GAP: Duration = Duration::from_secs(10 * 60);
/// A run is saved once its first key is this old, and on quitting.
pub const FLUSH_AFTER: Duration = Duration::from_secs(10 * 60);
/// Runs with fewer keys than this aren't worth keeping.
pub const MIN_KEYS: u64 = 20;
/// Deleting more than this many characters in a row is rewriting, not fixing
/// a slip, so the retyped characters aren't paired up as typos.
const TYPO_BURST: usize = 2;
/// How many recent runs make up your usual correction rate.
const USUAL_RUNS: usize = 20;

pub const MODE: &str = "compose";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Nudge {
    RoughPatch,
    Stretch,
}

impl Nudge {
    pub fn message(self) -> &'static str {
        match self {
            Nudge::RoughPatch => "lots of corrections lately, a short break might help",
            Nudge::Stretch => "a long while typing: time for a stretch",
        }
    }
}

/// What's been typed since the last saved run.
#[derive(Debug, Clone, Default)]
struct Chunk {
    started: Option<Instant>,
    keys: u64,
    corrections: u64,
    net: i64,
    flow: Duration,
    letters: BTreeMap<String, LetterStat>,
    typos: BTreeMap<String, u64>,
}

#[derive(Debug, Clone, Default)]
pub struct Composer {
    /// The message so far, each character marked when it replaced a deleted one.
    text: Vec<(char, bool)>,
    /// Characters deleted since the last one typed, the most recent last: the
    /// next character typed goes where the last of these was.
    deleted: Vec<char>,
    /// Whether `deleted` came from a short burst of single deletes.
    slip: bool,
    last_key: Option<Instant>,
    chunk: Chunk,
    recent: VecDeque<bool>,
    /// Since the composer opened, for the status line.
    keys: u64,
    corrections: u64,
    net: i64,
    flow: Duration,
    sitting: Duration,
    usual: Option<f64>,
}

impl Composer {
    /// `usual` is your usual correction rate, in % of keys, from earlier runs.
    pub fn new(usual: Option<f64>) -> Composer {
        Composer {
            usual,
            ..Composer::default()
        }
    }

    pub fn text(&self) -> &[(char, bool)] {
        &self.text
    }

    pub fn is_empty(&self) -> bool {
        self.text.is_empty()
    }

    /// Times a key press: the gap since the last one, capped, added to flow
    /// time. Returns the gap when it's short enough to time a key by.
    fn tick(&mut self, now: Instant) -> Option<Duration> {
        let gap = self.last_key.map(|t| now.saturating_duration_since(t));
        if gap.is_none_or(|g| g >= SITTING_GAP) {
            self.sitting = Duration::ZERO;
        }
        let flow = gap.map_or(Duration::ZERO, |g| g.min(GAP_CAP));
        self.flow += flow;
        self.sitting += flow;
        self.chunk.flow += flow;
        self.chunk.started.get_or_insert(now);
        self.last_key = Some(now);
        gap.filter(|g| *g < GAP_CAP)
    }

    fn remember(&mut self, correction: bool) {
        self.recent.push_back(correction);
        if self.recent.len() > WINDOW {
            self.recent.pop_front();
        }
    }

    pub fn type_char(&mut self, c: char, now: Instant) {
        let gap = self.tick(now);
        self.keys += 1;
        self.net += 1;
        self.chunk.keys += 1;
        self.chunk.net += 1;
        self.remember(false);
        let replaced = self.deleted.pop();
        if let Some(old) = replaced
            && self.slip
            && old != c
            && !c.is_whitespace()
        {
            *self.chunk.typos.entry(format!("{c}>{old}")).or_default() += 1;
            self.chunk.letters.entry(c.to_string()).or_default().miss += 1;
        }
        if let Some(gap) = gap
            && !c.is_whitespace()
        {
            let stat = self.chunk.letters.entry(c.to_string()).or_default();
            stat.n += 1;
            stat.ms += gap.as_secs_f64() * 1000.0;
        }
        self.text.push((c, replaced.is_some()));
    }

    fn correction(&mut self, now: Instant, removed: Vec<char>, slip: bool) {
        self.tick(now);
        self.corrections += 1;
        self.chunk.corrections += 1;
        let n = removed.len() as i64;
        self.net -= n;
        self.chunk.net -= n;
        self.remember(true);
        self.slip = slip && self.deleted.len() + removed.len() <= TYPO_BURST;
        self.deleted.extend(removed);
    }

    /// Backspace: one correction.
    pub fn delete_char(&mut self, now: Instant) {
        let Some((c, _)) = self.text.pop() else {
            return;
        };
        self.correction(now, vec![c], true);
    }

    /// Ctrl-W or Alt-Backspace: back over any spaces, then the word. Still one correction.
    pub fn delete_word(&mut self, now: Instant) {
        let mut removed = Vec::new();
        while self.text.last().is_some_and(|(c, _)| c.is_whitespace()) {
            removed.push(self.text.pop().unwrap().0);
        }
        while self.text.last().is_some_and(|(c, _)| !c.is_whitespace()) {
            removed.push(self.text.pop().unwrap().0);
        }
        if !removed.is_empty() {
            self.correction(now, removed, false);
        }
    }

    /// Ctrl-U: throwing the message away is a choice, not a mistake.
    pub fn clear(&mut self) {
        let n = self.text.len() as i64;
        self.net -= n;
        self.chunk.net -= n;
        self.text.clear();
        self.deleted.clear();
    }

    /// The message, to send; the composer is empty afterwards.
    pub fn take(&mut self) -> String {
        self.deleted.clear();
        std::mem::take(&mut self.text)
            .into_iter()
            .map(|(c, _)| c)
            .collect()
    }

    /// Typing speed since the composer opened, counting only flow time.
    pub fn wpm(&self) -> Option<f64> {
        wpm(self.net, self.flow)
    }

    /// Corrections as a % of keys since the composer opened.
    pub fn corrected(&self) -> Option<f64> {
        (self.keys > 0).then(|| 100.0 * self.corrections as f64 / self.keys as f64)
    }

    pub fn nudge(&self) -> Option<Nudge> {
        if self.sitting.as_secs_f64() >= STRETCH_MINUTES * 60.0 {
            return Some(Nudge::Stretch);
        }
        if self.recent.len() < NUDGE_MIN_KEYS {
            return None;
        }
        let rate =
            100.0 * self.recent.iter().filter(|c| **c).count() as f64 / self.recent.len() as f64;
        let limit = NUDGE_FLOOR.max(self.usual.unwrap_or(0.0) * NUDGE_FACTOR);
        (rate > limit).then_some(Nudge::RoughPatch)
    }

    /// Whether the run so far has been going long enough to save.
    pub fn due(&self, now: Instant) -> bool {
        self.chunk
            .started
            .is_some_and(|s| now.saturating_duration_since(s) >= FLUSH_AFTER)
    }

    /// The typing since the last run, as a run to save (if there's enough of
    /// it), starting the next one afresh either way.
    pub fn take_run(&mut self, stage: usize, t_ms: u64) -> Option<Run> {
        let c = std::mem::take(&mut self.chunk);
        (c.keys >= MIN_KEYS).then(|| Run {
            t: t_ms,
            stage,
            mode: MODE.into(),
            wpm: wpm(c.net, c.flow).unwrap_or(0.0),
            acc: 100.0 * (1.0 - c.corrections as f64 / c.keys as f64),
            secs: c.flow.as_secs_f64(),
            letters: c.letters,
            typos: c.typos,
        })
    }
}

fn wpm(net: i64, flow: Duration) -> Option<f64> {
    let mins = flow.as_secs_f64() / 60.0;
    (mins > 0.0).then(|| net.max(0) as f64 / 5.0 / mins)
}

/// Your usual correction rate, in % of keys, over your recent compose runs.
pub fn usual(runs: &[Run]) -> Option<f64> {
    let recent: Vec<&Run> = runs.iter().rev().take(USUAL_RUNS).collect();
    (!recent.is_empty())
        .then(|| recent.iter().map(|r| 100.0 - r.acc).sum::<f64>() / recent.len() as f64)
}

/// The bytes that send `text` to a terminal as one paste and press Enter, so
/// a multi-line message arrives whole.
pub fn paste(text: &str) -> Vec<u8> {
    let mut out = b"\x1b[200~".to_vec();
    out.extend_from_slice(text.as_bytes());
    out.extend_from_slice(b"\x1b[201~\r");
    out
}

/// What the screen wants done after a key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    None,
    /// Bytes for the other pane.
    Send(Vec<u8>),
    Quit,
}

/// The compose pane: the message being typed, and one status row.
pub struct Screen {
    pub composer: Composer,
    /// What the other pane is called, for the status row.
    pub target: String,
    /// The key that sends, for the status row: `ctrl-enter` where the terminal
    /// reports modifiers on enter, `alt-enter` where it can't.
    pub send_key: &'static str,
    /// Something went wrong sending; shown in place of the stats until the next key.
    pub problem: Option<String>,
}

impl Screen {
    pub fn new(composer: Composer, target: String, send_key: &'static str) -> Screen {
        Screen {
            composer,
            target,
            send_key,
            problem: None,
        }
    }

    /// Enter is a newline and never reaches the other pane, so a stray press
    /// can't answer a question it's asking; ctrl-enter (or alt-enter) sends.
    pub fn handle_key(&mut self, key: KeyEvent, now: Instant) -> Action {
        use KeyCode::*;
        self.problem = None;
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        let alt = key.modifiers.contains(KeyModifiers::ALT);
        // ctrl-enter sends; alt-enter and cmd-enter too, where the terminal
        // lets them through (Rex keeps cmd-enter for splitting)
        let send = ctrl || alt || key.modifiers.contains(KeyModifiers::SUPER);
        match key.code {
            Char('c') if ctrl => return Action::Quit,
            Enter if send && !self.composer.is_empty() => {
                return Action::Send(paste(&self.composer.take()));
            }
            Enter if send || self.composer.is_empty() => {}
            Enter => self.composer.type_char('\n', now),
            Char('j') if ctrl => self.composer.type_char('\n', now),
            _ if self.composer.is_empty() && key.modifiers.is_empty() => {
                if let Some(bytes) = passthrough(key.code) {
                    return Action::Send(bytes.to_vec());
                }
                if let Char(c) = key.code {
                    self.composer.type_char(c, now);
                }
            }
            Backspace if alt => self.composer.delete_word(now),
            Backspace => self.composer.delete_char(now),
            Char('w') if ctrl => self.composer.delete_word(now),
            Char('u') if ctrl => self.composer.clear(),
            Char(c) if !send => self.composer.type_char(c, now),
            _ => {}
        }
        Action::None
    }

    pub fn render(&self, frame: &mut Frame) {
        let [text_area, status] =
            Layout::vertical([Constraint::Fill(1), Constraint::Length(1)]).areas(frame.area());
        let dim = Style::new().fg(Color::DarkGray);
        let lines = if self.composer.is_empty() {
            vec![Line::from(vec![
                Span::styled(" ", Style::new().add_modifier(Modifier::REVERSED)),
                Span::styled(
                    format!(
                        " type a message · {} sends it to {} · with nothing typed, esc and the arrows go straight there",
                        self.send_key, self.target
                    ),
                    dim,
                ),
            ])]
        } else {
            let all = wrapped(self.composer.text(), text_area.width.max(2) as usize);
            let skip = all.len().saturating_sub(text_area.height as usize);
            all.into_iter().skip(skip).collect()
        };
        frame.render_widget(Paragraph::new(lines), text_area);

        let mut spans = vec![Span::styled(
            format!("→ {}", self.target),
            Style::new().fg(Color::Magenta).add_modifier(Modifier::BOLD),
        )];
        let middle = if let Some(p) = &self.problem {
            Span::styled(format!("  {p}"), Style::new().fg(Color::Red))
        } else if let Some(n) = self.composer.nudge() {
            Span::styled(format!("  {}", n.message()), Style::new().fg(Color::Yellow))
        } else {
            let wpm = self
                .composer
                .wpm()
                .map_or("–".into(), |w| format!("{w:.0}"));
            let pct = self
                .composer
                .corrected()
                .map_or("–".into(), |c| format!("{c:.0}%"));
            Span::raw(format!("  {wpm} wpm · {pct} corrected"))
        };
        spans.push(middle);
        spans.push(Span::styled(
            format!("  ·  {} send · ctrl-c quit", self.send_key),
            dim,
        ));
        frame.render_widget(Paragraph::new(Line::from(spans)), status);
    }
}

/// The message as screen lines, corrections in yellow, ending with the cursor.
fn wrapped(text: &[(char, bool)], width: usize) -> Vec<Line<'static>> {
    let fixed = Style::new().fg(Color::Yellow);
    let mut lines = vec![];
    let mut line: Vec<Span<'static>> = vec![];
    let mut col = 0;
    for &(c, corrected) in text {
        if c == '\n' {
            lines.push(Line::from(std::mem::take(&mut line)));
            col = 0;
            continue;
        }
        if col == width {
            lines.push(Line::from(std::mem::take(&mut line)));
            col = 0;
        }
        let shown = if c == '\t' { ' ' } else { c };
        let style = if corrected { fixed } else { Style::new() };
        line.push(Span::styled(shown.to_string(), style));
        col += 1;
    }
    if col == width {
        lines.push(Line::from(std::mem::take(&mut line)));
    }
    line.push(Span::styled(
        " ",
        Style::new().add_modifier(Modifier::REVERSED),
    ));
    lines.push(Line::from(line));
    lines
}

/// The keys that go straight to the other pane while nothing is typed: esc
/// to interrupt, and the arrows and page keys to look around. None of them
/// can answer a question on their own.
fn passthrough(code: KeyCode) -> Option<&'static [u8]> {
    use KeyCode::*;
    Some(match code {
        Esc => b"\x1b",
        Up => b"\x1b[A",
        Down => b"\x1b[B",
        Right => b"\x1b[C",
        Left => b"\x1b[D",
        PageUp => b"\x1b[5~",
        PageDown => b"\x1b[6~",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(start: Instant, ms: u64) -> Instant {
        start + Duration::from_millis(ms)
    }

    fn typed(c: &Composer) -> String {
        c.text().iter().map(|(c, _)| *c).collect()
    }

    #[test]
    fn a_retyped_character_is_marked_as_a_correction() {
        let t = Instant::now();
        let mut c = Composer::new(None);
        for (i, ch) in "thw".chars().enumerate() {
            c.type_char(ch, at(t, 200 * i as u64));
        }
        c.delete_char(at(t, 700));
        c.type_char('e', at(t, 900));
        assert_eq!(typed(&c), "the");
        let marks: Vec<bool> = c.text().iter().map(|(_, m)| *m).collect();
        assert_eq!(marks, [false, false, true]);
        let run = c.take_run(1, 0);
        assert!(run.is_none(), "too few keys to keep");
    }

    #[test]
    fn every_delete_press_is_one_correction_and_slips_become_typos() {
        let t = Instant::now();
        let mut c = Composer::new(None);
        let mut ms = 0;
        let mut next = || {
            ms += 150;
            at(t, ms)
        };
        for ch in "hello world tesy".chars() {
            c.type_char(ch, next());
        }
        c.delete_char(next());
        c.type_char('t', next());
        for ch in " again".chars() {
            c.type_char(ch, next());
        }
        c.delete_word(next());
        for ch in "and more".chars() {
            c.type_char(ch, next());
        }
        assert_eq!(typed(&c), "hello world test and more");
        let run = c.take_run(2, 42).unwrap();
        assert_eq!(run.mode, "compose");
        assert_eq!(run.stage, 2);
        assert_eq!(run.t, 42);
        // 31 characters typed, 2 delete presses
        assert!((run.acc - 100.0 * (1.0 - 2.0 / 31.0)).abs() < 1e-9);
        assert_eq!(run.typos, BTreeMap::from([("t>y".to_string(), 1)]));
        assert_eq!(run.letters["t"].miss, 1);
        // a deleted word retyped differently isn't a pile of typos
        assert!(!run.typos.keys().any(|k| k.starts_with('a')));
        assert_eq!(c.corrected(), Some(100.0 * 2.0 / 31.0));
    }

    #[test]
    fn pauses_do_not_slow_the_speed_down() {
        let t = Instant::now();
        let mut c = Composer::new(None);
        // 25 characters at 200ms, a minute's thought, then 25 more
        for i in 0..25 {
            c.type_char('a', at(t, 200 * i));
        }
        for i in 0..25 {
            c.type_char('b', at(t, 70_000 + 200 * i));
        }
        // flow: 48 gaps of 200ms plus the pause, capped at 2s = 11.6s
        let wpm = c.wpm().unwrap();
        assert!((wpm - 50.0 / 5.0 / (11.6 / 60.0)).abs() < 1e-6, "{wpm}");
        let run = c.take_run(0, 0).unwrap();
        // the key after the pause isn't timed
        assert_eq!(run.letters["b"].n, 24);
        assert!((run.letters["a"].average_ms().unwrap() - 200.0).abs() < 1e-6);
    }

    #[test]
    fn clearing_and_sending_are_not_mistakes() {
        let t = Instant::now();
        let mut c = Composer::new(None);
        for (i, ch) in "draft".chars().enumerate() {
            c.type_char(ch, at(t, 100 * i as u64));
        }
        c.clear();
        assert!(c.is_empty());
        c.type_char('o', at(t, 1000));
        c.type_char('k', at(t, 1100));
        assert_eq!(c.take(), "ok");
        assert!(c.is_empty());
        assert_eq!(c.corrected(), Some(0.0));
    }

    #[test]
    fn many_corrections_lately_suggest_a_break() {
        let t = Instant::now();
        let mut c = Composer::new(Some(4.0));
        let mut ms = 0;
        for i in 0..100 {
            ms += 150;
            c.type_char('a', at(t, ms));
            if i % 10 == 0 {
                ms += 150;
                c.delete_char(at(t, ms));
            }
        }
        // 10 in 110 is under the 10% floor
        assert_eq!(c.nudge(), None);
        for _ in 0..10 {
            ms += 150;
            c.delete_char(at(t, ms));
            ms += 150;
            c.type_char('a', at(t, ms));
        }
        assert_eq!(c.nudge(), Some(Nudge::RoughPatch));
        // someone who usually corrects a lot isn't nagged for it
        c.usual = Some(15.0);
        assert_eq!(c.nudge(), None);
    }

    #[test]
    fn a_long_sitting_suggests_a_stretch() {
        let t = Instant::now();
        let mut c = Composer::new(None);
        let keys = (STRETCH_MINUTES * 60.0 / 0.5) as u64 + 2;
        for i in 0..keys {
            c.type_char('a', at(t, 500 * i));
        }
        assert_eq!(c.nudge(), Some(Nudge::Stretch));
        // a long pause starts a new sitting
        c.type_char('a', at(t, 500 * keys + SITTING_GAP.as_millis() as u64));
        assert_eq!(c.nudge(), None);
    }

    #[test]
    fn a_run_is_due_ten_minutes_after_it_began() {
        let t = Instant::now();
        let mut c = Composer::new(None);
        assert!(!c.due(at(t, 0)));
        c.type_char('a', t);
        assert!(!c.due(at(t, 60_000)));
        assert!(c.due(t + FLUSH_AFTER));
    }

    #[test]
    fn usual_is_the_mean_correction_rate_of_recent_runs() {
        assert_eq!(usual(&[]), None);
        let run = |acc| Run {
            acc,
            ..Run::default()
        };
        assert_eq!(usual(&[run(90.0), run(96.0)]), Some(7.0));
    }

    #[test]
    fn a_message_is_sent_as_one_paste() {
        assert_eq!(paste("a\nb"), b"\x1b[200~a\nb\x1b[201~\r");
    }

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn ctrl(c: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL)
    }

    fn draw(s: &Screen) -> String {
        use ratatui::{Terminal, backend::TestBackend};
        let mut t = Terminal::new(TestBackend::new(60, 4)).unwrap();
        t.draw(|f| s.render(f)).unwrap();
        let b = t.backend().buffer().clone();
        b.content()
            .chunks(60)
            .map(|row| row.iter().map(|c| c.symbol()).collect::<String>())
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn with(code: KeyCode, m: KeyModifiers) -> KeyEvent {
        KeyEvent::new(code, m)
    }

    #[test]
    fn enter_is_a_newline_and_ctrl_enter_sends() {
        let t = Instant::now();
        let mut s = Screen::new(Composer::new(None), "claude".into(), "ctrl-enter");
        let send = with(KeyCode::Enter, KeyModifiers::CONTROL);
        // nothing typed: no enter of any kind reaches claude
        assert_eq!(s.handle_key(key(KeyCode::Enter), t), Action::None);
        assert_eq!(s.handle_key(send, t), Action::None);
        assert!(s.composer.is_empty());
        for c in "hi".chars() {
            assert_eq!(s.handle_key(key(KeyCode::Char(c)), t), Action::None);
        }
        assert_eq!(s.handle_key(key(KeyCode::Enter), t), Action::None);
        s.handle_key(key(KeyCode::Char('x')), t);
        assert_eq!(s.handle_key(ctrl('j'), t), Action::None);
        s.handle_key(key(KeyCode::Char('y')), t);
        // with a message going, esc doesn't interrupt the other pane
        assert_eq!(s.handle_key(key(KeyCode::Esc), t), Action::None);
        assert_eq!(s.handle_key(send, t), Action::Send(paste("hi\nx\ny")));
        assert!(s.composer.is_empty());
        s.handle_key(key(KeyCode::Char('z')), t);
        assert_eq!(
            s.handle_key(with(KeyCode::Enter, KeyModifiers::ALT), t),
            Action::Send(paste("z"))
        );
        assert_eq!(s.handle_key(ctrl('c'), t), Action::Quit);
    }

    #[test]
    fn with_nothing_typed_only_safe_keys_pass_through() {
        let t = Instant::now();
        let mut s = Screen::new(Composer::new(None), "claude".into(), "ctrl-enter");
        assert_eq!(
            s.handle_key(key(KeyCode::Esc), t),
            Action::Send(b"\x1b".to_vec())
        );
        assert_eq!(
            s.handle_key(key(KeyCode::Up), t),
            Action::Send(b"\x1b[A".to_vec())
        );
        // shift-tab would change claude's permission mode
        assert_eq!(s.handle_key(key(KeyCode::BackTab), t), Action::None);
        assert_eq!(s.handle_key(key(KeyCode::Tab), t), Action::None);
        // a digit starts a message rather than picking a menu option
        assert_eq!(s.handle_key(key(KeyCode::Char('1')), t), Action::None);
        assert_eq!(typed(&s.composer), "1");
    }

    #[test]
    fn the_screen_shows_the_message_and_one_status_row() {
        let t = Instant::now();
        let mut s = Screen::new(Composer::new(None), "claude".into(), "ctrl-enter");
        let empty = draw(&s);
        assert!(empty.contains("ctrl-enter sends it to claude"));
        for c in "hello".chars() {
            s.handle_key(key(KeyCode::Char(c)), t);
        }
        let screen = draw(&s);
        let rows: Vec<&str> = screen.lines().collect();
        assert!(rows[0].starts_with("hello"));
        assert!(rows[3].starts_with("→ claude"));
        assert!(rows[3].contains("corrected"));
    }

    #[test]
    fn long_messages_wrap_and_keep_the_end_in_view() {
        let text: Vec<(char, bool)> = "abcdefgh\nij".chars().map(|c| (c, false)).collect();
        let lines = wrapped(&text, 4);
        let shown: Vec<String> = lines.iter().map(|l| l.to_string()).collect();
        assert_eq!(shown, ["abcd", "efgh", "ij "]);
    }
}
