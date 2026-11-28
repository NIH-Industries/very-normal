//! Where your keyboard's layout lives, and what it says about the stage.
//!
//! very-normal doesn't need to know what keyboard you have. To check a stage
//! landed it reads the layout from wherever you keep it: a `layout.txt` you
//! build your firmware from, or an Oryx layout for a ZSA board. Either way it
//! comes back as a [`Snapshot`] of the 30 letter keys, the home row holds and
//! the mirror layer.

use std::path::PathBuf;

use anyhow::{Result, bail};

use crate::{layout, layout_file, oryx};

#[derive(Debug, Clone, PartialEq)]
pub enum Keyboard {
    /// A `layout.txt`, drawn in the keyboard's shape.
    File(PathBuf),
    /// An Oryx layout id, for a ZSA board.
    Oryx(String),
    /// Nothing set: very-normal can say what to change, but not check it.
    Unset,
}

/// `$VERY_NORMAL_LAYOUT` (a path to a layout.txt, or an Oryx layout id), or
/// the plan's `layout_file`, or its `oryx_layout`.
pub fn configured() -> Keyboard {
    let plan = layout::plan();
    from_settings(
        std::env::var("VERY_NORMAL_LAYOUT").ok().as_deref(),
        plan.layout_file.as_deref(),
        plan.oryx_layout.as_deref(),
    )
}

fn from_settings(env: Option<&str>, file: Option<&str>, oryx: Option<&str>) -> Keyboard {
    let env = env.filter(|s| !s.is_empty());
    if let Some(v) = env {
        return if looks_like_path(v) {
            Keyboard::File(expand(v))
        } else {
            Keyboard::Oryx(v.to_string())
        };
    }
    match (
        file.filter(|s| !s.is_empty()),
        oryx.filter(|s| !s.is_empty()),
    ) {
        (Some(f), _) => Keyboard::File(expand(f)),
        (None, Some(id)) => Keyboard::Oryx(id.to_string()),
        (None, None) => Keyboard::Unset,
    }
}

fn looks_like_path(v: &str) -> bool {
    v.contains('/') || v.contains('.')
}

/// `~/` means your home folder.
pub fn expand(path: &str) -> PathBuf {
    match (path.strip_prefix("~/"), std::env::var_os("HOME")) {
        (Some(rest), Some(home)) => PathBuf::from(home).join(rest),
        _ => PathBuf::from(path),
    }
}

impl Keyboard {
    pub fn read(&self) -> Result<Snapshot> {
        match self {
            Keyboard::File(path) => layout_file::read(path),
            Keyboard::Oryx(id) => oryx::fetch(id),
            Keyboard::Unset => bail!(
                "no keyboard layout to check: set layout_file (a layout.txt) or oryx_layout in your plan, or $VERY_NORMAL_LAYOUT"
            ),
        }
    }

    /// Where you make the edits: the file, or the Oryx page.
    pub fn place(&self) -> String {
        match self {
            Keyboard::File(path) => path.display().to_string(),
            Keyboard::Oryx(id) => oryx::edit_url(id),
            Keyboard::Unset => "your keyboard's layout".to_string(),
        }
    }

    /// Opens the file or page where you make the edits.
    pub fn open(&self) -> Result<String> {
        if *self == Keyboard::Unset {
            bail!("no keyboard layout set in your plan");
        }
        let place = self.place();
        crate::open_url(&place)?;
        Ok(place)
    }
}

/// Where each of the 30 letter positions sits in a Voyager-shaped list of
/// keys: rows of six per half, the left half's outer column first. Oryx lists
/// the left half then the right; a layout.txt goes row by row.
pub const ROW_OF_TWELVE: [usize; 30] = [
    1, 2, 3, 4, 5, 6, 7, 8, 9, 10, //
    13, 14, 15, 16, 17, 18, 19, 20, 21, 22, //
    25, 26, 27, 28, 29, 30, 31, 32, 33, 34,
];

/// The hold each home row key should have for the plan's home row mods, as
/// `LEFT_CTRL`, `RIGHT_GUI` and so on.
pub fn expected_holds() -> Vec<(usize, String)> {
    (10..20)
        .filter_map(|i| {
            let side = if i % 10 < 5 { "LEFT" } else { "RIGHT" };
            let code = match layout::modifier(i)? {
                "ctrl" => "CTRL",
                "alt" => "ALT",
                "cmd" => "GUI",
                _ => "SHIFT",
            };
            Some((i, format!("{side}_{code}")))
        })
        .collect()
}

