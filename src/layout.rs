//! The plan: the ladder of stages from where you start to your own layout.
//!
//! A plan is a TOML file (see `examples/very-normal.toml`). Positions run
//! left to right, top row then home row then bottom row, ten keys a row (five
//! per hand). Only letters and `; , . / '` live here; the outer columns never
//! move.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use anyhow::{Context, Result, bail};
use serde::Deserialize;

/// Mark's plan, used when you haven't made your own.
pub const EXAMPLE: &str = include_str!("../examples/very-normal.toml");
/// Everyday English words, most used first, from Tatoeba (CC BY 2.0 FR).
const EVERYDAY: &str = include_str!("../data/everyday-words.txt");

/// Everyday English words, most used first: drill words for anyone without
/// enough typing of their own.
pub fn everyday_words() -> Vec<String> {
    EVERYDAY
        .lines()
        .filter(|l| !l.starts_with('#') && !l.trim().is_empty())
        .map(|l| l.trim().to_string())
        .collect()
}

pub const OUTER_LEFT: [&str; 3] = ["tab", "esc", "shf"];
pub const OUTER_RIGHT: [&str; 3] = ["bsp", "'", "ent"];

/// The outer key on the right of a row. Beside the home row it's the quote,
/// unless the plan's 30 keys take the quote in (as Dvorak's do); then it
/// holds whatever they leave out.
pub fn outer_right(row: usize) -> String {
    if row == 1 {
        let stage0: String = keys(0).into_iter().collect();
        return crate::named::left_out(&stage0).to_string();
    }
    OUTER_RIGHT[row].to_string()
}

#[derive(Debug, Clone, Deserialize)]
pub struct Plan {
    pub name: String,
    /// A layout.txt your keyboard's firmware is built from.
    #[serde(default)]
    pub layout_file: Option<String>,
    /// An Oryx layout id, for a ZSA board.
    #[serde(default)]
    pub oryx_layout: Option<String>,
    /// What each home row key does when held, left to right.
    #[serde(default)]
    pub home_row_mods: Vec<String>,
    #[serde(default)]
    pub common_words: Vec<String>,
    #[serde(rename = "stage", default)]
    pub stages: Vec<Stage>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Stage {
    pub name: String,
    #[serde(default)]
    pub blurb: String,
    pub keys: String,
    /// Share of your typing on the home row at this stage.
    #[serde(default)]
    pub home: f64,
    /// Share of letter pairs typed with the same finger at this stage.
    #[serde(default)]
    pub same_finger: f64,
    #[serde(default)]
    pub vim: Vec<String>,
    #[serde(default)]
    pub words: Vec<String>,
}

impl Plan {
    /// One stage and nowhere to go: practice on the layout you have.
    pub fn staying(&self) -> bool {
        self.stages.len() == 1
    }

    /// A plan that stays on `keys`, called `name`, with `common_words` for its
    /// drills: `own` when they're from your own typing, so the file is private.
    pub fn stay_toml(name: &str, keys: &str, common_words: &[String], own: bool) -> String {
        let rows: Vec<String> = keys
            .chars()
            .collect::<Vec<_>>()
            .chunks(10)
            .map(|r| r.iter().collect())
            .collect();
        let words: Vec<String> = common_words
            .iter()
            .map(|w| serde_json::to_string(w).expect("a string"))
            .collect();
        let source = if own {
            "# The drill words are your own vocabulary, from your typing to Claude Code: keep\n\
             # this file private.\n"
        } else {
            "# The drill words are everyday English, counted from Tatoeba (CC BY 2.0 FR).\n"
        };
        format!(
            "# A very-normal plan that stays on {name}: no new layout, just practice.\n\
             # Made by `very-normal use`. Switch to a plan that moves keys any time.\n\
             {source}\n\
             name = {}\n\ncommon_words = [{}]\n\n\
             [[stage]]\nname = {}\nblurb = \"No keys move: practice on the layout you type on now.\"\nkeys = {}\n",
            serde_json::to_string(name).expect("a string"),
            words.join(", "),
            serde_json::to_string(name).expect("a string"),
            serde_json::to_string(&rows.join(" ")).expect("a string"),
        )
    }

    pub fn parse(text: &str) -> Result<Plan> {
        let mut plan: Plan = toml::from_str(text)?;
        for stage in &mut plan.stages {
            stage.keys.retain(|c| !c.is_whitespace());
        }
        plan.validate()?;
        Ok(plan)
    }

    pub fn load(path: &Path) -> Result<Plan> {
        let text =
            std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
        Plan::parse(&text).with_context(|| format!("in {}", path.display()))
    }

