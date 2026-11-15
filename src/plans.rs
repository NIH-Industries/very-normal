//! The plans you've made, and switching between them. Making a plan never
//! touches the one you're practising; `use` switches, and keeps each plan's
//! progress separately so stage numbers never get mixed up between plans.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

use crate::layout::{self, Plan};
use crate::named;

/// A file name for a plan's name: `Home Row Hero` is `home-row-hero`.
pub fn slug(name: &str) -> String {
    let mut out = String::new();
    for c in name.chars().flat_map(char::to_lowercase) {
        if c.is_alphanumeric() {
            out.push(c);
        } else if !out.is_empty() && !out.ends_with('-') {
            out.push('-');
        }
    }
    let out = out.trim_end_matches('-').to_string();
    if out.is_empty() { "plan".into() } else { out }
}

/// Where made plans and their summaries live: next to the active plan.
pub fn dir() -> PathBuf {
    layout::plan_path().with_file_name("plans")
}

pub fn made_path(name: &str) -> PathBuf {
    dir().join(format!("{}.toml", slug(name)))
}

/// A plan's shareable summary sits next to it.
pub fn summary_of(plan: &Path) -> PathBuf {
    plan.with_extension("md")
}

/// Where a plan's progress waits while you practise another one.
pub fn parked(progress: &Path, slug: &str) -> PathBuf {
    progress
        .with_file_name("plans")
        .join(format!("{slug}.json"))
}

/// Nothing to lose yet: no plan of your own and no practice recorded.
pub fn fresh(progress: &Path) -> bool {
    !layout::plan_path().exists() && !progress.exists()
}

/// The plan you're practising.
pub fn active_slug() -> String {
    slug(&layout::plan().name)
}

fn builtin_slug() -> String {
    slug(
        &Plan::parse(layout::EXAMPLE)
            .expect("the example plan is valid")
            .name,
    )
}

/// The plans you've made, by slug, plus the built-in one.
pub fn list() -> Vec<String> {
    let mut out: Vec<String> = std::fs::read_dir(dir())
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let p = e.path();
            (p.extension()? == "toml").then(|| p.file_stem()?.to_str().map(String::from))?
        })
        .collect();
    let builtin = builtin_slug();
    if !out.contains(&builtin) {
        out.push(builtin);
    }
    out.sort();
    out
}

/// Where you're starting from: a known layout's name, a plan (one you've
/// made, by name, or any plan file) where it ends or at `@N` for its stage N,
/// or your own 30 keys written top row first.
pub fn start(s: &str) -> Result<(String, String)> {
    if let Some((name, keys)) = named::find(s) {
        return Ok((name.to_string(), keys.to_string()));
    }
    if let Some(found) = plan_start(s)? {
        return Ok(found);
    }
    named::start(s).map_err(|e| {
        anyhow::anyhow!("{e}, or a plan (a name from `very-normal plan`, or a .toml file)")
    })
}

/// A plan's layout, when `s` names one: by file or by name, with an optional `@N` stage,
/// and `~` for your home folder.
fn plan_start(s: &str) -> Result<Option<(String, String)>> {
    let (target, stage) = match s.rsplit_once('@') {
        Some((p, n)) if !p.is_empty() && n.parse::<usize>().is_ok() => (p, Some(n.parse()?)),
        _ => (s, None),
    };
    let expanded = match (target.strip_prefix("~/"), std::env::var_os("HOME")) {
        (Some(rest), Some(home)) => Path::new(&home).join(rest),
        _ => PathBuf::from(target),
    };
    let target_path = expanded.as_path();
    let plan = if target_path.is_file() {
        Plan::load(target_path)?
    } else if made_path(target).is_file() {
        Plan::load(&made_path(target))?
    } else if slug(target) == builtin_slug() {
        Plan::parse(layout::EXAMPLE)?
    } else if stage.is_some() || target.ends_with(".toml") {
        bail!("there's no plan called {target:?}: give a plan's name or its .toml file");
    } else {
        return Ok(None);
    };
    let last = plan.stages.len().saturating_sub(1);
    let n = stage.unwrap_or(last);
    if n > last {
        bail!("{} has stages 0 to {last}, not {n}", plan.name);
    }
    let keys = plan.stages[n].keys.clone();
    named::check(&keys).with_context(|| format!("{}, stage {n}", plan.name))?;
    let name = match stage {
        Some(n) => format!("{}, stage {n}", plan.name),
        None => plan.name.clone(),
    };
    Ok(Some((name, keys)))
}

