//! Reading a `layout.txt`: a keyboard layout drawn in the keyboard's shape.
//!
//! Each `layer NAME` is four rows of twelve keys (six per half) and a row of
//! four thumb keys, as https://github.com/markryall/voyager-keymap builds
//! firmware from. The first layer is the main one, and a layer called
//! `mirror` is the mirror layer. A key is a character (`a`, `;`), maybe with
//! a hold: `s+ctl` holds Ctrl, `tab>numbers` holds a layer. `color` lines
//! and `lights` blocks are about LEDs, so they're skipped here.

use std::path::Path;

use anyhow::{Context, Result, bail};

use crate::keyboard::{ROW_OF_TWELVE, Snapshot, expected_holds, mod_name};

const ROWS: [usize; 5] = [12, 12, 12, 12, 4];

struct Layer {
    name: String,
    keys: Vec<String>,
}

pub fn read(path: &Path) -> Result<Snapshot> {
    let text = std::fs::read_to_string(path)
        .with_context(|| format!("reading your layout from {}", path.display()))?;
    parse(&text, &path.display().to_string())
}

/// What each of the ten home row keys does when held, as a plan's
/// `home_row_mods` ("ctrl", "alt", "cmd", "shift" or "").
pub fn home_row_mods(path: &Path) -> Result<Vec<String>> {
    let text = std::fs::read_to_string(path)
        .with_context(|| format!("reading your layout from {}", path.display()))?;
    let layers = layers(&text)?;
    Ok((10..20)
        .map(|i| mod_name(&hold(&layers[0].keys, ROW_OF_TWELVE[i])).to_string())
        .collect())
}

pub fn parse(text: &str, source: &str) -> Result<Snapshot> {
    let layers = layers(text)?;
    let main = &layers[0].keys;
    let mirror = layers
        .iter()
        .find(|l| l.name.eq_ignore_ascii_case("mirror"))
        .map(|l| {
            (0..3)
                .flat_map(|row| (0..5).map(move |col| row * 10 + col))
                .map(|i| character(&l.keys[ROW_OF_TWELVE[i]]))
                .collect()
        });
    Ok(Snapshot {
        source: source.to_string(),
        main: ROW_OF_TWELVE.iter().map(|s| character(&main[*s])).collect(),
        holds: expected_holds()
            .iter()
            .map(|(i, _)| (*i, hold(main, ROW_OF_TWELVE[*i])))
            .collect(),
        mirror,
    })
}

fn layers(text: &str) -> Result<Vec<Layer>> {
    let lines: Vec<(usize, &str)> = text
        .lines()
        .enumerate()
        .map(|(n, l)| (n + 1, l.trim()))
        .filter(|(_, l)| !l.is_empty() && !l.starts_with('#'))
        .collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let (n, line) = lines[i];
        let words: Vec<&str> = line.split_whitespace().collect();
        i += 1;
        match words[0] {
            "color" => {}
            "layer" | "lights" => {
                let mut keys = Vec::new();
                for want in ROWS {
                    let Some((rn, row)) = lines.get(i) else {
                        bail!("line {n}: the {} ends early", words[0]);
                    };
                    let got: Vec<&str> = row.split_whitespace().collect();
                    if got.len() != want {
                        bail!("line {rn}: {} keys, the row has {want}", got.len());
                    }
                    keys.extend(got.iter().map(|k| k.to_string()));
                    i += 1;
                }
                if words[0] == "layer" {
                    let Some(name) = words.get(1) else {
                        bail!("line {n}: a layer needs a name");
                    };
                    out.push(Layer {
                        name: name.to_string(),
                        keys,
                    });
                }
            }
            other => bail!("line {n}: expected `color`, `layer` or `lights`, got {other:?}"),
        }
    }
    if out.is_empty() {
        bail!("there are no layers in it");
    }
    Ok(out)
}

