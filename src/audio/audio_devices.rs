//! Remembered endpoint identities and explicit replacement links.
use crate::audio::Output;
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs::{self, OpenOptions},
    io::Read,
};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Identity {
    pub id: String,
    pub name: String,
    pub stable: Option<String>,
    pub instance: Option<String>,
}

#[derive(Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct Inventory {
    pub outputs: BTreeMap<String, Identity>,
    pub inputs: BTreeMap<String, Identity>,
    output_links: BTreeMap<String, String>,
    input_links: BTreeMap<String, String>,
}
impl Inventory {
    pub fn devices(&self, input: bool) -> &BTreeMap<String, Identity> {
        if input { &self.inputs } else { &self.outputs }
    }

    pub fn linked_id<'a>(&'a self, id: &'a str, input: bool) -> Option<&'a str> {
        let links = if input {
            &self.input_links
        } else {
            &self.output_links
        };
        let mut target = id;
        for _ in 0..=links.len() {
            match links.get(target) {
                Some(next) => target = next,
                None => break,
            }
        }
        (!links.contains_key(target)).then_some(target)
    }

    pub fn references<'a>(&'a self, mut id: &'a str, target: &str, input: bool) -> bool {
        let links = if input {
            &self.input_links
        } else {
            &self.output_links
        };
        for _ in 0..=links.len() {
            if id == target {
                return true;
            }
            let Some(next) = links.get(id) else { break };
            id = next;
        }
        false
    }

    pub fn resolve(&self, id: &str, input: bool, live: &[Identity]) -> Option<String> {
        let target = self.linked_id(id, input)?;
        if let Some(exact) = live.iter().find(|device| device.id == target) {
            return Some(exact.id.clone());
        }
        let saved = self.devices(input).get(target)?;
        if let Some(stable) = &saved.stable {
            let mut matches = live.iter().filter(|d| d.stable.as_ref() == Some(stable));
            if let Some(first) = matches.next() {
                return matches.next().is_none().then(|| first.id.clone());
            }
        }
        let instance = saved.instance.as_ref()?;
        let mut matches = live.iter().filter(|d| {
            d.instance
                .as_ref()
                .is_some_and(|s| s.eq_ignore_ascii_case(instance))
        });
        let first = matches.next()?;
        if matches.next().is_some()
            || matches!((&saved.stable, &first.stable), (Some(old), Some(new)) if old != new)
        {
            return None;
        }
        Some(first.id.clone())
    }

    pub fn picker(&self, input: bool, live: &[Identity], mut devices: Vec<Output>) -> Vec<Output> {
        for saved in self.devices(input).values() {
            if !devices.iter().any(|d| d.id == saved.id) {
                let state = match self.resolve(&saved.id, input, live) {
                    Some(id) if devices.iter().any(|d| d.id == id && d.active) => "Reconnected",
                    Some(_) => "Offline",
                    None => "Remembered · offline",
                };
                devices.push(Output {
                    id: saved.id.clone(),
                    name: saved.name.clone(),
                    state: state.into(),
                    active: false,
                    console: false,
                    multimedia: false,
                    communications: false,
                    volume_percent: None,
                    muted: None,
                });
            }
        }
        devices.sort_by_key(|d| (!d.active, d.name.to_lowercase(), d.id.clone()));
        devices
    }

    pub fn remember(input: bool, live: &[Identity]) -> Result<Self, String> {
        update(|inventory| {
            let devices = if input {
                &mut inventory.inputs
            } else {
                &mut inventory.outputs
            };
            for identity in live {
                let mut observed = identity.clone();
                if let Some(previous) = devices.get(&identity.id) {
                    observed.stable = observed.stable.or_else(|| previous.stable.clone());
                    observed.instance = observed.instance.or_else(|| previous.instance.clone());
                }
                devices.insert(identity.id.clone(), observed);
            }
            Ok(())
        })
    }

    pub fn relink(input: bool, old: Identity, replacement: &str) -> Result<(), String> {
        update(|inventory| {
            let links = if input {
                &mut inventory.input_links
            } else {
                &mut inventory.output_links
            };
            let mut target = replacement;
            for _ in 0..=links.len() {
                if target == old.id {
                    return Err("That replacement would create a circular device link.".into());
                }
                match links.get(target) {
                    Some(next) => target = next,
                    None => break,
                }
            }
            links.insert(old.id.clone(), replacement.into());
            let devices = if input {
                &mut inventory.inputs
            } else {
                &mut inventory.outputs
            };
            devices.entry(old.id.clone()).or_insert(old);
            Ok(())
        })
        .map(|_| ())
    }
}

fn read_inventory(path: &std::path::Path) -> Result<Inventory, String> {
    let file = match fs::File::open(path) {
        Ok(file) => file,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Inventory::default()),
        Err(e) => return Err(e.to_string()),
    };
    let mut bytes = Vec::new();
    file.take(4 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > 4 * 1024 * 1024 {
        return Err("Remembered audio inventory is too large.".into());
    }
    serde_json::from_slice(&bytes)
        .map_err(|e| format!("Could not read remembered audio devices: {e}"))
}

fn update(edit: impl FnOnce(&mut Inventory) -> Result<(), String>) -> Result<Inventory, String> {
    let path = crate::storage::settings_path("audio-devices.json")?;
    fs::create_dir_all(path.parent().ok_or("Missing settings directory.")?)
        .map_err(|e| e.to_string())?;
    let lock = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(path.with_extension("lock"))
        .map_err(|e| e.to_string())?;
    lock.lock().map_err(|e| e.to_string())?;
    let old = read_inventory(&path)?;
    let mut next = old.clone();
    edit(&mut next)?;
    if next != old {
        crate::storage::write_document(&path, &next)?;
    }
    Ok(next)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn endpoint(id: &str, stable: Option<&str>) -> Identity {
        Identity {
            id: id.into(),
            name: "Speakers".into(),
            stable: stable.map(str::to_owned),
            instance: Some("endpoint-instance".into()),
        }
    }

    #[test]
    fn remembered_identity_requires_an_unambiguous_match_in_the_same_direction() {
        let mut inventory = Inventory::default();
        let saved = endpoint("old", Some("stable"));
        inventory.outputs.insert(saved.id.clone(), saved);
        let renamed = endpoint("new", Some("stable"));
        let conflict = endpoint("other", Some("different"));
        assert_eq!(
            inventory
                .resolve("old", false, &[renamed.clone(), conflict.clone()])
                .as_deref(),
            Some("new")
        );
        assert_eq!(
            inventory.resolve("old", true, std::slice::from_ref(&renamed)),
            None
        );
        assert_eq!(inventory.resolve("old", false, &[conflict]), None);
        assert_eq!(
            inventory.resolve(
                "old",
                false,
                &[renamed, endpoint("ambiguous", Some("stable"))]
            ),
            None
        );
        assert_eq!(
            inventory
                .resolve("old", false, &[endpoint("no-stable-id", None)])
                .as_deref(),
            Some("no-stable-id")
        );
    }
}