    fn validate(&self) -> Result<()> {
        if self.stages.is_empty() {
            bail!("a plan needs at least one stage: the layout you start on");
        }
        let mut start: Vec<char> = self.stages[0].keys.chars().collect();
        start.sort_unstable();
        for (i, stage) in self.stages.iter().enumerate() {
            let mut k: Vec<char> = stage.keys.chars().collect();
            if k.len() != 30 {
                bail!(
                    "stage {i} has {} keys; every stage has 30 (three rows of ten)",
                    k.len()
                );
            }
            k.sort_unstable();
            if k != start {
                bail!("stage {i} isn't a rearrangement of stage 0's keys");
            }
            if i > 0 && stage.keys == self.stages[i - 1].keys {
                bail!("stage {i} doesn't move any keys");
            }
        }
        if !self.home_row_mods.is_empty() && self.home_row_mods.len() != 10 {
            bail!(
                "home_row_mods has {} entries; it needs 10, one per home row key",
                self.home_row_mods.len()
            );
        }
        if self.common_words.len() < 50 {
            bail!(
                "common_words needs at least 50 words for the drills; it has {}",
                self.common_words.len()
            );
        }
        for m in &self.home_row_mods {
            if !matches!(m.as_str(), "" | "ctrl" | "alt" | "cmd" | "shift") {
                bail!("home_row_mods can hold ctrl, alt, cmd, shift or \"\", not {m:?}");
            }
        }
        Ok(())
    }
}

/// `plan.toml` in the profile in use.
pub fn plan_path() -> PathBuf {
    crate::profile::dir().join("plan.toml")
}

static PLAN: OnceLock<Plan> = OnceLock::new();

/// Loads your plan if you have one, otherwise Mark's. Call once at startup;
/// says which file it used, or `None` for the built-in example.
pub fn install() -> Result<Option<PathBuf>> {
    let path = plan_path();
    let (plan, used) = if path.exists() {
        (Plan::load(&path)?, Some(path))
    } else {
        (example(), None)
    };
    let _ = PLAN.set(plan);
    Ok(used)
}

fn example() -> Plan {
    Plan::parse(EXAMPLE).expect("the example plan is valid")
}

pub fn plan() -> &'static Plan {
    PLAN.get_or_init(example)
}

pub fn last() -> usize {
    plan().stages.len() - 1
}

/// A plan with one stage: practice on the layout you have, going nowhere new.
pub fn staying() -> bool {
    plan().staying()
}

fn stage(s: usize) -> &'static Stage {
    &plan().stages[s.min(last())]
}

pub fn name(s: usize) -> &'static str {
    &stage(s).name
}

pub fn blurb(s: usize) -> &'static str {
    &stage(s).blurb
}

/// Home row share and same-finger pairs, both as percentages.
pub fn stats(s: usize) -> (f64, f64) {
    (stage(s).home, stage(s).same_finger)
}

pub fn vim(s: usize) -> &'static [String] {
    &stage(s).vim
}

pub fn words(s: usize) -> &'static [String] {
    &stage(s).words
}

pub fn common_words() -> &'static [String] {
    &plan().common_words
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Change {
    pub index: usize,
    pub from: char,
    pub to: char,
}

impl Change {
    pub fn right_hand(&self) -> bool {
        self.index % 10 >= 5
    }
}

pub fn keys(s: usize) -> Vec<char> {
    stage(s).keys.chars().collect()
}

pub fn keys_string(s: usize) -> &'static str {
    &stage(s).keys
}

/// The keys that change when you climb from `stage - 1` to `stage`.
pub fn changes(s: usize) -> Vec<Change> {
    if s == 0 || s > last() {
        return Vec::new();
    }
    let before = keys(s - 1);
    keys(s)
        .into_iter()
        .enumerate()
        .filter(|(i, c)| before[*i] != *c)
        .map(|(index, to)| Change {
            index,
            from: before[index],
            to,
        })
        .collect()
}

/// The characters a stage puts somewhere new, in key order.
pub fn focus(s: usize) -> Vec<char> {
    let mut out = Vec::new();
    for c in changes(s) {
        if !out.contains(&c.to) {
            out.push(c.to);
        }
    }
    out
}

/// Whether a position types something different from stage 0.
pub fn moved(s: usize, index: usize) -> bool {
    keys(s)[index] != keys(0)[index]
}

pub fn changed_now(s: usize, index: usize) -> bool {
    changes(s).iter().any(|c| c.index == index)
}

/// What a key does when held, for home row positions 10 to 19.
pub fn modifier(index: usize) -> Option<&'static str> {
    let m = plan().home_row_mods.get(index.checked_sub(10)?)?;
    (!m.is_empty()).then_some(m.as_str())
}

pub fn position(s: usize, ch: char) -> Option<usize> {
    keys(s).iter().position(|c| *c == ch)
}

/// The left-hand position that mirrors a right-hand one, and the reverse.
pub fn mirror(index: usize) -> usize {
    let (row, col) = (index / 10, index % 10);
    row * 10 + (9 - col)
}

