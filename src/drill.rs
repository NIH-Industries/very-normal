//! The drills, and the typing engine that scores them.

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use crate::layout;
use crate::store::{LetterStat, Progress};

/// How long a pause before the next key lights up on the map.
pub const HINT_AFTER: Duration = Duration::from_millis(1200);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Recall,
    Words,
    Review,
    Real,
    Links,
    Mirror,
}

impl Mode {
    pub const ALL: [Mode; 6] = [
        Mode::Recall,
        Mode::Words,
        Mode::Review,
        Mode::Real,
        Mode::Links,
        Mode::Mirror,
    ];

    /// The name stored with each run, shared with the web tutor.
    pub fn id(self) -> &'static str {
        match self {
            Mode::Recall => "recall",
            Mode::Words => "words",
            Mode::Review => "review",
            Mode::Real => "real",
            Mode::Links => "links",
            Mode::Mirror => "mirror",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Mode::Recall => "key recall",
            Mode::Words if layout::staying() => "weak keys",
            Mode::Words => "new keys",
            Mode::Review => "review",
            Mode::Real => "real text 60s",
            Mode::Links => "markdown links",
            Mode::Mirror => "mirror (bonus)",
        }
    }

    pub fn label_for(id: &str) -> &'static str {
        Mode::ALL
            .iter()
            .find(|m| m.id() == id)
            .map(|m| m.label())
            .unwrap_or("other")
    }

    pub fn time_limit(self) -> Option<Duration> {
        (self == Mode::Real).then(|| Duration::from_secs(60))
    }

    pub fn describe(self, stage: usize) -> String {
        if layout::staying() {
            return self.describe_staying();
        }
        match self {
            Mode::Recall if stage == 0 => "Single home-row keys. Pick a stage to drill its moved keys.".into(),
            Mode::Recall => format!(
                "One key at a time from this stage's moves: {}. Keys you've missed or hesitated on lately come back more often.",
                layout::focus(stage).iter().map(char::to_string).collect::<Vec<_>>().join(" ")
            ),
            Mode::Words if stage == 0 => "Common words from your own typing.".into(),
            Mode::Words => "30 words you actually type, heavy in this stage's moved keys. The gate counts this one.".into(),
            Mode::Review if stage < 2 => "Same as new keys until stage 2.".into(),
            Mode::Review => format!("Words from every stage so far (1 to {stage}), so earlier swaps stay sharp."),
            Mode::Real if stage == 0 => "60 seconds of your most common words. Three of these set your baseline.".into(),
            Mode::Real => "60 seconds of your most common words. The gate compares this with your baseline.".into(),
            Mode::Links => "[text](link) shapes for your symbol layers. The gate ignores it.".into(),
            Mode::Mirror => "Left hand only. Hold the mirror key for right-hand letters; the second letter on each left key is what it types then. The gate ignores it.".into(),
        }
    }
}

impl Mode {
    /// What each drill is for on a plan that stays on the layout you have.
    fn describe_staying(self) -> String {
        match self {
            Mode::Recall => "One key at a time from the whole board. Keys you've missed or hesitated on lately come back more often.".into(),
            Mode::Words => "30 everyday words, leaning on the keys you've been slowest on or missed most lately.".into(),
            Mode::Review => "Same as weak keys: with no keys moving, there's nothing older to review.".into(),
            Mode::Real => "60 seconds of everyday words, unweighted. The first three set your baseline; the rest show how you're getting on.".into(),
            Mode::Links => "[text](link) shapes for your symbol layers.".into(),
            Mode::Mirror => "Left hand only. Hold the mirror key for right-hand letters; the second letter on each left key is what it types then.".into(),
        }
    }
}

/// A small xorshift generator: plenty for shuffling drill words.
pub struct Rng(u64);

impl Rng {
    pub fn new() -> Rng {
        let seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0x9e37_79b9_7f4a_7c15);
        Rng::seeded(seed)
    }

    pub fn seeded(seed: u64) -> Rng {
        Rng(seed | 1)
    }

    pub fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }

    /// A float in [0, 1).
    pub fn unit(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }

    pub fn below(&mut self, n: usize) -> usize {
        (self.unit() * n as f64) as usize
    }

    pub fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        &items[self.below(items.len())]
    }
}

impl Default for Rng {
    fn default() -> Self {
        Rng::new()
    }
}

