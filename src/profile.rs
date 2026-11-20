//! Profiles: every profile is a folder of its own, holding `progress.json`,
//! `plan.toml`, `charts.html` and `plans/` (the plans you've made, their
//! summaries, and progress kept for the ones you're not practising). They live
//! in `~/.local/share/very-normal/profiles/`, and `main` is the one that opens
//! unless you say otherwise.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

/// The profile that opens when nothing names another.
pub const DEFAULT: &str = "main";

/// Where named profiles live.
pub fn root(home: &Path) -> PathBuf {
    home.join(".local/share/very-normal/profiles")
}

fn home() -> PathBuf {
    PathBuf::from(std::env::var_os("HOME").unwrap_or_default())
}

/// A profile name is a plain word, so it can't reach outside `root`.
pub fn check_name(name: &str) -> Result<()> {
    if name.is_empty() || name.starts_with('.') || name.contains(['/', '\\']) {
        bail!("a profile name is a plain word, like `demo`, not {name:?}");
    }
    Ok(())
}

/// Takes `--profile <name>` (or `--profile=<name>`) out of the arguments, and
/// gives the folder of the profile to use: that one, or `$VERY_NORMAL_HOME`
/// (any folder), or `$VERY_NORMAL_PROFILE` (a name), or `main`.
pub fn resolve(
    args: Vec<String>,
    home: &Path,
    home_var: Option<OsString>,
    profile_var: Option<OsString>,
) -> Result<(Vec<String>, PathBuf)> {
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
    if let Some(name) = name {
        check_name(&name)?;
        return Ok((rest, root(home).join(name)));
    }
    if let Some(dir) = home_var.filter(|d| !d.is_empty()) {
        return Ok((rest, PathBuf::from(dir)));
    }
    let name = profile_var
        .filter(|n| !n.is_empty())
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| DEFAULT.to_string());
    check_name(&name).context("in $VERY_NORMAL_PROFILE")?;
    Ok((rest, root(home).join(name)))
}

/// The profile folder in use: `$VERY_NORMAL_HOME`, which `main` sets from
/// [`resolve`], or `main` when nothing set it (in tests, say).
pub fn dir() -> PathBuf {
    match std::env::var_os("VERY_NORMAL_HOME").filter(|d| !d.is_empty()) {
        Some(d) => PathBuf::from(d),
        None => root(&home()).join(DEFAULT),
    }
}

/// What to call the profile in use on screen, or `None` for `main`: its name
/// when it's one of the named profiles, otherwise its folder.
pub fn label() -> Option<String> {
    label_for(&dir(), &root(&home()))
}

fn label_for(dir: &Path, root: &Path) -> Option<String> {
    if dir.parent() == Some(root) {
        let name = dir.file_name()?.to_string_lossy().into_owned();
        return (name != DEFAULT).then_some(name);
    }
    Some(dir.display().to_string())
}

/// Moves a profile from before profiles were folders into `main`: the plan
/// and made plans from `~/.config/very-normal`, and progress and charts from
/// `~/.local/share/very-normal`. Only when `main` doesn't exist yet, so it runs
/// once and never overwrites anything. Says what it moved.
pub fn migrate(home: &Path) -> Result<Vec<PathBuf>> {
    let main = root(home).join(DEFAULT);
    if main.exists() {
        return Ok(Vec::new());
    }
    let config = home.join(".config/very-normal");
    let data = home.join(".local/share/very-normal");
    let mut moves: Vec<(PathBuf, PathBuf)> = Vec::new();
    for (from_dir, files) in [
        (&config, &["plan.toml"][..]),
        (&data, &["progress.json", "charts.html"][..]),
    ] {
        for f in files {
            let from = from_dir.join(f);
            if from.is_file() {
                moves.push((from, main.join(f)));
            }
        }
        let plans = from_dir.join("plans");
        for entry in std::fs::read_dir(&plans).into_iter().flatten().flatten() {
            let from = entry.path();
            if from.is_file() {
                moves.push((from, main.join("plans").join(entry.file_name())));
            }
        }
    }
    if moves.is_empty() {
        return Ok(Vec::new());
    }
    std::fs::create_dir_all(main.join("plans"))?;
    let mut moved = Vec::new();
    for (from, to) in moves {
        if to.exists() {
            continue;
        }
        if std::fs::rename(&from, &to).is_err() {
            // a different disk: copy, then remove
            std::fs::copy(&from, &to)
                .with_context(|| format!("moving {} to {}", from.display(), to.display()))?;
            std::fs::remove_file(&from)?;
        }
        moved.push(from);
    }
    // tidy up folders the move left empty; anything else stays where it is
    for d in [config.join("plans"), config, data.join("plans")] {
        let _ = std::fs::remove_dir(d);
    }
    Ok(moved)
}