/// Makes a plan the active one. Only for a fresh start: nothing to park.
pub fn activate(made: &Path) -> Result<()> {
    let to = layout::plan_path();
    if let Some(d) = to.parent() {
        std::fs::create_dir_all(d)?;
    }
    std::fs::copy(made, &to).with_context(|| format!("writing {}", to.display()))?;
    Ok(())
}

/// Switches to another plan: parks this plan's progress, and brings back
/// the other plan's progress if you've practised it before. If you haven't,
/// and its stages are exactly this plan's, your progress carries over.
pub fn switch(target: &str, progress: &Path) -> Result<String> {
    let current = active_slug();
    let want = slug(target);
    if want == current {
        bail!("you're already practising {}", layout::plan().name);
    }
    let made = dir().join(format!("{want}.toml"));
    let builtin = want == builtin_slug() && !made.exists();
    if !builtin && !made.exists() {
        bail!("no plan called {target}; you have: {}", list().join(", "));
    }
    let plan = if builtin {
        Plan::parse(layout::EXAMPLE)?
    } else {
        Plan::load(&made)?
    };

    // Keep a plan you wrote by hand, which has no copy in plans/ yet.
    let active = layout::plan_path();
    let keep = dir().join(format!("{current}.toml"));
    if active.exists() && !keep.exists() {
        std::fs::create_dir_all(dir())?;
        std::fs::copy(&active, &keep).with_context(|| format!("keeping {}", keep.display()))?;
    }

    let back = parked(progress, &want);
    let resumed = back.exists();
    let carried = !resumed && progress.exists() && same_stages(layout::plan(), &plan);
    if progress.exists() && !carried {
        let to = parked(progress, &current);
        std::fs::create_dir_all(to.parent().expect("has a parent"))?;
        std::fs::rename(progress, &to).with_context(|| format!("parking {}", to.display()))?;
    }
    if resumed {
        std::fs::rename(&back, progress)
            .with_context(|| format!("restoring {}", back.display()))?;
    }

    if builtin {
        if active.exists() {
            std::fs::remove_file(&active)?;
        }
    } else {
        activate(&made)?;
    }
    if carried {
        return Ok(format!(
            "Now practising {}. Its stages are the same as {current}'s, so your progress came with you.",
            plan.name
        ));
    }
    Ok(format!(
        "Now practising {}{}. Your progress on {current} is kept for when you switch back.",
        plan.name,
        if resumed {
            ", where you left off"
        } else {
            " from the start"
        }
    ))
}

/// Two plans whose stages have exactly the same keys share progress.
fn same_stages(a: &Plan, b: &Plan) -> bool {
    a.stages.len() == b.stages.len()
        && a.stages
            .iter()
            .zip(&b.stages)
            .all(|(x, y)| x.keys == y.keys)
}

/// Gives a plan you made a new name: inside its file (the plan's name, stage
/// names that use it, the header), its summary, and the file names, along with
/// the copy you're practising and any progress kept for it.
pub fn rename(old: &str, new: &str, progress: &Path) -> Result<String> {
    rename_in(&dir(), &layout::plan_path(), progress, old, new)
}

