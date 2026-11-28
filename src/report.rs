//! Plain-text explanations shared by the command line and the TUI.

use std::collections::BTreeMap;

use crate::drill::Mode;
use crate::keyboard::Snapshot;
use crate::store::{LetterStat, Progress, Run};
use crate::{coach, gate, layout};

/// The keyboard edits that turn `stage - 1` into `stage`.
pub fn edits(stage: usize) -> Vec<String> {
    let mut out = Vec::new();
    for c in layout::changes(stage) {
        let hold =
            layout::modifier(c.index).map_or(String::new(), |m| format!("  (keep the {m} hold)"));
        out.push(format!("key that types {} → {}{hold}", c.from, c.to));
    }
    for c in layout::changes(stage).iter().filter(|c| c.right_hand()) {
        out.push(format!(
            "mirror layer: key that types {} → {}",
            c.from, c.to
        ));
    }
    out
}

pub fn next_stage(current: usize, place: &str) -> Vec<String> {
    if current >= layout::last() {
        return vec!["You're on the last stage. Nothing left to edit.".into()];
    }
    let next = current + 1;
    let mut out = vec![
        format!("Stage {next}: {}", layout::name(next)),
        format!("Edit: {place}"),
    ];
    out.extend(edits(next).into_iter().map(|e| format!("  {e}")));
    out.push(format!(
        "  vim to practise: {}",
        layout::vim(next).join(" ")
    ));
    out
}

/// The same, squeezed into a few lines for the TUI.
pub fn next_stage_compact(current: usize, place: &str) -> Vec<String> {
    if current >= layout::last() {
        return vec!["You're on the last stage. Nothing left to edit.".into()];
    }
    let next = current + 1;
    let changes = layout::changes(next);
    let swap = |c: &layout::Change| format!("{}→{}", c.from, c.to);
    let keys: Vec<String> = changes
        .iter()
        .map(|c| match layout::modifier(c.index) {
            Some(m) => format!("{} (keep {m})", swap(c)),
            None => swap(c),
        })
        .collect();
    let mirror: Vec<String> = changes
        .iter()
        .filter(|c| c.right_hand())
        .map(swap)
        .collect();
    let mut out = vec![
        format!("Stage {next}: {}", layout::name(next)),
        format!("keys: {}", keys.join("  ")),
    ];
    if !mirror.is_empty() {
        out.push(format!("mirror: {}", mirror.join("  ")));
    }
    out.push(format!("vim: {}", layout::vim(next).join(" ")));
    out.push(place.to_string());
    out
}

/// What your keyboard's layout says it's on, measured against the stage you practise.
pub fn check(snap: &Snapshot, current: usize) -> Vec<String> {
    let mut out = vec![snap.source.clone()];
    let findings = snap.compare(current);
    match snap.stage() {
        Some(s) if s == current && findings.is_empty() => {
            out.push(format!(
                "Matches stage {current}: every key, hold and mirror key."
            ));
        }
        Some(s) if s == current + 1 && snap.compare(s).is_empty() => {
            out.push(format!(
                "Stage {s}'s edits are all in. Flash it and climb to stage {s}."
            ));
        }
        Some(s) if snap.compare(s).is_empty() => {
            out.push(format!(
                "Matches stage {s}, but you're practising stage {current}."
            ));
        }
        _ => {
            let target =
                if current < layout::last() && snap.compare(current + 1).len() < findings.len() {
                    current + 1
                } else {
                    current
                };
            let f = snap.compare(target);
            out.push(format!(
                "Closest to stage {target}, with {} difference(s):",
                f.len()
            ));
            out.extend(f.iter().take(8).map(|f| format!("  {f}")));
            if f.len() > 8 {
                out.push(format!("  and {} more", f.len() - 8));
            }
        }
    }
    out
}

/// One drill type in the session report.
#[derive(Debug, Clone, PartialEq)]
pub struct DrillSummary {
    pub mode: Mode,
    pub runs: usize,
    pub wpm: f64,
    pub acc: f64,
    /// Against recent sessions, or "first session".
    pub compared: String,
}