/// The named profiles, sorted.
pub fn list() -> Vec<String> {
    list_in(&root(&home()))
}

fn list_in(root: &Path) -> Vec<String> {
    let mut out: Vec<String> = std::fs::read_dir(root)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.path().is_dir())
        .filter_map(|e| e.file_name().to_str().map(String::from))
        .filter(|n| !n.starts_with('.'))
        .collect();
    out.sort();
    out
}

/// Lines for `very-normal profiles`: each profile, with the one in use marked.
pub fn listing() -> Vec<String> {
    let current = dir();
    let root = root(&home());
    let mut out: Vec<String> = list_in(&root)
        .into_iter()
        .map(|n| {
            let mark = if root.join(&n) == current { "*" } else { " " };
            let default = if n == DEFAULT {
                "  (opens by default)"
            } else {
                ""
            };
            format!("{mark} {n}{default}")
        })
        .collect();
    if current.parent() != Some(root.as_path()) {
        out.push(format!("* {} ($VERY_NORMAL_HOME)", current.display()));
    }
    if out.is_empty() {
        out.push("No profiles yet: `very-normal` makes main the first time it runs.".into());
    }
    out
}

/// Renames a named profile's folder.
pub fn rename(old: &str, new: &str) -> Result<String> {
    rename_in(&root(&home()), &dir(), old, new)
}

fn rename_in(root: &Path, current: &Path, old: &str, new: &str) -> Result<String> {
    check_name(old)?;
    check_name(new)?;
    let (from, to) = (root.join(old), root.join(new));
    if !from.is_dir() {
        bail!(
            "no profile called {old}; you have: {}",
            list_in(root).join(", ")
        );
    }
    if from == current {
        bail!("{old} is the profile you're using; rename it from another, with --profile");
    }
    if to.exists() {
        bail!("there's already a profile called {new}");
    }
    std::fs::rename(&from, &to)?;
    let note = if new == DEFAULT {
        " It opens by default now."
    } else if old == DEFAULT {
        " There's no main now, so the next plain `very-normal` starts a fresh one."
    } else {
        ""
    };
    Ok(format!("Renamed profile {old} to {new}.{note}"))
}

/// Deletes a named profile's folder, with its plans and progress.
pub fn remove(name: &str) -> Result<String> {
    remove_in(&root(&home()), &dir(), name)
}