/// The key with any hold taken off: `s+ctl` is `s`, `tab>numbers` is `tab`.
fn tap(key: &str) -> &str {
    let first = key.chars().next().map_or(0, char::len_utf8);
    match key[first..].find(['>', '+']) {
        Some(at) => &key[..first + at],
        None => key,
    }
}

fn character(key: &str) -> char {
    let t = tap(key);
    let mut chars = t.chars();
    match (chars.next(), chars.next()) {
        (Some(c), None) if c != '_' => c.to_ascii_lowercase(),
        _ => '?',
    }
}

/// What the key at a position holds, as `LEFT_CTRL`; empty for nothing.
fn hold(keys: &[String], at: usize) -> String {
    let key = &keys[at];
    let t = tap(key);
    let Some(m) = key[t.len()..].strip_prefix('+') else {
        return String::new();
    };
    let left = at % 12 < 6;
    let (side, m) = match m.split_at_checked(1) {
        Some(("l", rest)) if is_mod(rest) => ("LEFT", rest),
        Some(("r", rest)) if is_mod(rest) => ("RIGHT", rest),
        _ => (if left { "LEFT" } else { "RIGHT" }, m),
    };
    let code = match m {
        "ctl" => "CTRL",
        "alt" => "ALT",
        "gui" => "GUI",
        "shift" => "SHIFT",
        _ => return String::new(),
    };
    format!("{side}_{code}")
}

fn is_mod(m: &str) -> bool {
    matches!(m, "ctl" | "alt" | "gui" | "shift")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keyboard::Finding;
    use crate::layout;

    fn fixture() -> String {
        let path = format!(
            "{}/tests/fixtures/layout-stage1.txt",
            env!("CARGO_MANIFEST_DIR")
        );
        std::fs::read_to_string(path).unwrap()
    }

    #[test]
    fn reads_the_very_normal_layout_as_stage_one() {
        let s = parse(&fixture(), "layout.txt").unwrap();
        assert_eq!(s.stage(), Some(1));
        assert!(s.compare(1).is_empty(), "{:?}", s.compare(1));
    }

    #[test]
    fn lists_what_stage_two_still_needs() {
        let s = parse(&fixture(), "layout.txt").unwrap();
        let f = s.compare(2);
        let keys = f
            .iter()
            .filter(|f| matches!(f, Finding::Key { .. }))
            .count();
        let mirror = f
            .iter()
            .filter(|f| matches!(f, Finding::Mirror { .. }))
            .count();
        assert_eq!(keys, layout::changes(2).len());
        assert_eq!(
            mirror,
            layout::changes(2).iter().filter(|c| c.right_hand()).count()
        );
    }

    #[test]
    fn reads_home_row_mods_for_a_new_plan() {
        let path = format!(
            "{}/tests/fixtures/layout-stage1.txt",
            env!("CARGO_MANIFEST_DIR")
        );
        assert_eq!(
            home_row_mods(Path::new(&path)).unwrap(),
            ["", "ctrl", "alt", "cmd", "", "", "cmd", "alt", "ctrl", ""]
        );
    }

    #[test]
    fn holds_take_their_side_from_the_board_unless_named() {
        let keys: Vec<String> = ["a+ctl", "b+rgui", "c>nav", "d"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        assert_eq!(hold(&keys, 0), "LEFT_CTRL");
        assert_eq!(hold(&keys, 1), "RIGHT_GUI");
        assert_eq!(hold(&keys, 2), "");
        assert_eq!(character(&keys[2]), 'c');
    }

    #[test]
    fn symbols_that_look_like_holds_are_still_keys() {
        assert_eq!(character("+"), '+');
        assert_eq!(character(">"), '>');
        assert_eq!(character(";"), ';');
        assert_eq!(character("_"), '?');
        assert_eq!(character("bspc>vim"), '?');
    }

    #[test]
    fn a_short_row_says_where() {
        let e = parse("layer main\n  a b c\n", "x").unwrap_err().to_string();
        assert!(e.contains("line 2"), "{e}");
    }
}