impl DrillSummary {
    /// "2 runs, 14.8 WPM at 96.1%"
    pub fn stats(&self) -> String {
        format!(
            "{} run{}, {:.1} WPM at {:.1}%",
            self.runs,
            if self.runs == 1 { "" } else { "s" },
            self.wpm,
            self.acc
        )
    }
}

/// The end of a guided session, in sections.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SessionReport {
    pub drills: Vec<DrillSummary>,
    /// "k 1625ms (earlier 1159ms)", slowest first.
    pub slowest: Vec<String>,
    /// Gate items still open for this stage; empty when they're all done.
    pub to_come: Vec<String>,
    /// The real-text outlook, from stage 1 on.
    pub trend: Option<String>,
}

impl SessionReport {
    pub fn is_empty(&self) -> bool {
        self.drills.is_empty() && self.to_come.is_empty() && self.trend.is_none()
    }

    /// The report as plain lines, one per section.
    pub fn lines(&self) -> Vec<String> {
        let mut out: Vec<String> = self
            .drills
            .iter()
            .map(|d| format!("{}: {} ({})", d.mode.label(), d.stats(), d.compared))
            .collect();
        if !self.slowest.is_empty() {
            out.push(format!("Slowest keys: {}", self.slowest.join(" · ")));
        }
        out.push(if self.to_come.is_empty() {
            "Every gate item for this stage is done.".to_string()
        } else {
            format!(
                "Still to come before the next stage: {}.",
                self.to_come.join("; ")
            )
        });
        out.extend(self.trend.clone());
        out
    }
}

/// The end of a guided session: how it went against the last few, and how
/// far off the next stage is. Runs from `block_start_ms` on are this session.
pub fn session(progress: &Progress, stage: usize, block_start_ms: u64) -> SessionReport {
    let at_stage: Vec<&Run> = progress.runs.iter().filter(|r| r.stage == stage).collect();
    let (earlier, now): (Vec<&Run>, Vec<&Run>) =
        at_stage.iter().partition(|r| r.t < block_start_ms);
    let earlier = coach::group(&earlier);
    let mut out = SessionReport::default();

    for mode in [Mode::Real, Mode::Words, Mode::Review] {
        let this: Vec<&Run> = now
            .iter()
            .copied()
            .filter(|r| r.mode == mode.id())
            .collect();
        if this.is_empty() {
            continue;
        }
        let (wpm, acc) = means(&this);
        let before: Vec<&Run> = earlier
            .iter()
            .filter(|g| g.iter().any(|r| r.mode == mode.id()))
            .rev()
            .take(3)
            .flat_map(|g| g.iter().copied().filter(|r| r.mode == mode.id()))
            .collect();
        let compared = if before.is_empty() {
            "first session".to_string()
        } else {
            let (w, a) = means(&before);
            format!(
                "{:+.1} WPM, {:+.1}% against recent sessions",
                wpm - w,
                acc - a
            )
        };
        out.drills.push(DrillSummary {
            mode,
            runs: this.len(),
            wpm,
            acc,
            compared,
        });
    }

    let mine = letters(&now);
    let before = letters(&earlier.iter().flatten().copied().collect::<Vec<_>>());
    let mut slow: Vec<(&String, f64)> = mine
        .iter()
        .filter(|(c, l)| c.as_str() != " " && l.n >= 3)
        .map(|(c, l)| (c, l.ms / l.n as f64))
        .collect();
    slow.sort_by(|a, b| b.1.total_cmp(&a.1));
    if !slow.is_empty() {
        out.slowest = slow
            .iter()
            .take(3)
            .map(|(c, ms)| match before.get(*c).filter(|l| l.n > 0) {
                Some(l) => format!("{c} {ms:.0}ms (earlier {:.0}ms)", l.ms / l.n as f64),
                None => format!("{c} {ms:.0}ms"),
            })
            .collect();
    }

    let items = gate::items(progress, stage);
    let left: Vec<&str> = items
        .iter()
        .filter(|i| !i.ok)
        .map(|i| i.label.as_str())
        .collect();
    out.to_come = left.iter().map(|l| l.to_string()).collect();

    let real_done = stage > 0
        && items
            .iter()
            .any(|i| i.label.starts_with("Real text") && i.ok);
    if real_done {
        out.trend = Some("Real text is there for this stage.".into());
    } else if stage > 0 {
        out.trend = Some(match progress.baseline() {
            None => "Record a baseline in stage 0 to see a real-text trend.".into(),
            Some(b) => trend(&earlier, &now, b * gate::SPEED_SHARE),
        });
    }
    out
}