fn remove_in(root: &Path, current: &Path, name: &str) -> Result<String> {
    check_name(name)?;
    let target = root.join(name);
    if !target.is_dir() {
        bail!(
            "no profile called {name}; you have: {}",
            list_in(root).join(", ")
        );
    }
    if target == current {
        bail!("{name} is the profile you're using; remove it from another, with --profile");
    }
    std::fs::remove_dir_all(&target).with_context(|| format!("removing {}", target.display()))?;
    Ok(format!(
        "Removed profile {name}, with its plans and progress ({}).",
        target.display()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    fn os(s: &str) -> Option<OsString> {
        Some(OsString::from(s))
    }

    #[test]
    fn picks_the_flag_then_home_then_profile_then_main() {
        let h = Path::new("/h");
        let profiles = PathBuf::from("/h/.local/share/very-normal/profiles");
        let r = |a: &[&str], home_var, profile_var| resolve(args(a), h, home_var, profile_var);
        assert_eq!(r(&[], None, None).unwrap().1, profiles.join("main"));
        assert_eq!(r(&[], None, os("work")).unwrap().1, profiles.join("work"));
        assert_eq!(
            r(&[], os("/tmp/x"), os("work")).unwrap().1,
            PathBuf::from("/tmp/x")
        );
        let (rest, d) = r(&["dashboard", "--profile=demo"], os("/tmp/x"), os("work")).unwrap();
        assert_eq!((rest, d), (args(&["dashboard"]), profiles.join("demo")));
        let (rest, d) = r(&["--profile", "demo", "stage", "2"], None, None).unwrap();
        assert_eq!((rest.len(), d), (2, profiles.join("demo")));
        assert_eq!(
            r(&[], os(""), os("")).unwrap().1,
            profiles.join("main"),
            "empty is unset"
        );
        assert!(r(&["--profile"], None, None).is_err());
        assert!(r(&["--profile", "../x"], None, None).is_err());
        assert!(r(&["--profile", ".hidden"], None, None).is_err());
        assert!(r(&[], None, os("a/b")).is_err());
    }

    #[test]
    fn names_the_profile_unless_its_main() {
        let root = Path::new("/h/profiles");
        assert_eq!(label_for(&root.join("main"), root), None);
        assert_eq!(label_for(&root.join("demo"), root), Some("demo".into()));
        assert_eq!(label_for(Path::new("/tmp/x"), root), Some("/tmp/x".into()));
    }

    fn scratch(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("vn-profile-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn write(p: &Path, text: &str) {
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, text).unwrap();
    }

    #[test]
    fn moves_the_old_split_profile_into_main_once() {
        let h = scratch("migrate");
        let config = h.join(".config/very-normal");
        let data = h.join(".local/share/very-normal");
        write(&config.join("plan.toml"), "plan");
        write(&config.join("plans/mine.toml"), "made");
        write(&config.join("plans/mine.md"), "summary");
        write(&data.join("progress.json"), "progress");
        write(&data.join("charts.html"), "charts");
        write(&data.join("plans/other.json"), "parked");
        let moved = migrate(&h).unwrap();
        assert_eq!(moved.len(), 6);
        let main = root(&h).join("main");
        for (f, text) in [
            ("plan.toml", "plan"),
            ("plans/mine.toml", "made"),
            ("plans/mine.md", "summary"),
            ("progress.json", "progress"),
            ("charts.html", "charts"),
            ("plans/other.json", "parked"),
        ] {
            assert_eq!(std::fs::read_to_string(main.join(f)).unwrap(), text, "{f}");
        }
        assert!(!config.exists(), "the emptied config folder goes");
        assert!(!data.join("plans").exists() && !data.join("progress.json").exists());
        // once main exists, it never runs again
        write(&data.join("progress.json"), "stray");
        assert!(migrate(&h).unwrap().is_empty());
        assert_eq!(
            std::fs::read_to_string(main.join("progress.json")).unwrap(),
            "progress"
        );
        std::fs::remove_dir_all(&h).unwrap();
    }

    #[test]
    fn nothing_to_move_makes_nothing() {
        let h = scratch("fresh");
        assert!(migrate(&h).unwrap().is_empty());
        assert!(
            !root(&h).exists(),
            "a fresh start makes main when it first saves"
        );
        std::fs::remove_dir_all(&h).unwrap();
    }

    #[test]
    fn renames_and_removes_any_profile_but_the_one_in_use() {
        let h = scratch("manage");
        let root = root(&h);
        for n in ["main", "demo", "old"] {
            write(&root.join(n).join("progress.json"), n);
        }
        let main = root.join("main");
        assert_eq!(list_in(&root), ["demo", "main", "old"]);
        assert!(rename_in(&root, &main, "main", "x").is_err(), "in use");
        assert!(rename_in(&root, &main, "demo", "old").is_err(), "taken");
        assert!(rename_in(&root, &main, "nope", "x").is_err());
        assert!(
            rename_in(&root, &main, "demo", "try")
                .unwrap()
                .contains("Renamed")
        );
        assert!(remove_in(&root, &main, "main").is_err(), "in use");
        assert!(remove_in(&root, &main, "../h").is_err());
        assert!(
            remove_in(&root, &main, "old")
                .unwrap()
                .contains("Removed profile old")
        );
        assert_eq!(list_in(&root), ["main", "try"]);
        std::fs::remove_dir_all(&h).unwrap();
    }
}
