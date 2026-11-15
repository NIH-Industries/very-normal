//! Progress on disk. The shape matches the web tutor's backup text, so a
//! backup copied out of the artifact imports as it is.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Progress {
    #[serde(default)]
    pub stage: usize,
    #[serde(default)]
    pub runs: Vec<Run>,
    /// Keyed by stage number as a string.
    #[serde(default)]
    pub manual: BTreeMap<String, Manual>,
    /// Stage number, then the character, both as strings.
    #[serde(default)]
    pub letters: BTreeMap<String, BTreeMap<String, LetterStat>>,
    /// Stage number, then two characters typed one after the other: how long
    /// the second took after the first, and how often it was missed there.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub pairs: BTreeMap<String, BTreeMap<String, LetterStat>>,
    /// The local date of the last warm-up, as YYYY-MM-DD.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub warmed_up: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Run {
    /// Milliseconds since the Unix epoch.
    pub t: u64,
    pub stage: usize,
    pub mode: String,
    pub wpm: f64,
    pub acc: f64,
    /// How long the drill took; missing from older runs and web tutor backups.
    #[serde(default)]
    pub secs: f64,
    /// This run's timings and misses by character, so a key's speed can be
    /// followed over time. Missing from older runs.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub letters: BTreeMap<String, LetterStat>,
    /// Wrong keys, as "wanted>typed", with how often.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub typos: BTreeMap<String, u64>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Manual {
    #[serde(default)]
    pub oryx: bool,
    #[serde(default)]
    pub vim: bool,
    #[serde(default)]
    pub workday: bool,
    /// A baseline typed in by hand, for stage 0.
    #[serde(default)]
    pub wpm: f64,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct LetterStat {
    pub n: u64,
    pub miss: u64,
    pub ms: f64,
}

impl LetterStat {
    pub fn add(&mut self, other: &LetterStat) {
        self.n += other.n;
        self.miss += other.miss;
        self.ms += other.ms;
    }

    pub fn average_ms(&self) -> Option<f64> {
        (self.n > 0).then(|| self.ms / self.n as f64)
    }
}

/// `$VERY_NORMAL_DATA`, or `progress.json` in `$VERY_NORMAL_HOME`, or
/// `~/.local/share/very-normal/progress.json`.
pub fn default_path() -> PathBuf {
    path_from(
        std::env::var_os("VERY_NORMAL_DATA"),
        std::env::var_os("VERY_NORMAL_HOME"),
        std::env::var_os("HOME"),
    )
}

/// Takes `--profile <name>` (or `--profile=<name>`) out of the arguments, and
/// gives the folder that profile lives in:
/// `~/.local/share/very-normal/profiles/<name>`.
pub fn take_profile(
    args: Vec<String>,
    home: Option<std::ffi::OsString>,
) -> anyhow::Result<(Vec<String>, Option<PathBuf>)> {
    let mut rest = Vec::new();
    let mut name = None;
    let mut it = args.into_iter();
    while let Some(a) = it.next() {
        if a == "--profile" {
            name = Some(
                it.next()
                    .ok_or_else(|| anyhow::anyhow!("--profile needs a name"))?,
            );
        } else if let Some(n) = a.strip_prefix("--profile=") {
            name = Some(n.to_string());
        } else {
            rest.push(a);
        }
    }
    let Some(name) = name else {
        return Ok((rest, None));
    };
    if name.is_empty() || name.starts_with('.') || name.contains(['/', '\\']) {
        anyhow::bail!("a profile name is a plain word, like `demo`, not {name:?}");
    }
    let dir = PathBuf::from(home.unwrap_or_default())
        .join(".local/share/very-normal/profiles")
        .join(name);
    Ok((rest, Some(dir)))
}

fn path_from(
    data: Option<std::ffi::OsString>,
    profile: Option<std::ffi::OsString>,
    home: Option<std::ffi::OsString>,
) -> PathBuf {
    if let Some(p) = data.filter(|p| !p.is_empty()) {
        return PathBuf::from(p);
    }
    if let Some(dir) = profile.filter(|p| !p.is_empty()) {
        return PathBuf::from(dir).join("progress.json");
    }
    PathBuf::from(home.unwrap_or_default()).join(".local/share/very-normal/progress.json")
}