fn rename_in(dir: &Path, active: &Path, progress: &Path, old: &str, new: &str) -> Result<String> {
    let new = new.trim();
    if new.is_empty() {
        bail!("give the plan a name");
    }
    let (from, to) = (slug(old), slug(new));
    let made = dir.join(format!("{from}.toml"));
    if !made.exists() {
        if from == builtin_slug() {
            bail!("the built-in plan can't be renamed; make your own with `very-normal make-plan`");
        }
        bail!("no plan called {old}");
    }
    let plan = Plan::load(&made)?;
    let target = dir.join(format!("{to}.toml"));
    if to != from && (target.exists() || to == builtin_slug()) {
        bail!("there's already a plan called {new}");
    }
    let is_active = active.exists() && Plan::load(active).is_ok_and(|a| slug(&a.name) == from);

    let text = renamed_plan(&std::fs::read_to_string(&made)?, &plan.name, new);
    Plan::parse(&text).context("the renamed plan doesn't load")?;
    std::fs::write(&target, &text).with_context(|| format!("writing {}", target.display()))?;
    if is_active {
        std::fs::write(active, &text).with_context(|| format!("writing {}", active.display()))?;
    }
    let summary = summary_of(&made);
    if summary.exists() {
        let text = std::fs::read_to_string(&summary)?.replace(&plan.name, new);
        std::fs::write(summary_of(&target), text)?;
    }
    let kept = progress
        .with_file_name("plans")
        .join(format!("{from}.json"));
    if kept.exists() && to != from {
        std::fs::rename(&kept, kept.with_file_name(format!("{to}.json")))?;
    }
    if to != from {
        std::fs::remove_file(&made)?;
        if summary.exists() {
            std::fs::remove_file(&summary)?;
        }
    }
    Ok(format!(
        "{} is now {new}{}.",
        plan.name,
        if is_active {
            ", and it's still the plan you're practising"
        } else {
            ""
        }
    ))
}

