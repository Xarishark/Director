//! Command-line names: "Desk View" is `desk-view`, so names never need quotes.
//!
//! CLI names are derived from the saved name every time and never stored. Lookups accept an
//! exact ID or anything that normalizes to the same CLI name, so older quoted commands such
//! as `"Desk View"` keep working.

/// Lowercase letters and digits, with every run of other characters turned into one `-`.
/// Empty only for a name made entirely of symbols.
pub fn cli_name(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    let mut gap = false;
    for c in name.chars() {
        if c.is_alphanumeric() {
            if gap && !out.is_empty() {
                out.push('-');
            }
            gap = false;
            out.extend(c.to_lowercase());
        } else {
            gap = true;
        }
    }
    out
}

/// Find an item by exact ID, then by CLI name. `noun` names the kind ("display preset") and
/// `area` the CLI area ("display") for the error messages.
pub fn find<'a, T>(
    items: &'a [T],
    target: &str,
    key: impl Fn(&T) -> (&str, &str),
    noun: &str,
    area: &str,
) -> Result<&'a T, String> {
    if let Some(item) = items.iter().find(|i| key(i).0 == target) {
        return Ok(item);
    }
    let wanted = cli_name(target);
    let matches: Vec<&T> = if wanted.is_empty() {
        Vec::new()
    } else {
        items
            .iter()
            .filter(|i| cli_name(key(i).1) == wanted)
            .collect()
    };
    match matches.as_slice() {
        [one] => Ok(one),
        [] => Err(format!(
            "No {noun} named {}. Run director {area} list.",
            if wanted.is_empty() { target } else { &wanted }
        )),
        many => Err(format!(
            "More than one {noun} uses the command name {wanted}. Use its ID instead: {}.",
            many.iter()
                .map(|i| format!("{} ({})", key(i).0, key(i).1))
                .collect::<Vec<_>>()
                .join(", ")
        )),
    }
}

/// Refuse new or renamed items whose CLI name is empty or already used by another item of
/// the same kind. Unchanged items are not checked, so files that already contain a clash
/// keep loading and saving; the clash is reported when someone looks the name up.
pub fn check_new_names<T>(
    expected: &[T],
    next: &[T],
    key: impl Fn(&T) -> (&str, &str),
    noun: &str,
) -> Result<(), String> {
    for item in next {
        let (id, name) = key(item);
        if expected
            .iter()
            .any(|old| key(old).0 == id && key(old).1 == name)
        {
            continue;
        }
        let wanted = cli_name(name);
        if wanted.is_empty() {
            return Err(format!(
                "Give the {noun} a name with at least one letter or number."
            ));
        }
        if next
            .iter()
            .any(|other| key(other).0 != id && cli_name(key(other).1) == wanted)
        {
            return Err(format!(
                "Another {noun} already uses the command name {wanted}. Choose a different name."
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn key(i: &(String, String)) -> (&str, &str) {
        (&i.0, &i.1)
    }
    fn item(id: &str, name: &str) -> (String, String) {
        (id.into(), name.into())
    }
    #[test]
    fn names_become_lowercase_dashed_words() {
        assert_eq!(cli_name("Desk View"), "desk-view");
        assert_eq!(cli_name("TV “Gaming”"), "tv-gaming");
        assert_eq!(
            cli_name("Speakers (Realtek(R) Audio)"),
            "speakers-realtek-r-audio"
        );
        assert_eq!(cli_name("  4K / 120 Hz "), "4k-120-hz");
        assert_eq!(cli_name("Σαλόνι TV"), "σαλόνι-tv");
        assert_eq!(cli_name("desk-view"), "desk-view");
        assert_eq!(cli_name("★ ★"), "");
    }
    #[test]
    fn find_prefers_ids_then_cli_names_and_reports_clashes() {
        let items = vec![
            item("1", "Desk View"),
            item("desk-view", "Other"),
            item("3", "TV"),
        ];
        assert_eq!(
            find(&items, "desk-view", key, "scene", "scene").unwrap().0,
            "desk-view"
        );
        assert_eq!(
            find(&items, "Desk View", key, "scene", "scene").unwrap().0,
            "1"
        );
        assert_eq!(find(&items, "tv", key, "scene", "scene").unwrap().0, "3");
        assert_eq!(find(&items, "3", key, "scene", "scene").unwrap().0, "3");
        let missing = find(&items, "Couch Mode", key, "scene", "scene").unwrap_err();
        assert!(missing.contains("couch-mode") && missing.contains("director scene list"));
        assert!(find(&items, "★", key, "scene", "scene").is_err());
        let clash = vec![item("a", "Desk View"), item("b", "desk-view")];
        let error = find(&clash, "desk-view", key, "scene", "scene").unwrap_err();
        assert!(error.contains("a (Desk View)") && error.contains("b (desk-view)"));
        assert_eq!(
            find(&clash, "b", key, "scene", "scene").unwrap().1,
            "desk-view"
        );
    }
    #[test]
    fn saving_refuses_new_clashes_but_keeps_old_ones() {
        let old = vec![item("a", "Desk View"), item("b", "desk-view")];
        assert!(check_new_names(&old, &old, key, "scene").is_ok());
        let mut added = old.clone();
        added.push(item("c", "DESK  view"));
        assert!(
            check_new_names(&old, &added, key, "scene")
                .unwrap_err()
                .contains("desk-view")
        );
        let renamed = vec![item("a", "Desk View"), item("b", "Couch")];
        assert!(check_new_names(&old, &renamed, key, "scene").is_ok());
        let symbols = vec![item("a", "★")];
        assert!(
            check_new_names(&[], &symbols, key, "scene")
                .unwrap_err()
                .contains("letter or number")
        );
        let fresh = vec![item("x", "Desk"), item("y", "Couch")];
        assert!(check_new_names(&[], &fresh, key, "scene").is_ok());
    }
}