impl Progress {
    pub fn load(path: &Path) -> Result<Progress> {
        match std::fs::read_to_string(path) {
            Ok(text) => {
                Progress::parse(&text).with_context(|| format!("reading {}", path.display()))
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Progress::default()),
            Err(e) => Err(e).with_context(|| format!("reading {}", path.display())),
        }
    }

    pub fn parse(text: &str) -> Result<Progress> {
        Ok(serde_json::from_str(text)?)
    }

    /// Writes to a temporary file first so a crash never leaves half a file.
    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
        }
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_string_pretty(self)?)?;
        std::fs::rename(&tmp, path).with_context(|| format!("writing {}", path.display()))?;
        Ok(())
    }

    pub fn manual(&self, stage: usize) -> Manual {
        self.manual
            .get(&stage.to_string())
            .cloned()
            .unwrap_or_default()
    }

    pub fn manual_mut(&mut self, stage: usize) -> &mut Manual {
        self.manual.entry(stage.to_string()).or_default()
    }

    pub fn letters(&self, stage: usize) -> BTreeMap<String, LetterStat> {
        self.letters
            .get(&stage.to_string())
            .cloned()
            .unwrap_or_default()
    }

    pub fn runs_for(&self, stage: usize, mode: &str) -> Vec<&Run> {
        self.runs
            .iter()
            .filter(|r| r.stage == stage && r.mode == mode)
            .collect()
    }

    /// Average of the last three stage-0 real-text runs, or the number typed in.
    pub fn baseline(&self) -> Option<f64> {
        let real = self.runs_for(0, "real");
        if real.is_empty() {
            let typed = self.manual(0).wpm;
            return (typed > 0.0).then_some(typed);
        }
        let last: Vec<_> = real.iter().rev().take(3).collect();
        Some(last.iter().map(|r| r.wpm).sum::<f64>() / last.len() as f64)
    }

    pub fn pairs(&self, stage: usize) -> BTreeMap<String, LetterStat> {
        self.pairs
            .get(&stage.to_string())
            .cloned()
            .unwrap_or_default()
    }

    /// Saves a run, adding its letters (and the drill's pairs) to the stage's totals.
    pub fn record(&mut self, run: Run, pairs: &BTreeMap<String, LetterStat>) {
        let stage = self.letters.entry(run.stage.to_string()).or_default();
        for (ch, stat) in &run.letters {
            stage.entry(ch.clone()).or_default().add(stat);
        }
        let stage = self.pairs.entry(run.stage.to_string()).or_default();
        for (pair, stat) in pairs {
            stage.entry(pair.clone()).or_default().add(stat);
        }
        self.runs.push(run);
        if self.runs.len() > 2000 {
            let excess = self.runs.len() - 2000;
            self.runs.drain(..excess);
        }
    }

    /// Folds another progress file (such as a web tutor backup) into this one.
    /// Runs are deduplicated by time, stage and drill; ticked boxes stay ticked;
    /// letter timings are only taken for stages that have none yet, so importing
    /// the same backup twice doesn't double them.
    pub fn merge(&mut self, other: Progress) -> usize {
        let before = self.runs.len();
        for run in other.runs {
            let dup = self
                .runs
                .iter()
                .any(|r| r.t == run.t && r.stage == run.stage && r.mode == run.mode);
            if !dup {
                self.runs.push(run);
            }
        }
        self.runs.sort_by_key(|r| r.t);
        for (stage, m) in other.manual {
            let mine = self.manual.entry(stage).or_default();
            mine.oryx |= m.oryx;
            mine.vim |= m.vim;
            mine.workday |= m.workday;
            if mine.wpm <= 0.0 {
                mine.wpm = m.wpm;
            }
        }
        for (stage, letters) in other.letters {
            self.letters.entry(stage).or_insert(letters);
        }
        for (stage, pairs) in other.pairs {
            self.pairs.entry(stage).or_insert(pairs);
        }
        self.stage = self.stage.max(other.stage);
        self.runs.len() - before
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profile_comes_out_of_the_arguments_wherever_it_is() {
        let args = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        let home = Some(std::ffi::OsString::from("/h"));
        let dir = PathBuf::from("/h/.local/share/very-normal/profiles/demo");
        let (rest, p) = take_profile(args(&["--profile", "demo"]), home.clone()).unwrap();
        assert!(rest.is_empty());
        assert_eq!(p, Some(dir.clone()));
        let (rest, p) = take_profile(args(&["dashboard", "--profile=demo"]), home.clone()).unwrap();
        assert_eq!(rest, ["dashboard"]);
        assert_eq!(p, Some(dir));
        let (rest, p) = take_profile(args(&["stage", "2"]), home.clone()).unwrap();
        assert_eq!((rest.len(), p), (2, None));
        assert!(take_profile(args(&["--profile"]), home.clone()).is_err());
        assert!(take_profile(args(&["--profile", "../x"]), home.clone()).is_err());
        assert!(take_profile(args(&["--profile", ".hidden"]), home).is_err());
    }

    #[test]
    fn a_profile_folder_holds_the_progress_unless_data_says_otherwise() {
        let s = |v: &str| Some(std::ffi::OsString::from(v));
        assert_eq!(
            path_from(None, None, s("/h")),
            PathBuf::from("/h/.local/share/very-normal/progress.json")
        );
        assert_eq!(
            path_from(None, s("/tmp/demo"), s("/h")),
            PathBuf::from("/tmp/demo/progress.json")
        );
        assert_eq!(
            path_from(s("/x/p.json"), s("/tmp/demo"), s("/h")),
            PathBuf::from("/x/p.json")
        );
        assert_eq!(
            path_from(None, s(""), s("/h")),
            PathBuf::from("/h/.local/share/very-normal/progress.json"),
            "an empty variable counts as unset"
        );
    }

    const BACKUP: &str = r#"{"stage":1,"runs":[
        {"t":1794400000000,"stage":0,"mode":"real","wpm":34,"acc":96.5},
        {"t":1794400100000,"stage":0,"mode":"real","wpm":36,"acc":97},
        {"t":1794500000000,"stage":1,"mode":"words","wpm":13,"acc":98.2}],
        "manual":{"0":{"oryx":true,"wpm":0}},
        "letters":{"1":{"e":{"n":10,"miss":2,"ms":5200}}}}"#;

    #[test]
    fn reads_a_web_tutor_backup() {
        let p = Progress::parse(BACKUP).unwrap();
        assert_eq!(p.stage, 1);
        assert_eq!(p.runs.len(), 3);
        assert!(p.manual(0).oryx);
        assert_eq!(p.baseline(), Some(35.0));
        assert_eq!(p.letters(1)["e"].average_ms(), Some(520.0));
    }

    #[test]
    fn a_typed_baseline_counts_until_there_are_runs() {
        let mut p = Progress::default();
        assert_eq!(p.baseline(), None);
        p.manual_mut(0).wpm = 30.0;
        assert_eq!(p.baseline(), Some(30.0));
    }

    #[test]
    fn merging_the_same_backup_twice_changes_nothing() {
        let mut p = Progress::default();
        assert_eq!(p.merge(Progress::parse(BACKUP).unwrap()), 3);
        let once = p.clone();
        assert_eq!(p.merge(Progress::parse(BACKUP).unwrap()), 0);
        assert_eq!(p, once);
    }

    #[test]
    fn recording_a_run_adds_its_letters_and_pairs_to_the_stage() {
        let mut p = Progress::default();
        let stat = |n, miss, ms| LetterStat { n, miss, ms };
        let run = Run {
            stage: 2,
            letters: BTreeMap::from([("e".to_string(), stat(2, 1, 600.0))]),
            typos: BTreeMap::from([("e>r".to_string(), 1)]),
            ..Run::default()
        };
        let pairs = BTreeMap::from([("th".to_string(), stat(1, 0, 250.0))]);
        p.record(run.clone(), &pairs);
        p.record(run, &pairs);
        assert_eq!(p.letters(2)["e"], stat(4, 2, 1200.0));
        assert_eq!(p.pairs(2)["th"], stat(2, 0, 500.0));
        assert_eq!(p.runs[0].typos["e>r"], 1);
        let text = serde_json::to_string(&p).unwrap();
        assert_eq!(Progress::parse(&text).unwrap(), p);
    }

    #[test]
    fn saves_and_loads() {
        let dir = std::env::temp_dir().join(format!("very-normal-test-{}", std::process::id()));
        let path = dir.join("progress.json");
        let p = Progress::parse(BACKUP).unwrap();
        p.save(&path).unwrap();
        assert_eq!(Progress::load(&path).unwrap(), p);
        assert_eq!(
            Progress::load(&dir.join("missing.json")).unwrap(),
            Progress::default()
        );
        std::fs::remove_dir_all(dir).unwrap();
    }
}