/// A plan file's text with `old` renamed to `new` in its `name = ` lines and
/// its header comment, leaving drill words and everything else alone.
fn renamed_plan(text: &str, old: &str, new: &str) -> String {
    let quoted = |s: &str| toml::Value::String(s.to_string()).to_string();
    let mut out: String = text
        .lines()
        .map(|line| {
            let t = line.trim_start();
            let is_name = t.starts_with("name") && t["name".len()..].trim_start().starts_with('=');
            if is_name && line.contains(old) {
                let indent = &line[..line.len() - t.len()];
                let value = t.split_once('=').map_or("", |(_, v)| v.trim());
                let current = toml::from_str::<toml::Table>(&format!("v = {value}"))
                    .ok()
                    .and_then(|v| v.get("v")?.as_str().map(String::from))
                    .unwrap_or_default();
                format!("{indent}name = {}", quoted(&current.replace(old, new)))
            } else if t.starts_with('#') {
                line.replace(&format!(" {old},"), &format!(" {new},"))
            } else {
                line.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    if text.ends_with('\n') {
        out.push('\n');
    }
    out
}

/// Deletes a plan you made, its summary, and any progress kept for it.
pub fn remove(name: &str, progress: &Path) -> Result<String> {
    let want = slug(name);
    if want == active_slug() {
        bail!(
            "{} is the plan you're practising; switch to another with `very-normal use` first",
            layout::plan().name
        );
    }
    let made = dir().join(format!("{want}.toml"));
    if !made.exists() {
        if want == builtin_slug() {
            bail!("the built-in plan can't be removed");
        }
        bail!("no plan called {name}; you have: {}", list().join(", "));
    }
    let mut gone = vec![made.clone()];
    std::fs::remove_file(&made)?;
    for extra in [summary_of(&made), parked(progress, &want)] {
        if extra.exists() {
            std::fs::remove_file(&extra)?;
            gone.push(extra);
        }
    }
    Ok(format!(
        "Removed {}.",
        gone.iter()
            .map(|p| p.display().to_string())
            .collect::<Vec<_>>()
            .join(", ")
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugs_are_file_names() {
        assert_eq!(slug("Home Row Hero"), "home-row-hero");
        assert_eq!(slug("  Very  Normal! "), "very-normal");
        assert_eq!(slug("Mark's"), "mark-s");
        assert_eq!(slug("!!"), "plan");
    }

    #[test]
    fn renames_a_plan_everywhere_it_goes_by_name() {
        let root = std::env::temp_dir().join(format!("very-normal-rename-{}", std::process::id()));
        let plans = root.join("plans");
        std::fs::create_dir_all(&plans).unwrap();
        let example = Plan::parse(layout::EXAMPLE).unwrap();
        let old = "Take Two";
        let text = format!(
            "# A very-normal plan for {old}, made by `very-normal make-plan`\n{}",
            layout::EXAMPLE
                .replacen(
                    &format!("name = \"{}\"", example.name),
                    &format!("name = \"{old}\""),
                    1
                )
                .replace(&format!("Fully {}", example.name), &format!("Fully {old}"))
        );
        std::fs::write(plans.join("take-two.toml"), &text).unwrap();
        std::fs::write(
            plans.join("take-two.md"),
            format!("# {old}\n\"name\":\"{old}\""),
        )
        .unwrap();
        let active = root.join("plan.toml");
        std::fs::write(&active, &text).unwrap();
        let progress = root.join("progress.json");
        let kept = root.join("plans/take-two.json");
        std::fs::write(&kept, "{}").unwrap();

        let msg = rename_in(&plans, &active, &progress, "take two", "Home Row Glow").unwrap();
        assert!(msg.contains("still the plan you're practising"), "{msg}");
        let renamed = Plan::load(&plans.join("home-row-glow.toml")).unwrap();
        assert_eq!(renamed.name, "Home Row Glow");
        assert!(
            renamed
                .stages
                .iter()
                .any(|s| s.name == "Fully Home Row Glow"),
            "{:?}",
            renamed.stages.last().map(|s| &s.name)
        );
        assert_eq!(
            renamed.stages[1].keys, example.stages[1].keys,
            "drills and keys stay put"
        );
        assert_eq!(Plan::load(&active).unwrap().name, "Home Row Glow");
        let summary = std::fs::read_to_string(plans.join("home-row-glow.md")).unwrap();
        assert!(
            summary.starts_with("# Home Row Glow") && !summary.contains(old),
            "{summary}"
        );
        assert!(
            std::fs::read_to_string(plans.join("home-row-glow.toml"))
                .unwrap()
                .starts_with("# A very-normal plan for Home Row Glow,")
        );
        assert!(root.join("plans/home-row-glow.json").exists());
        assert!(!plans.join("take-two.toml").exists() && !kept.exists());

        assert!(
            rename_in(&plans, &active, &progress, "take two", "x").is_err(),
            "it's gone"
        );
        std::fs::write(
            plans.join("other.toml"),
            layout::EXAMPLE.replacen(
                &format!("name = \"{}\"", example.name),
                "name = \"Other\"",
                1,
            ),
        )
        .unwrap();
        let err = rename_in(&plans, &active, &progress, "other", "home row glow").unwrap_err();
        assert!(err.to_string().contains("already a plan"), "{err}");
        std::fs::remove_dir_all(root).ok();
    }

    #[test]
    fn starts_from_a_known_layout_a_plan_file_or_your_own_keys() {
        let example = Plan::parse(layout::EXAMPLE).unwrap();
        let file =
            std::env::temp_dir().join(format!("very-normal-from-{}.toml", std::process::id()));
        std::fs::write(&file, layout::EXAMPLE).unwrap();
        let f = file.display().to_string();

        let (name, keys) = start(&f).unwrap();
        assert_eq!(name, example.name, "a plan means where it ends");
        assert_eq!(keys, example.stages.last().unwrap().keys);

        let (name, keys) = start(&format!("{f}@1")).unwrap();
        assert_eq!(name, format!("{}, stage 1", example.name));
        assert_eq!(keys, example.stages[1].keys);

        assert!(
            start(&format!("{f}@99"))
                .unwrap_err()
                .to_string()
                .contains("stages 0 to")
        );
        assert!(start("/nowhere/at/all.toml").is_err());
        assert!(
            start(&format!("{f}:1")).is_err(),
            ":1 isn't a stage, so that's no file"
        );
        if let Some(home) = std::env::var_os("HOME") {
            let home = PathBuf::from(home);
            if let Ok(rest) = file.strip_prefix(&home) {
                let (name, _) = start(&format!("~/{}@1", rest.display())).unwrap();
                assert_eq!(name, format!("{}, stage 1", example.name));
            }
        }
        assert_eq!(start("dvorak").unwrap().0, "Dvorak");
        assert_eq!(
            start("qwertyuiop asdfghjkl; zxcvbnm,./").unwrap().1,
            named::QWERTY
        );
        let err = start("azerty-ish").unwrap_err().to_string();
        assert!(err.contains("a plan"), "{err}");
        std::fs::remove_file(file).ok();
    }
}
