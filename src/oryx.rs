//! Reading a layout from Oryx, ZSA's configurator, for a ZSA board's
//! [`crate::keyboard::Keyboard::Oryx`].
//!
//! Oryx has no documented API; this uses the GraphQL endpoint its own page
//! loads layouts from. It only works for layouts with a public link.

use std::process::Command;

use anyhow::{Context, Result, anyhow, bail};
use serde_json::Value;

use crate::keyboard::{Snapshot, expected_holds, mod_name};

pub fn edit_url(layout: &str) -> String {
    format!("https://configure.zsa.io/voyager/layouts/{layout}/latest/0")
}

/// Where each of the 30 letter positions sits in Oryx's key list for the
/// Voyager: the left half's rows of six (outer column first), then the
/// right half's rows of six (outer column last).
const SLOTS: [usize; 30] = [
    1, 2, 3, 4, 5, 26, 27, 28, 29, 30, //
    7, 8, 9, 10, 11, 32, 33, 34, 35, 36, //
    13, 14, 15, 16, 17, 38, 39, 40, 41, 42,
];

pub fn fetch(layout: &str) -> Result<Snapshot> {
    parse(&query(layout)?)
}

/// What each of the ten home row keys does when held, as a plan's
/// `home_row_mods` ("ctrl", "alt", "cmd", "shift" or "").
pub fn home_row_mods(layout: &str) -> Result<Vec<String>> {
    holds_of(&query(layout)?)
}

fn holds_of(v: &Value) -> Result<Vec<String>> {
    let main = main_layer(v)?;
    Ok((10..20)
        .map(|i| mod_name(&code(&main[SLOTS[i]], "hold")).to_string())
        .collect())
}

fn query(layout: &str) -> Result<Value> {
    if layout.is_empty() {
        bail!("no Oryx layout id");
    }
    let query = format!(
        "query {{ layout(hashId: \"{layout}\", geometry: \"voyager\", revisionId: \"latest\") \
         {{ title revision {{ hashId createdAt layers {{ title position keys }} }} }} }}"
    );
    let body = serde_json::json!({ "query": query }).to_string();
    let out = Command::new("curl")
        .args(["-sS", "--max-time", "20", "https://oryx.zsa.io/graphql"])
        .args(["-H", "Content-Type: application/json", "-d", &body])
        .output()
        .context("running curl")?;
    if !out.status.success() {
        bail!(
            "curl failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    serde_json::from_slice(&out.stdout).context("Oryx sent something that isn't JSON")
}

fn layout_of(v: &Value) -> Result<&Value> {
    v.pointer("/data/layout")
        .filter(|l| !l.is_null())
        .ok_or_else(|| anyhow!("Oryx didn't return that layout. Is its link public?"))
}

fn keys_of(layer: &Value) -> Vec<Value> {
    layer
        .get("keys")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
}

fn layers_of(v: &Value) -> Result<&Vec<Value>> {
    layout_of(v)?
        .pointer("/revision/layers")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow!("the layout has no layers"))
}

fn main_layer(v: &Value) -> Result<Vec<Value>> {
    let main = layers_of(v)?
        .iter()
        .find(|l| l.get("position").and_then(Value::as_u64) == Some(0))
        .map(keys_of)
        .ok_or_else(|| anyhow!("the layout has no main layer"))?;
    if main.len() < 43 {
        bail!(
            "the main layer has {} keys; is this a Voyager layout?",
            main.len()
        );
    }
    Ok(main)
}

fn code(key: &Value, part: &str) -> String {
    key.get(part)
        .and_then(|p| p.get("code"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim_start_matches("KC_")
        .to_string()
}

fn character(key: &Value) -> char {
    let c = code(key, "tap");
    match c.as_str() {
        "SCLN" => ';',
        "COMMA" => ',',
        "DOT" => '.',
        "SLASH" => '/',
        "QUOTE" => '\'',
        s if s.len() == 1 => s.chars().next().unwrap().to_ascii_lowercase(),
        _ => '?',
    }
}

pub fn parse(v: &Value) -> Result<Snapshot> {
    let layout = layout_of(v)?;
    let layers = layers_of(v)?;
    let main = main_layer(v)?;
    let mirror = layers
        .iter()
        .find(|l| {
            l.get("title")
                .and_then(Value::as_str)
                .is_some_and(|t| t.eq_ignore_ascii_case("mirror"))
        })
        .map(keys_of)
        .filter(|k| k.len() >= 43)
        .map(|k| {
            (0..3)
                .flat_map(|row| (0..5).map(move |col| row * 10 + col))
                .map(|i| character(&k[SLOTS[i]]))
                .collect()
        });
    let text = |p: &str| {
        layout
            .pointer(p)
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string()
    };
    Ok(Snapshot {
        source: format!(
            "Oryx: {} · revision {} · {}",
            text("/title"),
            text("/revision/hashId"),
            text("/revision/createdAt")
        ),
        main: SLOTS.iter().map(|s| character(&main[*s])).collect(),
        holds: expected_holds()
            .iter()
            .map(|(i, _)| (*i, code(&main[SLOTS[*i]], "hold")))
            .collect(),
        mirror,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keyboard::Finding;
    use crate::layout;

    fn fixture(name: &str) -> Snapshot {
        let path = format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
        let v: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        parse(&v).unwrap()
    }

    #[test]
    fn reads_the_planck_layout_as_stage_zero() {
        let s = fixture("oryx-stage0.json");
        assert!(s.source.contains("voyager-planck"), "{}", s.source);
        assert_eq!(s.stage(), Some(0));
        assert!(s.compare(0).is_empty());
    }

    #[test]
    fn reads_the_very_normal_layout_as_stage_one() {
        let s = fixture("oryx-stage1.json");
        assert_eq!(s.stage(), Some(1));
        assert!(s.compare(1).is_empty(), "{:?}", s.compare(1));
    }

    #[test]
    fn lists_what_stage_two_still_needs() {
        let s = fixture("oryx-stage1.json");
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
            "{}/tests/fixtures/oryx-stage1.json",
            env!("CARGO_MANIFEST_DIR")
        );
        let v: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        assert_eq!(
            holds_of(&v).unwrap(),
            ["", "ctrl", "alt", "cmd", "", "", "cmd", "alt", "ctrl", ""]
        );
    }

    #[test]
    fn a_missing_layout_is_explained() {
        let v = serde_json::json!({"data": {"layout": null}});
        assert!(parse(&v).unwrap_err().to_string().contains("public"));
    }
}
