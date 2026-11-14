//! The whole plan at a glance: what each stage moves, how long the finished
//! ones took, and a deliberately vague guess at the rest.
//!
//! A stage's weight is how much of your typing it moves: the share of letters
//! typed (by English letter frequency) whose key changes at that stage.
//!
//! Guesses wait for your own data. The stage you're on gets one once its
//! real-text trend has enough sessions to go on. Stages ahead get one once a
//! stage is behind you, from the practice minutes per point of weight it took.

use crate::coach;
use crate::layout;
use crate::report;
use crate::store::{Progress, Run};

/// Practice minutes a day when there's nothing to judge by: two sessions.
const DEFAULT_MINUTES_PER_DAY: f64 = 20.0;
/// Days of recent practice that set the daily pace.
const PACE_WINDOW_DAYS: f64 = 14.0;
/// The range shown around a guess.
const LOW: f64 = 0.6;
const HIGH: f64 = 1.6;

const DAY_MS: f64 = 86_400_000.0;

/// How often each letter turns up in English text, in percent.
const FREQUENCY: [(char, f64); 26] = [
    ('e', 12.7),
    ('t', 9.1),
    ('a', 8.2),
    ('o', 7.5),
    ('i', 7.0),
    ('n', 6.7),
    ('s', 6.3),
    ('h', 6.1),
    ('r', 6.0),
    ('d', 4.3),
    ('l', 4.0),
    ('c', 2.8),
    ('u', 2.8),
    ('m', 2.4),
    ('w', 2.4),
    ('f', 2.2),
    ('g', 2.0),
    ('y', 2.0),
    ('p', 1.9),
    ('b', 1.5),
    ('v', 1.0),
    ('k', 0.8),
    ('j', 0.15),
    ('x', 0.15),
    ('q', 0.1),
    ('z', 0.07),
];

fn frequency(c: char) -> f64 {
    FREQUENCY
        .iter()
        .find(|(l, _)| *l == c)
        .map_or(0.0, |(_, f)| *f)
}

/// The keys that land somewhere new going from `before` to `after` (30-key
/// strings, spaces ignored), most typed first, and the share of typing they
/// carry in percent.
pub fn moved(before: &str, after: &str) -> (Vec<char>, f64) {
    let a: Vec<char> = before.chars().filter(|c| *c != ' ').collect();
    let b: Vec<char> = after.chars().filter(|c| *c != ' ').collect();
    let mut keys: Vec<char> = b
        .iter()
        .zip(a.iter())
        .filter(|(n, o)| n != o)
        .map(|(n, _)| *n)
        .collect();
    keys.sort_by(|x, y| frequency(*y).total_cmp(&frequency(*x)));
    keys.dedup();
    let share = keys.iter().map(|c| frequency(*c)).sum();
    (keys, share)
}