/// A plan's `home_row_mods` name for a hold like `LEFT_CTRL`.
pub fn mod_name(hold: &str) -> &'static str {
    if hold.ends_with("CTRL") {
        "ctrl"
    } else if hold.ends_with("ALT") {
        "alt"
    } else if hold.ends_with("GUI") {
        "cmd"
    } else if hold.ends_with("SHIFT") {
        "shift"
    } else {
        ""
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Snapshot {
    /// Where it came from, in a line: the file, or the Oryx layout and revision.
    pub source: String,
    /// The main layer's 30 letter keys; `?` where a key isn't a plain character.
    pub main: Vec<char>,
    /// What the home row mod keys do when held.
    pub holds: Vec<(usize, String)>,
    /// The mirror layer's 15 left-hand letter keys, if there is a mirror layer.
    pub mirror: Option<Vec<char>>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Finding {
    Key {
        index: usize,
        got: char,
        want: char,
    },
    Hold {
        index: usize,
        got: String,
        want: String,
    },
    Mirror {
        index: usize,
        got: char,
        want: char,
    },
}

impl std::fmt::Display for Finding {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Finding::Key { index, got, want } => write!(
                f,
                "main layer: the key that started as {} types {got}, should type {want}",
                layout::keys(0)[*index]
            ),
            Finding::Hold { index, got, want } => write!(
                f,
                "home row mod: the key that started as {} holds {}, should hold {want}",
                layout::keys(0)[*index],
                if got.is_empty() { "nothing" } else { got }
            ),
            Finding::Mirror { index, got, want } => write!(
                f,
                "mirror layer: the left key that started as {} types {got}, should type {want}",
                layout::keys(0)[*index]
            ),
        }
    }
}

impl Snapshot {
    pub fn main_string(&self) -> String {
        self.main.iter().collect()
    }

    /// The stage the main layer matches, if it matches one exactly.
    pub fn stage(&self) -> Option<usize> {
        layout::find_stage(&self.main_string())
    }

    /// Everything that differs from the given stage.
    pub fn compare(&self, stage: usize) -> Vec<Finding> {
        let want = layout::keys(stage);
        let mut out: Vec<Finding> = (0..30)
            .filter(|i| self.main[*i] != want[*i])
            .map(|index| Finding::Key {
                index,
                got: self.main[index],
                want: want[index],
            })
            .collect();
        for ((index, got), (_, want)) in self.holds.iter().zip(expected_holds()) {
            if *got != want {
                out.push(Finding::Hold {
                    index: *index,
                    got: got.clone(),
                    want,
                });
            }
        }
        if let Some(mirror) = &self.mirror {
            for (n, got) in mirror.iter().enumerate() {
                let index = (n / 5) * 10 + n % 5;
                let want = want[layout::mirror(index)];
                if *got != want {
                    out.push(Finding::Mirror {
                        index,
                        got: *got,
                        want,
                    });
                }
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_environment_beats_the_plan() {
        assert_eq!(
            from_settings(Some("abc12"), Some("/k/layout.txt"), Some("Xx6pM")),
            Keyboard::Oryx("abc12".into())
        );
        assert_eq!(
            from_settings(Some("/k/layout.txt"), None, Some("Xx6pM")),
            Keyboard::File("/k/layout.txt".into())
        );
    }

    #[test]
    fn a_layout_file_beats_an_oryx_layout() {
        assert_eq!(
            from_settings(None, Some("/k/layout.txt"), Some("Xx6pM")),
            Keyboard::File("/k/layout.txt".into())
        );
        assert_eq!(
            from_settings(Some(""), None, Some("Xx6pM")),
            Keyboard::Oryx("Xx6pM".into())
        );
        assert_eq!(from_settings(None, Some(""), None), Keyboard::Unset);
    }

    #[test]
    fn nothing_set_says_how_to_set_it() {
        let e = Keyboard::Unset.read().unwrap_err().to_string();
        assert!(e.contains("layout_file"), "{e}");
    }
}