fn means(runs: &[&Run]) -> (f64, f64) {
    let n = runs.len() as f64;
    (
        runs.iter().map(|r| r.wpm).sum::<f64>() / n,
        runs.iter().map(|r| r.acc).sum::<f64>() / n,
    )
}

fn letters(runs: &[&Run]) -> BTreeMap<String, LetterStat> {
    let mut out: BTreeMap<String, LetterStat> = BTreeMap::new();
    for r in runs {
        for (c, l) in &r.letters {
            out.entry(c.clone()).or_default().add(l);
        }
    }
    out
}

/// A session's best real-text run at the gate's accuracy.
fn best_real(runs: &[&Run]) -> Option<f64> {
    runs.iter()
        .filter(|r| r.mode == "real" && r.acc >= gate::REAL_ACCURACY)
        .map(|r| r.wpm)
        .reduce(f64::max)
}

/// How far real text has come this stage, from your best so far. The best
/// never goes backwards, so a slow session doesn't read as a step back.
fn trend(earlier: &[Vec<&Run>], now: &[&Run], target: f64) -> String {
    let before: Vec<f64> = earlier.iter().filter_map(|g| best_real(g)).collect();
    let today = best_real(now);
    let Some(best) = before.iter().copied().chain(today).reduce(f64::max) else {
        return "No real-text run at 95%+ yet this stage. It'll come.".into();
    };
    let start = before.first().copied().or(today).unwrap_or(best);
    let mut line = format!("Best so far this stage: {best:.0} WPM");
    if best.round() > start.round() {
        line += &format!(" (started at {start:.0})");
    }
    if best >= target {
        line += &format!(", past the {target:.0} WPM the gate asks for.");
    } else {
        line += &format!(" · {:.0} WPM to go to {target:.0}.", (target - best).ceil());
    }
    let previous = before.iter().copied().reduce(f64::max);
    if matches!((today, previous), (Some(t), Some(p)) if t > p) {
        line += " A new best today.";
    }
    line
}

/// Sessions with a real-text run at 95%+ before there's a trend to fit.
pub const TREND_SESSIONS: usize = 3;

/// What the real-text trend says about reaching `target`.
enum Fit {
    TooFew,
    Flat,
    Sessions(u64),
}

/// Sessions until the real-text trend at `stage` reaches the gate's speed, when
/// there's an upward trend to go on. The line goes through your best so far at
/// each session, so a slow session flattens it a little rather than pulling it
/// back.
pub fn sessions_to_gate(progress: &Progress, stage: usize) -> Option<u64> {
    let target = progress.baseline()? * gate::SPEED_SHARE;
    let runs: Vec<&Run> = progress
        .runs
        .iter()
        .filter(|r| r.stage == stage && r.mode == "real")
        .collect();
    let points: Vec<f64> = coach::group(&runs)
        .iter()
        .filter_map(|g| best_real(g))
        .scan(f64::MIN, |best, wpm| {
            *best = best.max(wpm);
            Some(*best)
        })
        .collect();
    let points = &points[points.len().saturating_sub(5)..];
    match fit(points, target) {
        Fit::Sessions(n) => Some(n),
        _ => None,
    }
}

/// A straight line through the best real-text speeds, one per session.
fn fit(points: &[f64], target: f64) -> Fit {
    if points.len() < TREND_SESSIONS {
        return Fit::TooFew;
    }
    let n = points.len() as f64;
    let mean_x = (n - 1.0) / 2.0;
    let mean_y = points.iter().sum::<f64>() / n;
    let (mut sxy, mut sxx) = (0.0, 0.0);
    for (i, y) in points.iter().enumerate() {
        let dx = i as f64 - mean_x;
        sxy += dx * (y - mean_y);
        sxx += dx * dx;
    }
    let slope = sxy / sxx;
    if slope <= 0.0 {
        return Fit::Flat;
    }
    let fitted = mean_y + slope * (n - 1.0 - mean_x);
    Fit::Sessions(((target - fitted) / slope).ceil().max(1.0) as u64)
}