/// Which stage a main layer's 30 letter keys match, if any.
pub fn find_stage(main: &str) -> Option<usize> {
    plan().stages.iter().position(|s| s.keys == main)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_example_is_marks_very_normal() {
        let p = plan();
        assert_eq!(p.name, "Very Normal");
        assert_eq!(last(), 9);
        assert_eq!(keys_string(0), "qwertyuiopasdfghjkl;zxcvbnm,./");
        assert_eq!(keys_string(last()), "qwydgbhaujilstmpnoerzxcvk;f,./");
        assert_eq!(modifier(13), Some("cmd"));
        assert_eq!(modifier(10), None);
        assert_eq!(modifier(3), None);
    }

    #[test]
    fn every_stage_moves_a_few_keys_and_keeps_the_shortcuts() {
        for s in 1..=last() {
            let n = changes(s).len();
            assert!((3..=5).contains(&n), "stage {s} moves {n}");
            for c in "qwzxcv".chars() {
                assert_eq!(position(s, c), position(0, c), "{c} moved in stage {s}");
            }
        }
    }

    #[test]
    fn stage_one_swaps_e_l_t_f() {
        let got: Vec<(char, char)> = changes(1).iter().map(|c| (c.from, c.to)).collect();
        assert_eq!(got, vec![('e', 'l'), ('t', 'f'), ('f', 't'), ('l', 'e')]);
        assert_eq!(focus(1), vec!['l', 'f', 't', 'e']);
    }

    #[test]
    fn mirror_reflects_across_the_hands() {
        assert_eq!(mirror(18), 11);
        assert_eq!(mirror(0), 9);
        assert_eq!(mirror(mirror(23)), 23);
    }

    #[test]
    fn find_stage_recognises_each_stage() {
        for s in 0..=last() {
            assert_eq!(find_stage(keys_string(s)), Some(s));
        }
        assert_eq!(find_stage("nope"), None);
    }

    fn plan_with(stages: &[&str], extra: &str) -> Result<Plan> {
        let mut t = format!("name = \"t\"\n{extra}\n");
        t.push_str("common_words = [");
        for i in 0..60 {
            t.push_str(&format!("\"w{i}\", "));
        }
        t.push_str("]\n");
        for (i, k) in stages.iter().enumerate() {
            t.push_str(&format!("[[stage]]\nname = \"s{i}\"\nkeys = \"{k}\"\n"));
        }
        Plan::parse(&t)
    }

    #[test]
    fn everyday_words_are_plain_lowercase_english_without_names() {
        let words = everyday_words();
        assert_eq!(words.len(), 500);
        assert_eq!(&words[..3], ["the", "to", "i"]);
        assert!(
            words
                .iter()
                .all(|w| w.chars().all(|c| c.is_ascii_lowercase() || c == '\''))
        );
        for name in ["tom", "mary", "push", "branch"] {
            assert!(!words.contains(&name.to_string()), "{name}");
        }
        let plan = Plan::stay_toml("QWERTY", crate::named::QWERTY, &words, false);
        assert!(plan.contains("Tatoeba") && !plan.contains("private"));
        assert!(Plan::parse(&plan).unwrap().staying());
        let own = Plan::stay_toml("QWERTY", crate::named::QWERTY, &words, true);
        assert!(own.contains("keep\n# this file private"));
    }

    #[test]
    fn a_stay_plan_is_one_stage_on_your_layout() {
        let words: Vec<String> = (0..60).map(|i| format!("w{i}")).collect();
        let dvorak = "',.pyfgcrlaoeuidhtns;qjkxbmwvz";
        let plan = Plan::parse(&Plan::stay_toml("Dvorak", dvorak, &words, false)).unwrap();
        assert!(plan.staying());
        assert_eq!(plan.name, "Dvorak");
        assert_eq!(plan.stages[0].keys, dvorak);
        assert_eq!(plan.common_words, words);
    }

    #[test]
    fn rejects_broken_plans() {
        let q = "qwertyuiop asdfghjkl; zxcvbnm,./";
        let swap = "qwertyuiop asdfghjkl; zxcvbmn,./";
        assert!(plan_with(&[q, swap], "").is_ok());
        assert!(
            plan_with(&[q], "").unwrap().staying(),
            "one stage stays put"
        );
        assert!(!plan_with(&[q, swap], "").unwrap().staying());
        assert!(
            plan_with(&[], "")
                .unwrap_err()
                .to_string()
                .contains("at least one stage")
        );
        assert!(
            plan_with(&[q, q], "")
                .unwrap_err()
                .to_string()
                .contains("doesn't move")
        );
        assert!(
            plan_with(&[q, "qwertyuiop"], "")
                .unwrap_err()
                .to_string()
                .contains("30")
        );
        assert!(
            plan_with(&[q, "qwertyuiop asdfghjkl; zxcvbnm,.!"], "")
                .unwrap_err()
                .to_string()
                .contains("rearrangement")
        );
        assert!(plan_with(&[q, swap], "home_row_mods = [\"hyper\"]").is_err());
    }
}