/// Where a stage stands.
#[derive(Debug, Clone, PartialEq)]
pub enum Status {
    /// Behind you: how many days it spanned and its practice minutes, when there are runs to tell.
    Done(Option<(f64, f64)>),
    /// The one you're on: days and practice minutes so far, and the days likely
    /// left once its real-text trend says.
    Current {
        days: f64,
        minutes: f64,
        left: Option<(f64, f64)>,
    },
    /// Still to come: the days it'll likely take, once your own pace is known.
    Ahead(Option<(f64, f64)>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct StageLine {
    pub stage: usize,
    pub name: String,
    /// The keys this stage moves, most typed first.
    pub keys: Vec<char>,
    /// The share of your typing they carry, in percent.
    pub share: f64,
    pub status: Status,
}

/// When each stage's runs began and ended, and their practice minutes.
fn spans(runs: &[Run], stage: usize) -> Option<(u64, u64, f64)> {
    let at: Vec<&Run> = runs.iter().filter(|r| r.stage == stage).collect();
    let first = at.first()?;
    let start = first
        .t
        .saturating_sub((coach::run_secs(first) * 1000.0) as u64);
    // a stage ends where the next one's runs begin
    let end = runs
        .iter()
        .filter(|r| r.stage > stage && r.t > start)
        .map(|r| r.t.saturating_sub((coach::run_secs(r) * 1000.0) as u64))
        .min()
        .unwrap_or_else(|| at.last().map_or(start, |r| r.t));
    let minutes = at.iter().map(|r| coach::run_secs(r)).sum::<f64>() / 60.0;
    Some((start, end, minutes))
}

/// Practice minutes a day lately: the last fortnight's minutes over the days
/// it covers, within reason.
fn minutes_per_day(runs: &[Run], now_ms: u64) -> f64 {
    let window_start = now_ms.saturating_sub((PACE_WINDOW_DAYS * DAY_MS) as u64);
    let recent: Vec<&Run> = runs.iter().filter(|r| r.t >= window_start).collect();
    let Some(first) = recent.first() else {
        return DEFAULT_MINUTES_PER_DAY;
    };
    let days = ((now_ms.saturating_sub(first.t)) as f64 / DAY_MS)
        .ceil()
        .clamp(1.0, PACE_WINDOW_DAYS);
    let minutes = recent.iter().map(|r| coach::run_secs(r)).sum::<f64>() / 60.0;
    (minutes / days).clamp(10.0, 30.0)
}

fn range(days: f64) -> (f64, f64) {
    (days * LOW, days * HIGH)
}

/// Every stage, with what's happened and a rough guess at what's to come.
pub fn journey(progress: &Progress, now_ms: u64) -> Vec<StageLine> {
    let current = progress.stage.min(layout::last());
    let per_day = minutes_per_day(&progress.runs, now_ms);
    let shares: Vec<(Vec<char>, f64)> = (0..=layout::last())
        .map(|s| {
            if s == 0 {
                (Vec::new(), 0.0)
            } else {
                moved(layout::keys_string(s - 1), layout::keys_string(s))
            }
        })
        .collect();

    // your own pace, once a stage with moves is behind you
    let (done_minutes, done_share) = (1..current)
        .filter_map(|s| spans(&progress.runs, s).map(|(_, _, m)| (m, shares[s].1)))
        .fold((0.0, 0.0), |(m, w), (dm, dw)| (m + dm, w + dw));
    let per_point = (done_share > 0.0 && done_minutes > 0.0).then(|| done_minutes / done_share);

    (0..=layout::last())
        .map(|s| {
            let (keys, share) = shares[s].clone();
            let status = if s < current {
                Status::Done(spans(&progress.runs, s).map(|(start, end, minutes)| {
                    ((end.saturating_sub(start)) as f64 / DAY_MS, minutes)
                }))
            } else if s == current {
                let (days, minutes) = spans(&progress.runs, s)
                    .map_or((0.0, 0.0), |(start, _, m)| {
                        ((now_ms.saturating_sub(start)) as f64 / DAY_MS, m)
                    });
                let left = report::sessions_to_gate(progress, s)
                    .map(|n| range(n as f64 * coach::SESSION_MINUTES / per_day));
                Status::Current {
                    days,
                    minutes,
                    left,
                }
            } else {
                Status::Ahead(per_point.map(|p| range(share * p / per_day)))
            };
            StageLine {
                stage: s,
                name: layout::name(s).to_string(),
                keys,
                share,
                status,
            }
        })
        .collect()
}

/// The days likely left in the whole plan, once every stage still to go has a guess.
pub fn remaining(lines: &[StageLine]) -> Option<(f64, f64)> {
    lines
        .iter()
        .try_fold((0.0, 0.0), |(lo, hi), l| match l.status {
            Status::Current { left, .. } | Status::Ahead(left) => {
                left.map(|d| (lo + d.0, hi + d.1))
            }
            Status::Done(_) => Some((lo, hi)),
        })
}

/// A range of days in words, kept vague on purpose.
pub fn vague(days: (f64, f64)) -> &'static str {
    let mid = (days.0.max(0.01) * days.1.max(0.01)).sqrt();
    match mid {
        m if m < 4.0 => "a few days",
        m if m < 9.0 => "about a week",
        m if m < 15.0 => "1–2 weeks",
        m if m < 30.0 => "2–4 weeks",
        m if m < 60.0 => "1–2 months",
        m if m < 100.0 => "2–3 months",
        _ => "3 months or more",
    }
}

/// How long a finished or current stage has taken: "4 days · 1h 20m".
pub fn spent(days: f64, minutes: f64) -> String {
    let days = days.floor() as u64;
    let when = match days {
        0 => "today".to_string(),
        1 => "1 day".to_string(),
        n => format!("{n} days"),
    };
    let m = minutes.round() as u64;
    let practice = if m >= 60 {
        format!("{}h {:02}m", m / 60, m % 60)
    } else {
        format!("{m}m")
    };
    format!("{when} · {practice}")
}

#[cfg(test)]
mod tests {
    use super::*;