/// How you're getting on, for a plan that stays on the layout you have.
#[derive(Debug, Default, PartialEq)]
pub struct Standing {
    /// Where you started: your first real-text runs, or the number typed in.
    pub baseline: Option<f64>,
    /// Your last few real-text runs: WPM, accuracy and how many.
    pub recent: Option<(f64, f64, usize)>,
    /// Your fastest real-text run at the gate's accuracy.
    pub best: Option<f64>,
    /// Your weakest keys lately, worst first: the key, its average ms and miss rate.
    pub weak: Vec<(char, f64, f64)>,
}

/// Real-text runs averaged for "lately".
pub const RECENT_REAL: usize = 5;

pub fn standing(progress: &Progress) -> Standing {
    let real = progress.runs_for(0, "real");
    let recent: Vec<&Run> = real.iter().rev().take(RECENT_REAL).copied().collect();
    let letters = crate::drill::recent_letters(0, progress);
    let mut trouble: Vec<(char, f64)> = crate::drill::trouble(0, progress).into_iter().collect();
    trouble.sort_by(|a, b| b.1.total_cmp(&a.1));
    let weak = trouble
        .into_iter()
        .take(5)
        .filter_map(|(c, _)| {
            let l = letters.get(&c.to_string())?;
            let tries = (l.n + l.miss).max(1) as f64;
            Some((c, l.average_ms()?, 100.0 * l.miss as f64 / tries))
        })
        .collect();
    Standing {
        baseline: progress.baseline(),
        recent: (!recent.is_empty()).then(|| {
            let (wpm, acc) = means(&recent);
            (wpm, acc, recent.len())
        }),
        best: real
            .iter()
            .filter(|r| r.acc >= gate::REAL_ACCURACY)
            .map(|r| r.wpm)
            .reduce(f64::max),
        weak,
    }
}

/// One line on the typing done for real through compose mode: how much this
/// week, how fast and how tidy, and against where it started once there's
/// more than a week of it. None until there's some.
pub fn real_use(progress: &Progress, now_ms: u64) -> Option<String> {
    const WEEK: u64 = 7 * 24 * 60 * 60 * 1000;
    let runs = &progress.freestyle;
    let first = runs.first()?.t;
    let weighted = |rs: &[&Run]| {
        let secs: f64 = rs.iter().map(|r| r.secs).sum();
        (secs > 0.0).then(|| {
            (
                rs.iter().map(|r| r.wpm * r.secs).sum::<f64>() / secs,
                rs.iter().map(|r| (100.0 - r.acc) * r.secs).sum::<f64>() / secs,
                secs,
            )
        })
    };
    let week: Vec<&Run> = runs.iter().filter(|r| r.t + WEEK >= now_ms).collect();
    let mut line = match weighted(&week) {
        Some((wpm, fixed, secs)) => format!(
            "Typing for real: {} this week · {wpm:.0} WPM · {fixed:.0}% corrected",
            duration(secs)
        ),
        None => {
            let last: Vec<&Run> = runs.iter().rev().take(5).collect();
            let (wpm, fixed, _) = weighted(&last)?;
            format!("Typing for real: none this week (lately {wpm:.0} WPM · {fixed:.0}% corrected)")
        }
    };
    if runs.last()?.t >= first + WEEK {
        let start: Vec<&Run> = runs.iter().filter(|r| r.t < first + WEEK).collect();
        if let Some((wpm, fixed, _)) = weighted(&start) {
            line.push_str(&format!(" · started at {wpm:.0} WPM, {fixed:.0}%"));
        }
    }
    Some(line)
}