/// The stage's own word list, or common words that use its moved keys. On a
/// plan that stays put, every everyday word: the weighting finds the weak keys.
fn stage_words(stage: usize) -> Vec<String> {
    if layout::staying() {
        return layout::common_words().to_vec();
    }
    let own = layout::words(stage);
    if stage > 0 && !own.is_empty() {
        return own.to_vec();
    }
    let focus = layout::focus(stage);
    let common = layout::common_words();
    let picked: Vec<String> = common
        .iter()
        .filter(|w| w.chars().any(|c| focus.contains(&c)))
        .take(80)
        .cloned()
        .collect();
    if picked.len() >= 10 {
        picked
    } else {
        common.iter().take(200).cloned().collect()
    }
}

/// How much more often a word with your worst key comes up than one without.
pub const LEAN: f64 = 3.0;

/// How far back a key's speed and misses count: about two sessions of this
/// stage, so a key you've since learned stops getting extra reps, and one
/// that's started tripping you up gets them soon.
pub const RECENT_RUNS: usize = 12;

/// A key at least this far from the middle to the worst counts as a hard one:
/// the drill leans on it, and its words are spread out.
const HARD: f64 = 0.3;

/// Each key's speed and misses over this stage's last [`RECENT_RUNS`] runs,
/// or over every run when none of those kept their per-key timings.
pub fn recent_letters(stage: usize, progress: &Progress) -> BTreeMap<String, LetterStat> {
    let mut out: BTreeMap<String, LetterStat> = BTreeMap::new();
    for run in progress
        .runs
        .iter()
        .rev()
        .filter(|r| r.stage == stage && !r.letters.is_empty())
        .take(RECENT_RUNS)
    {
        for (ch, stat) in &run.letters {
            out.entry(ch.clone()).or_default().add(stat);
        }
    }
    if out.is_empty() {
        progress.letters(stage)
    } else {
        out
    }
}

/// Keys that need more work than the stage's middle key lately: slower, or
/// missed more, scored from just above the middle (near 0) to the worst (1).
/// Keys pressed fewer than five times aren't judged yet.
pub fn trouble(stage: usize, progress: &Progress) -> BTreeMap<char, f64> {
    let scores: Vec<(char, f64)> = recent_letters(stage, progress)
        .iter()
        .filter_map(|(c, s)| {
            let ch = c.chars().next().filter(|ch| *ch != ' ')?;
            let tries = s.n + s.miss;
            (s.n > 0 && tries >= 5).then(|| {
                let miss_rate = s.miss as f64 / tries as f64;
                (ch, s.ms / s.n as f64 * (1.0 + 10.0 * miss_rate))
            })
        })
        .collect();
    if scores.len() < 3 {
        return BTreeMap::new();
    }
    let mut sorted: Vec<f64> = scores.iter().map(|(_, v)| *v).collect();
    sorted.sort_by(f64::total_cmp);
    let (mid, worst) = (sorted[sorted.len() / 2], sorted[sorted.len() - 1]);
    if worst <= mid {
        return BTreeMap::new();
    }
    scores
        .into_iter()
        .filter(|(_, v)| *v > mid)
        .map(|(c, v)| (c, (v - mid) / (worst - mid)))
        .collect()
}

/// The keys a words drill leans on, worst first: at most three, and only
/// ones clearly behind the rest. None when it's going gently.
pub fn leaning(mode: Mode, stage: usize, progress: &Progress, gentle: bool) -> Vec<char> {
    if gentle || !matches!(mode, Mode::Words | Mode::Review) {
        return Vec::new();
    }
    let mut keys: Vec<(char, f64)> = trouble(stage, progress)
        .into_iter()
        .filter(|(_, t)| *t >= HARD)
        .collect();
    keys.sort_by(|a, b| b.1.total_cmp(&a.1));
    keys.into_iter().take(3).map(|(c, _)| c).collect()
}