    const QWERTY: &str = "qwertyuiop asdfghjkl; zxcvbnm,./";

    #[test]
    fn weighs_a_stage_by_how_much_typing_it_moves() {
        let (keys, share) = moved(QWERTY, "qwlrfyuiop asdtghjke; zxcvbnm,./");
        assert_eq!(keys, ['e', 't', 'l', 'f']);
        assert!((share - 28.0).abs() < 0.01, "{share}");
        let (keys, share) = moved(QWERTY, QWERTY);
        assert!(keys.is_empty() && share == 0.0);
    }

    #[test]
    fn early_stages_move_more_than_late_ones() {
        let lines = journey(&Progress::default(), 1_800_000_000_000);
        if lines.len() > 5 {
            assert!(lines[1].share > lines[5].share, "{lines:?}");
        }
    }

    #[test]
    fn guesses_nothing_without_your_own_data() {
        let p = Progress {
            stage: 1,
            ..Progress::default()
        };
        let lines = journey(&p, 1_800_000_000_000);
        for l in &lines[1..] {
            match &l.status {
                Status::Current { left, .. } => assert_eq!(*left, None),
                Status::Ahead(d) => assert_eq!(*d, None),
                Status::Done(_) => panic!("stage {} isn't done", l.stage),
            }
        }
        assert_eq!(remaining(&lines), None);
    }

    #[test]
    fn a_finished_stage_sets_the_pace_for_the_rest() {
        let day = DAY_MS as u64;
        let t0 = 1_800_000_000_000;
        // stage 1 took 280 minutes over a fortnight
        let mut runs: Vec<Run> = (0..14).map(|i| run(t0 + i * day, 1, 1200.0)).collect();
        runs.push(run(t0 + 14 * day, 2, 60.0));
        let p = Progress {
            stage: 2,
            runs,
            ..Progress::default()
        };
        let lines = journey(&p, t0 + 14 * day + 1000);
        assert!(
            matches!(lines[1].status, Status::Done(Some(_))),
            "{:?}",
            lines[1]
        );
        if lines.len() > 3 {
            let Status::Ahead(Some((lo, hi))) = lines[3].status else {
                panic!("{:?}", lines[3]);
            };
            assert!(lo > 0.0 && hi > lo);
        }
    }

    #[test]
    fn says_ranges_in_words() {
        assert_eq!(vague((1.0, 3.0)), "a few days");
        assert_eq!(vague((4.0, 10.0)), "about a week");
        assert_eq!(vague((13.0, 34.0)), "2–4 weeks");
        assert_eq!(vague((60.0, 160.0)), "2–3 months");
    }

    #[test]
    fn tells_finished_time_plainly() {
        assert_eq!(spent(0.2, 12.4), "today · 12m");
        assert_eq!(spent(4.7, 80.0), "4 days · 1h 20m");
    }

    fn run(t: u64, stage: usize, secs: f64) -> Run {
        Run {
            t,
            stage,
            mode: "words".into(),
            wpm: 12.0,
            acc: 97.0,
            secs,
            ..Run::default()
        }
    }

    #[test]
    fn a_stage_runs_from_its_first_run_to_the_next_stage() {
        let day = DAY_MS as u64;
        let t0 = 1_800_000_000_000;
        let runs = vec![
            run(t0, 1, 60.0),
            run(t0 + day, 1, 120.0),
            run(t0 + 3 * day, 2, 60.0),
        ];
        let (start, end, minutes) = spans(&runs, 1).unwrap();
        assert_eq!(start, t0 - 60_000);
        assert_eq!(end, t0 + 3 * day - 60_000);
        assert_eq!(minutes, 3.0);
        assert_eq!(spans(&runs, 3), None);
    }

    #[test]
    fn daily_pace_comes_from_recent_practice_within_reason() {
        let now = 1_800_000_000_000;
        assert_eq!(minutes_per_day(&[], now), DEFAULT_MINUTES_PER_DAY);
        let day = DAY_MS as u64;
        // 15 minutes a day for four days
        let runs: Vec<Run> = (0..4)
            .map(|i| run(now - (4 - i) * day + 1, 1, 900.0))
            .collect();
        assert!((minutes_per_day(&runs, now) - 15.0).abs() < 0.01);
        let lots = vec![run(now - 1000, 1, 3600.0 * 3.0)];
        assert_eq!(minutes_per_day(&lots, now), 30.0);
    }
}