fn duration(secs: f64) -> String {
    let mins = (secs / 60.0).round() as u64;
    if mins < 60 {
        format!("{} min", mins.max(1))
    } else {
        format!("{}h {:02}m", mins / 60, mins % 60)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn standing_sums_up_real_text_and_weak_keys() {
        let mut p = Progress::default();
        assert_eq!(standing(&p), Standing::default());
        let stat = |ms: f64, miss| LetterStat {
            n: 10,
            miss,
            ms: ms * 10.0,
        };
        for (wpm, acc) in [(40.0, 97.0), (44.0, 90.0), (46.0, 96.0)] {
            p.runs.push(Run {
                stage: 0,
                mode: "real".into(),
                wpm,
                acc,
                letters: BTreeMap::from([
                    ("a".to_string(), stat(250.0, 0)),
                    ("b".to_string(), stat(260.0, 0)),
                    ("c".to_string(), stat(270.0, 0)),
                    ("k".to_string(), stat(900.0, 1)),
                ]),
                ..Run::default()
            });
        }
        let st = standing(&p);
        let (wpm, acc, n) = st.recent.unwrap();
        assert_eq!(n, 3);
        assert!((wpm - 130.0 / 3.0).abs() < 0.01 && (acc - 283.0 / 3.0).abs() < 0.01);
        assert_eq!(st.best, Some(46.0), "the 90% run doesn't count");
        assert_eq!(st.weak[0].0, 'k');
        assert!((st.weak[0].1 - 900.0).abs() < 0.01 && st.weak[0].2 > 0.0);
    }

    #[test]
    fn stage_two_edits_keep_holds_and_list_mirror_keys() {
        let e = edits(2);
        assert!(
            e.contains(&"key that types k → o  (keep the alt hold)".to_string()),
            "{e:?}"
        );
        assert_eq!(e.iter().filter(|l| l.starts_with("mirror")).count(), 3);
    }

    #[test]
    fn nothing_is_left_after_the_last_stage() {
        assert_eq!(next_stage(layout::last(), "u").len(), 1);
        assert_eq!(next_stage(0, "u")[1], "Edit: u");
    }

    fn run(t: u64, mode: &str, wpm: f64, acc: f64) -> Run {
        Run {
            t,
            stage: 1,
            mode: mode.into(),
            wpm,
            acc,
            secs: 60.0,
            ..Run::default()
        }
    }

    const HOUR: u64 = 3_600_000;
    const START: u64 = 1_000 * HOUR;

    fn progress(runs: Vec<Run>) -> Progress {
        let mut p = Progress {
            stage: 1,
            runs,
            ..Progress::default()
        };
        p.manual_mut(0).wpm = 30.0;
        p
    }

    fn calm(lines: &[String]) {
        for l in lines {
            for w in ["only", "behind", "hurry", "must", "should have"] {
                assert!(!l.contains(w), "{w:?} in {l}");
            }
        }
    }

    #[test]
    fn a_first_session_has_nothing_to_compare() {
        let p = progress(vec![
            run(START + 60_000, "real", 12.0, 96.0),
            run(START + 120_000, "words", 13.0, 98.0),
        ]);
        let r = session(&p, 1, START).lines();
        assert!(
            r[0].starts_with("real text 60s: 1 run, 12.0 WPM at 96.0% (first session)"),
            "{r:?}"
        );
        assert!(
            r[1].starts_with("new keys: 1 run, 13.0 WPM at 98.0% (first session)"),
            "{r:?}"
        );
        assert!(
            r.iter()
                .any(|l| l == "Best so far this stage: 12 WPM · 15 WPM to go to 27."),
            "{r:?}"
        );
        assert!(r.iter().any(|l| l.starts_with("Still to come")), "{r:?}");
        calm(&r);
    }

    #[test]
    fn compares_with_recent_sessions() {
        let p = progress(vec![
            run(START - 5 * HOUR, "words", 10.0, 97.0),
            run(START - 3 * HOUR, "words", 12.0, 99.0),
            run(START + 60_000, "words", 13.0, 98.5),
        ]);
        let r = session(&p, 1, START).lines();
        assert!(
            r[0].contains("(+2.0 WPM, +0.5% against recent sessions)"),
            "{r:?}"
        );
    }

    #[test]
    fn names_the_slowest_keys() {
        let mut now = run(START + 60_000, "words", 13.0, 98.0);
        for (c, ms) in [
            ("e", 4_000.0),
            ("l", 3_000.0),
            ("t", 1_500.0),
            ("a", 9_000.0),
            (" ", 9_000.0),
        ] {
            let n = if c == "a" { 2 } else { 4 };
            now.letters.insert(c.into(), LetterStat { n, miss: 0, ms });
        }
        let mut before = run(START - 3 * HOUR, "words", 12.0, 98.0);
        before.letters.insert(
            "e".into(),
            LetterStat {
                n: 4,
                miss: 0,
                ms: 6_000.0,
            },
        );
        let r = session(&progress(vec![before, now]), 1, START).lines();
        let line = r.iter().find(|l| l.starts_with("Slowest keys")).unwrap();
        assert_eq!(
            line,
            "Slowest keys: e 1000ms (earlier 1500ms) · l 750ms · t 375ms"
        );
    }

    #[test]
    fn real_text_shows_the_best_so_far_and_the_gap() {
        let p = progress(vec![
            run(START - 6 * HOUR, "real", 16.0, 96.0),
            run(START - 3 * HOUR, "real", 17.0, 97.0),
            run(START + 60_000, "real", 18.0, 96.0),
        ]);
        let r = session(&p, 1, START).lines();
        assert_eq!(
            r.last().unwrap(),
            "Best so far this stage: 18 WPM (started at 16) · 9 WPM to go to 27. A new best today."
        );
        calm(&r);
    }

    #[test]
    fn a_slow_session_does_not_read_as_a_step_back() {
        let p = progress(vec![
            run(START - 9 * HOUR, "real", 16.0, 96.0),
            run(START - 6 * HOUR, "real", 20.0, 97.0),
            run(START - 3 * HOUR, "real", 22.0, 96.0),
            run(START + 60_000, "real", 15.0, 95.0),
        ]);
        let r = session(&p, 1, START).lines();
        assert_eq!(
            r.last().unwrap(),
            "Best so far this stage: 22 WPM (started at 16) · 5 WPM to go to 27."
        );
        calm(&r);
    }

    #[test]
    fn a_slow_session_barely_moves_the_forecast() {
        let rising = vec![
            run(START - 12 * HOUR, "real", 16.0, 96.0),
            run(START - 9 * HOUR, "real", 18.0, 96.0),
            run(START - 6 * HOUR, "real", 20.0, 97.0),
            run(START - 3 * HOUR, "real", 22.0, 96.0),
        ];
        let before = sessions_to_gate(&progress(rising.clone()), 1).unwrap();
        let mut tired = rising;
        tired.push(run(START, "real", 15.0, 95.0));
        let after = sessions_to_gate(&progress(tired), 1).unwrap();
        assert!(
            after >= before && after <= before + 2,
            "{before} -> {after}"
        );
    }

    #[test]
    fn no_accurate_real_text_yet() {
        let p = progress(vec![run(START + 60_000, "real", 18.0, 90.0)]);
        let r = session(&p, 1, START).lines();
        assert!(r.last().unwrap().contains("No real-text run at 95%+ yet"));
        calm(&r);
    }

    #[test]
    fn real_use_sums_up_the_week_against_the_start() {
        const DAY: u64 = 24 * 60 * 60 * 1000;
        let run = |t, wpm, acc, secs| Run {
            t,
            mode: "compose".into(),
            wpm,
            acc,
            secs,
            ..Run::default()
        };
        let mut p = Progress::default();
        assert_eq!(real_use(&p, 0), None);
        p.freestyle = vec![run(DAY, 30.0, 90.0, 600.0)];
        assert_eq!(
            real_use(&p, DAY).unwrap(),
            "Typing for real: 10 min this week · 30 WPM · 10% corrected"
        );
        assert_eq!(
            real_use(&p, 20 * DAY).unwrap(),
            "Typing for real: none this week (lately 30 WPM · 10% corrected)"
        );
        p.freestyle.push(run(20 * DAY, 40.0, 96.0, 3000.0));
        p.freestyle.push(run(20 * DAY + 1, 34.0, 94.0, 1500.0));
        assert_eq!(
            real_use(&p, 20 * DAY).unwrap(),
            "Typing for real: 1h 15m this week · 38 WPM · 5% corrected · started at 30 WPM, 10%"
        );
    }
}