/// Words with troublesome keys come up more often, up to `LEAN` times, so
/// they get extra reps without the whole drill turning into them. Hard words
/// are spread out: never two in a row, or three when they're most of the pool,
/// so the misses don't pile up.
fn words_from(pool: &[String], n: usize, trouble: &BTreeMap<char, f64>, rng: &mut Rng) -> String {
    let worst: Vec<f64> = pool
        .iter()
        .map(|w| {
            w.chars()
                .filter_map(|c| trouble.get(&c))
                .fold(0.0, |a: f64, t| a.max(*t))
        })
        .collect();
    let weights: Vec<f64> = worst.iter().map(|t| 1.0 + (LEAN - 1.0) * t).collect();
    let hard: Vec<bool> = worst.iter().map(|t| *t >= HARD).collect();
    let hard_count = hard.iter().filter(|h| **h).count();
    let easy_count = pool.len() - hard_count;
    let max_run = if easy_count >= hard_count { 1 } else { 2 };
    let pick = |rng: &mut Rng, easy_only: bool| -> usize {
        let allowed = |i: usize| !(easy_only && hard[i]);
        let total: f64 = (0..pool.len())
            .filter(|i| allowed(*i))
            .map(|i| weights[i])
            .sum();
        let mut t = total * rng.unit();
        let mut last = 0;
        for i in (0..pool.len()).filter(|i| allowed(*i)) {
            last = i;
            t -= weights[i];
            if t <= 0.0 {
                return i;
            }
        }
        last
    };
    let mut run = 0;
    let mut out = Vec::with_capacity(n);
    for _ in 0..n {
        let i = pick(rng, easy_count > 0 && run >= max_run);
        run = if hard[i] { run + 1 } else { 0 };
        out.push(pool[i].as_str());
    }
    out.join(" ")
}

/// Keys you've missed or been slow on lately get picked more often, unless
/// it's going gently, when every key comes up as often.
fn weighted_char(stage: usize, progress: &Progress, gentle: bool, rng: &mut Rng) -> char {
    let mut focus = layout::focus(stage);
    if focus.is_empty() && layout::staying() {
        focus = layout::keys(stage);
    } else if focus.is_empty() {
        focus = "asdfjkl;".chars().collect();
    }
    let stats = recent_letters(stage, progress);
    let weights: Vec<f64> = focus
        .iter()
        .map(|c| match stats.get(&c.to_string()) {
            _ if gentle => 1.0,
            Some(s) if s.n > 0 => {
                1.0 + 12.0 * (s.miss as f64 / s.n as f64) + s.ms / s.n as f64 / 350.0
            }
            _ => 3.0,
        })
        .collect();
    let mut t = weights.iter().sum::<f64>() * rng.unit();
    for (c, w) in focus.iter().zip(&weights) {
        t -= w;
        if t <= 0.0 {
            return *c;
        }
    }
    focus[0]
}

/// A drill's text. `gentle` (after a rough run) drops the extra reps on hard
/// keys, so the next drill is easy wins; real text is never weighted.
pub fn build(mode: Mode, stage: usize, progress: &Progress, gentle: bool, rng: &mut Rng) -> String {
    let common = layout::common_words();
    let trouble = if gentle {
        BTreeMap::new()
    } else {
        trouble(stage, progress)
    };
    match mode {
        Mode::Recall => {
            let several = layout::focus(stage).len() > 1;
            let mut out = String::new();
            let mut prev = None;
            while out.chars().count() < 40 {
                let c = weighted_char(stage, progress, gentle, rng);
                if several && Some(c) == prev {
                    continue;
                }
                out.push(c);
                prev = Some(c);
            }
            out
        }
        Mode::Words => words_from(&stage_words(stage), 30, &trouble, rng),
        Mode::Review => {
            let pool: Vec<String> = if stage < 2 {
                stage_words(stage)
            } else {
                (1..=stage).flat_map(stage_words).collect()
            };
            words_from(&pool, 30, &trouble, rng)
        }
        Mode::Real => (0..260)
            .map(|_| common[(rng.unit().powf(2.2) * common.len() as f64) as usize].as_str())
            .collect::<Vec<_>>()
            .join(" "),
        Mode::Links => {
            let top = &common[..300.min(common.len())];
            (0..10)
                .map(|_| {
                    let (a, b, c) = (rng.pick(top), rng.pick(top), rng.pick(top));
                    if rng.unit() < 0.5 {
                        format!("see [{a} {b}]({c}.md)")
                    } else {
                        format!("[{a}](https://{b}.dev/{c})")
                    }
                })
                .collect::<Vec<_>>()
                .join(" ")
        }
        Mode::Mirror => {
            let pool: Vec<String> = common
                .iter()
                .take(150)
                .filter(|w| w.len() >= 2 && w.chars().all(|c| c.is_ascii_lowercase()))
                .cloned()
                .collect();
            words_from(&pool, 20, &BTreeMap::new(), rng)
        }
    }
}

pub struct Drill {
    pub mode: Mode,
    pub stage: usize,
    pub text: Vec<char>,
    pub pos: usize,
    pub keys: u64,
    pub misses: u64,
    pub error: bool,
    pub done: bool,
    started: Option<Instant>,
    last_key: Option<Instant>,
    last_progress: Instant,
    finished_at: Option<Instant>,
    log: BTreeMap<String, LetterStat>,
    pairs: BTreeMap<String, LetterStat>,
    typos: BTreeMap<String, u64>,
}

impl Drill {
    pub fn new(mode: Mode, stage: usize, text: String, now: Instant) -> Drill {
        Drill {
            mode,
            stage,
            text: text.chars().collect(),
            pos: 0,
            keys: 0,
            misses: 0,
            error: false,
            done: false,
            started: None,
            last_key: None,
            last_progress: now,
            finished_at: None,
            log: BTreeMap::new(),
            pairs: BTreeMap::new(),
            typos: BTreeMap::new(),
        }
    }

    pub fn expected(&self) -> Option<char> {
        (!self.done)
            .then(|| self.text.get(self.pos).copied())
            .flatten()
    }

    pub fn started(&self) -> bool {
        self.started.is_some()
    }

    /// Restarts the hint clock, for when the drill gets focus.
    pub fn touch(&mut self, now: Instant) {
        self.last_progress = now;
    }

    /// Wrong keys don't advance; they count as a miss on the key you wanted.
    pub fn press(&mut self, c: char, now: Instant) {
        if self.done {
            return;
        }
        let Some(want) = self.expected() else { return };
        if self.started.is_none() {
            self.started = Some(now);
            self.last_key = Some(now);
        }
        self.keys += 1;
        // the key before, when both are letters or symbols: a pair to time
        let pair = self
            .pos
            .checked_sub(1)
            .map(|i| self.text[i])
            .filter(|p| *p != ' ' && want != ' ')
            .map(|p| format!("{p}{want}"));
        let stat = self.log.entry(want.to_string()).or_default();
        if c == want {
            if want != ' ' && self.pos > 0 {
                let gap = now - self.last_key.unwrap_or(now);
                let ms = gap.min(Duration::from_secs(3)).as_secs_f64() * 1000.0;
                stat.n += 1;
                stat.ms += ms;
                if let Some(pair) = pair {
                    let p = self.pairs.entry(pair).or_default();
                    p.n += 1;
                    p.ms += ms;
                }
            }
            self.pos += 1;
            self.last_key = Some(now);
            self.last_progress = now;
            self.error = false;
            if self.pos >= self.text.len() {
                self.finish(now);
            }
        } else {
            self.misses += 1;
            stat.miss += 1;
            self.error = true;
            *self.typos.entry(format!("{want}>{c}")).or_default() += 1;
            if let Some(pair) = pair {
                self.pairs.entry(pair).or_default().miss += 1;
            }
        }
    }

    /// Ends a timed drill once its time is up.
    pub fn tick(&mut self, now: Instant) {
        if let (Some(limit), false) = (self.mode.time_limit(), self.done)
            && self.elapsed(now) >= limit
        {
            self.finish(now);
        }
    }

    fn finish(&mut self, now: Instant) {
        self.done = true;
        self.finished_at = Some(now);
    }

    pub fn elapsed(&self, now: Instant) -> Duration {
        match self.started {
            Some(s) => self.finished_at.unwrap_or(now) - s,
            None => Duration::ZERO,
        }
    }

    pub fn remaining(&self, now: Instant) -> Option<Duration> {
        self.mode
            .time_limit()
            .map(|l| l.saturating_sub(self.elapsed(now)))
    }

    pub fn wpm(&self, now: Instant) -> f64 {
        let minutes = self.elapsed(now).as_secs_f64().max(1.0) / 60.0;
        self.pos as f64 / 5.0 / minutes
    }

    pub fn accuracy(&self) -> f64 {
        if self.keys == 0 {
            100.0
        } else {
            100.0 * (self.keys - self.misses) as f64 / self.keys as f64
        }
    }

    pub fn hesitating(&self, now: Instant) -> bool {
        !self.done && now - self.last_progress >= HINT_AFTER
    }

    pub fn letters(&self) -> &BTreeMap<String, LetterStat> {
        &self.log
    }

    /// Timings for each pair of keys typed one after the other.
    pub fn pairs(&self) -> &BTreeMap<String, LetterStat> {
        &self.pairs
    }

    /// Wrong keys, as "wanted>typed".
    pub fn typos(&self) -> &BTreeMap<String, u64> {
        &self.typos
    }

    /// The slowest keys of this drill, for the recall summary.
    pub fn slowest(&self, n: usize) -> Vec<(char, f64, u64)> {
        let mut v: Vec<(char, f64, u64)> = self
            .log
            .iter()
            .filter_map(|(c, s)| {
                let ch = c.chars().next()?;
                Some((ch, s.average_ms()?, s.miss))
            })
            .collect();
        v.sort_by(|a, b| b.1.total_cmp(&a.1));
        v.truncate(n);
        v
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(start: Instant, ms: u64) -> Instant {
        start + Duration::from_millis(ms)
    }

    #[test]
    fn wrong_keys_count_as_misses_and_do_not_advance() {
        let t0 = Instant::now();
        let mut d = Drill::new(Mode::Words, 1, "ab".into(), t0);
        d.press('x', at(t0, 0));
        assert_eq!((d.pos, d.misses, d.error), (0, 1, true));
        d.press('a', at(t0, 100));
        d.press('b', at(t0, 400));
        assert!(d.done);
        assert_eq!(d.keys, 3);
        assert!((d.accuracy() - 66.666).abs() < 0.01);
        assert_eq!(d.letters()["b"].ms, 300.0);
        assert_eq!(d.letters()["a"].miss, 1);
        assert_eq!(d.typos()["a>x"], 1);
    }

    #[test]
    fn pairs_time_the_second_key_and_skip_spaces() {
        let t0 = Instant::now();
        let mut d = Drill::new(Mode::Words, 1, "th e".into(), t0);
        d.press('t', at(t0, 0));
        d.press('y', at(t0, 50));
        d.press('h', at(t0, 200));
        d.press(' ', at(t0, 300));
        d.press('e', at(t0, 450));
        assert_eq!(d.pairs().len(), 1);
        let th = d.pairs()["th"];
        assert_eq!((th.n, th.miss, th.ms), (1, 1, 200.0));
    }

    #[test]
    fn timed_drills_stop_at_the_limit() {
        let t0 = Instant::now();
        let mut d = Drill::new(Mode::Real, 0, "a".repeat(500), t0);
        d.press('a', t0);
        d.tick(at(t0, 59_000));
        assert!(!d.done);
        d.tick(at(t0, 60_000));
        assert!(d.done);
        assert_eq!(d.elapsed(at(t0, 90_000)), Duration::from_secs(60));
    }

    #[test]
    fn wpm_is_characters_over_five_per_minute() {
        let t0 = Instant::now();
        let mut d = Drill::new(Mode::Words, 1, "hello world".into(), t0);
        for (i, c) in "hello world".chars().enumerate() {
            d.press(c, at(t0, i as u64 * 1200));
        }
        // 11 characters over 12 seconds
        assert!((d.wpm(at(t0, 99_000)) - 11.0).abs() < 0.01);
    }

    #[test]
    fn the_hint_shows_after_a_pause() {
        let t0 = Instant::now();
        let d = Drill::new(Mode::Recall, 1, "e".into(), t0);
        assert!(!d.hesitating(at(t0, 1000)));
        assert!(d.hesitating(at(t0, 1300)));
    }

    #[test]
    fn recall_only_uses_the_stage_keys() {
        let mut rng = Rng::seeded(42);
        let text = build(Mode::Recall, 3, &Progress::default(), false, &mut rng);
        let focus = layout::focus(3);
        assert_eq!(text.chars().count(), 40);
        assert!(text.chars().all(|c| focus.contains(&c)));
        assert!(text.chars().zip(text.chars().skip(1)).all(|(a, b)| a != b));
    }

    fn with_letters(stats: &[(char, u64, u64, f64)]) -> Progress {
        let mut p = Progress::default();
        let stage = p.letters.entry("1".into()).or_default();
        for (c, n, miss, avg) in stats {
            stage.insert(
                c.to_string(),
                LetterStat {
                    n: *n,
                    miss: *miss,
                    ms: avg * *n as f64,
                },
            );
        }
        p
    }

    #[test]
    fn trouble_scores_slow_and_missed_keys_above_the_middle() {
        let p = with_letters(&[
            ('a', 20, 0, 250.0),
            ('b', 20, 0, 260.0),
            ('c', 20, 0, 270.0),
            ('k', 20, 0, 2000.0),
            ('i', 20, 2, 600.0),
            ('z', 2, 2, 5000.0), // too few to judge
        ]);
        let t = trouble(1, &p);
        assert_eq!(t[&'k'], 1.0);
        assert!(t[&'i'] > 0.4 && t[&'i'] < 1.0, "{t:?}");
        assert!(!t.contains_key(&'a') && !t.contains_key(&'z'));
        assert_eq!(leaning(Mode::Words, 1, &p, false), vec!['k', 'i']);
        assert!(leaning(Mode::Words, 1, &p, true).is_empty(), "gently");
        assert!(leaning(Mode::Real, 1, &p, false).is_empty());
        assert!(trouble(2, &p).is_empty());
    }

    #[test]
    fn words_with_the_worst_key_come_up_more_often_but_never_twice_in_a_row() {
        let pool: Vec<String> = ["ok", "no", "an", "so"].map(String::from).into();
        let t = BTreeMap::from([('k', 1.0)]);
        let text = words_from(&pool, 6000, &t, &mut Rng::seeded(3));
        let words: Vec<&str> = text.split(' ').collect();
        assert!(words.windows(2).all(|w| w != ["ok", "ok"]), "spread out");
        // half the picks after an easy word, none straight after itself: a third
        let share = words.iter().filter(|w| **w == "ok").count() as f64 / 6000.0;
        assert!((share - 1.0 / 3.0).abs() < 0.03, "{share}");
        let plain = words_from(&pool, 6000, &BTreeMap::new(), &mut Rng::seeded(3));
        let share = plain.split(' ').filter(|w| *w == "ok").count() as f64 / 6000.0;
        assert!((share - 0.25).abs() < 0.03, "no lean: {share}");
    }

    #[test]
    fn hard_words_come_two_at_most_when_theyre_most_of_the_pool() {
        let pool: Vec<String> = ["ok", "kin", "no"].map(String::from).into();
        let t = BTreeMap::from([('k', 1.0)]);
        let text = words_from(&pool, 3000, &t, &mut Rng::seeded(5));
        let words: Vec<&str> = text.split(' ').collect();
        assert!(words.windows(3).all(|w| w.contains(&"no")));
        assert!(words.windows(2).any(|w| w.iter().all(|w| *w != "no")));
    }

    #[test]
    fn trouble_follows_recent_runs_not_the_whole_stage() {
        let mut p = with_letters(&[
            ('a', 20, 0, 250.0),
            ('b', 20, 0, 260.0),
            ('c', 20, 0, 270.0),
            ('k', 200, 0, 2000.0),
        ]);
        assert!(
            trouble(1, &p).contains_key(&'k'),
            "all-time until runs have timings"
        );
        let stat = |ms: f64| LetterStat {
            n: 10,
            miss: 0,
            ms: ms * 10.0,
        };
        p.runs.push(crate::store::Run {
            stage: 1,
            letters: BTreeMap::from([
                ("a".to_string(), stat(250.0)),
                ("b".to_string(), stat(260.0)),
                ("c".to_string(), stat(900.0)),
                ("k".to_string(), stat(255.0)),
            ]),
            ..Default::default()
        });
        let t = trouble(1, &p);
        assert!(!t.contains_key(&'k'), "learned since: {t:?}");
        assert_eq!(t.get(&'c'), Some(&1.0), "newly slow: {t:?}");
    }

    #[test]
    fn recall_goes_evenly_when_gentle() {
        let p = with_letters(&[('a', 20, 0, 250.0)]);
        let focus = layout::focus(1);
        let mut rng = Rng::seeded(9);
        let picks: Vec<char> = (0..4000)
            .map(|_| weighted_char(1, &p, true, &mut rng))
            .collect();
        for c in &focus {
            let share = picks.iter().filter(|p| *p == c).count() as f64 / 4000.0;
            assert!(
                (share - 1.0 / focus.len() as f64).abs() < 0.04,
                "{c}: {share}"
            );
        }
    }

    #[test]
    fn every_mode_builds_text() {
        let mut rng = Rng::seeded(7);
        for stage in 0..=layout::last() {
            for mode in Mode::ALL {
                assert!(!build(mode, stage, &Progress::default(), false, &mut rng).is_empty());
            }
        }
    }
}
