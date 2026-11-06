//! `list` commands: every saved item and device with the CLI name that commands accept.
//! Read-only; nothing here changes a setting.
use crate::{
    audio::{AudioBackend, Output},
    cli_name::cli_name,
    displays, presets, scenes, tvs,
    windows_audio::WindowsAudio,
    windows_displays,
};
use serde::Serialize;
use serde_json::{Value, json};

/// What `director list` and each `<area> list` print.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Category {
    Scenes,
    DisplayPresets,
    AudioPresets,
    Tvs,
    Screens,
    AudioOutputs,
    AudioInputs,
}
impl Category {
    pub const ALL: [Category; 7] = [
        Self::Scenes,
        Self::DisplayPresets,
        Self::AudioPresets,
        Self::Tvs,
        Self::Screens,
        Self::AudioOutputs,
        Self::AudioInputs,
    ];
    fn title(self) -> &'static str {
        match self {
            Self::Scenes => "Scenes",
            Self::DisplayPresets => "Display presets",
            Self::AudioPresets => "Audio presets",
            Self::Tvs => "TVs",
            Self::Screens => "Screens",
            Self::AudioOutputs => "Audio outputs",
            Self::AudioInputs => "Audio inputs",
        }
    }
    fn key(self) -> &'static str {
        match self {
            Self::Scenes => "scenes",
            Self::DisplayPresets => "display_presets",
            Self::AudioPresets => "audio_presets",
            Self::Tvs => "tvs",
            Self::Screens => "screens",
            Self::AudioOutputs => "audio_outputs",
            Self::AudioInputs => "audio_inputs",
        }
    }
    fn empty(self) -> &'static str {
        match self {
            Self::Scenes => "No scenes yet. Create one on the Scenes page.",
            Self::DisplayPresets => {
                "No display presets yet. Save one in the window or with director display capture <name>."
            }
            Self::AudioPresets => "No audio presets yet. Create one in the window.",
            Self::Tvs => "No TVs yet. Add one on the TVs page.",
            Self::Screens => "Windows reported no screens.",
            Self::AudioOutputs => "Windows reported no audio outputs.",
            Self::AudioInputs => "Windows reported no audio inputs.",
        }
    }
}

/// One printed line: the CLI name, then the saved name and a short summary.
struct Row {
    cli: String,
    name: String,
    summary: String,
    /// A device Windows remembers that is not connected now.
    absent: bool,
}

/// Print one category, or JSON with a `cli_name` on each item.
pub fn run(category: Category, json: bool, all: bool) -> Result<i32, String> {
    let (rows, value) = collect(category)?;
    if json {
        print_json(&value)?;
    } else {
        print_rows(&rows, category, all);
    }
    Ok(0)
}

/// `director list`: every category, in sections. A category that can't be read is reported
/// in its section and the rest still print; the exit code is then 1.
pub fn everything(json: bool, all: bool) -> Result<i32, String> {
    let mut failed = false;
    let mut object = serde_json::Map::new();
    for (i, category) in Category::ALL.into_iter().enumerate() {
        match collect(category) {
            Ok((rows, value)) => {
                if json {
                    object.insert(category.key().into(), value);
                } else {
                    if i > 0 {
                        println!();
                    }
                    println!("{}", category.title());
                    print_rows(&rows, category, all);
                }
            }
            Err(e) => {
                failed = true;
                if json {
                    object.insert(category.key().into(), json!({ "error": e }));
                } else {
                    if i > 0 {
                        println!();
                    }
                    println!("{}", category.title());
                    println!("  {e}");
                }
            }
        }
    }
    if json {
        print_json(&Value::Object(object))?;
    }
    Ok(if failed { 1 } else { 0 })
}

fn print_json(value: &Value) -> Result<(), String> {
    println!(
        "{}",
        serde_json::to_string_pretty(value).map_err(|e| e.to_string())?
    );
    Ok(())
}

/// Devices Windows remembers but that are not connected are hidden unless `all` is set.
fn print_rows(rows: &[Row], category: Category, all: bool) {
    let shown: Vec<&Row> = rows.iter().filter(|r| all || !r.absent).collect();
    let hidden = rows.len() - shown.len();
    if shown.is_empty() && hidden == 0 {
        println!("  {}", category.empty());
        return;
    }
    for line in format_rows(&shown) {
        println!("{line}");
    }
    match hidden {
        0 => {}
        1 => println!("  1 device that is not present is hidden. Add --all to show it."),
        n => println!("  {n} devices that are not present are hidden. Add --all to show them."),
    }
}

/// CLI names in a column, so the name to type is easy to spot.
fn format_rows(rows: &[&Row]) -> Vec<String> {
    let shown = |r: &Row| {
        if r.cli.is_empty() {
            "-".to_string()
        } else {
            r.cli.clone()
        }
    };
    let width = rows
        .iter()
        .map(|r| shown(r).chars().count())
        .max()
        .unwrap_or(0);
    rows.iter()
        .map(|r| {
            let cli = shown(r);
            let pad = width.saturating_sub(cli.chars().count());
            let mut line = format!("  {cli}{}  {}", " ".repeat(pad), r.name);
            if !r.summary.is_empty() {
                line.push_str(" — ");
                line.push_str(&r.summary);
            }
            line
        })
        .collect()
}

/// `item` with a `cli_name` field added, so scripts can map the names they type to items.
fn with_cli_name(item: &impl Serialize, name: &str) -> Result<Value, String> {
    let mut map = serde_json::Map::new();
    map.insert("cli_name".into(), Value::String(cli_name(name)));
    match serde_json::to_value(item).map_err(|e| e.to_string())? {
        Value::Object(fields) => map.extend(fields),
        other => {
            map.insert("value".into(), other);
        }
    }
    Ok(Value::Object(map))
}

fn row(name: &str, summary: String) -> Row {
    Row {
        cli: cli_name(name),
        name: name.into(),
        summary,
        absent: false,
    }
}

fn collect(category: Category) -> Result<(Vec<Row>, Value), String> {
    match category {
        Category::Scenes => {
            let active = crate::scene_session::active();
            let all = scenes::load(&scenes::path()?)?;
            let rows = all
                .iter()
                .map(|s| {
                    let mut summary = scene_summary(s);
                    if active.as_deref() == Some(s.id.as_str()) {
                        summary = if summary.is_empty() {
                            "active".into()
                        } else {
                            format!("{summary} (active)")
                        };
                    }
                    row(&s.name, summary)
                })
                .collect();
            let values = all
                .iter()
                .map(|s| {
                    let mut v = with_cli_name(s, &s.name)?;
                    v["active"] = Value::Bool(active.as_deref() == Some(s.id.as_str()));
                    Ok(v)
                })
                .collect::<Result<Vec<_>, String>>()?;
            Ok((rows, Value::Array(values)))
        }
        Category::DisplayPresets => {
            let all = displays::load(&displays::path()?)?;
            let rows = all
                .iter()
                .map(|p| row(&p.name, p.layout.summary()))
                .collect();
            let values = all
                .iter()
                .map(|p| with_cli_name(p, &p.name))
                .collect::<Result<_, _>>()?;
            Ok((rows, Value::Array(values)))
        }
        Category::AudioPresets => {
            let all = presets::load(&presets::path()?)?;
            let rows = all
                .iter()
                .map(|p| {
                    row(
                        &p.name,
                        format!(
                            "{} · {}",
                            p.output_name,
                            p.volume_percent
                                .map_or("keep volume".into(), |v| format!("{v}%"))
                        ),
                    )
                })
                .collect();
            let values = all
                .iter()
                .map(|p| with_cli_name(p, &p.name))
                .collect::<Result<_, _>>()?;
            Ok((rows, Value::Array(values)))
        }
        Category::Tvs => {
            let all = tvs::load(&tvs::path()?)?;
            let rows = all.iter().map(|t| row(&t.name, tv_summary(t))).collect();
            // Never the whole record: it holds the encrypted pairing credential.
            let values = all
                .iter()
                .map(|t| {
                    json!({
                        "cli_name": cli_name(&t.name),
                        "id": t.id,
                        "name": t.name,
                        "address": t.address,
                        "paired": t.credential.is_some(),
                        "wake_available": !t.network.targets().is_empty(),
                    })
                })
                .collect();
            Ok((rows, Value::Array(values)))
        }
        Category::Screens => {
            let layout = windows_displays::capture()?;
            let rows = layout
                .monitors
                .iter()
                .map(|m| row(&m.name, m.summary()))
                .collect();
            let mut value = serde_json::to_value(&layout).map_err(|e| e.to_string())?;
            if let Some(monitors) = value.get_mut("monitors").and_then(Value::as_array_mut) {
                for (m, v) in layout.monitors.iter().zip(monitors) {
                    *v = with_cli_name(v, &m.name)?;
                }
            }
            Ok((rows, value))
        }
        Category::AudioOutputs | Category::AudioInputs => {
            let mut backend = WindowsAudio::new()?;
            let devices = if category == Category::AudioInputs {
                backend.inputs()?
            } else {
                backend.outputs()?
            };
            let rows = devices
                .iter()
                .map(|d| Row {
                    absent: d.state == "Not present",
                    ..row(&d.name, device_summary(d))
                })
                .collect();
            let values = devices
                .iter()
                .map(|d| with_cli_name(d, &d.name))
                .collect::<Result<_, _>>()?;
            Ok((rows, Value::Array(values)))
        }
    }
}

/// Which kinds of steps a scene has, for example "screens · sound · FPS · 2 apps".
fn scene_summary(s: &scenes::Scene) -> String {
    let mut parts = Vec::new();
    if s.tv.is_some() {
        parts.push("TV".to_string());
    }
    if s.display.is_some() {
        parts.push("screens".into());
    }
    if s.audio.is_some() || s.tv_audio.is_some() {
        parts.push("sound".into());
    }
    if s.fps.is_some() {
        parts.push("FPS".into());
    }
    let actions = s.before.len() + s.actions.len() + s.leave.len();
    match actions {
        0 => {}
        1 => parts.push("1 action".into()),
        n => parts.push(format!("{n} actions")),
    }
    parts.join(" · ")
}

fn tv_summary(t: &tvs::Tv) -> String {
    format!(
        "{} · {} · {}",
        t.address,
        if t.credential.is_some() {
            "paired"
        } else {
            "not paired"
        },
        if t.network.targets().is_empty() {
            "wake not set up"
        } else {
            "wake available"
        }
    )
}

fn device_summary(d: &Output) -> String {
    let role = if d.console && d.multimedia {
        " · default"
    } else if d.console {
        " · console default"
    } else if d.multimedia {
        " · multimedia default"
    } else {
        ""
    };
    format!("{}{role} · {}", d.state, d.id)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rows_line_up_cli_names() {
        let rows = [
            row("Desk View", "2 screens".into()),
            row("TV", String::new()),
            row("★", "symbols only".into()),
        ];
        assert_eq!(
            format_rows(&rows.iter().collect::<Vec<_>>()),
            [
                "  desk-view  Desk View — 2 screens",
                "  tv         TV",
                "  -          ★ — symbols only",
            ]
        );
    }
    #[test]
    fn json_items_start_with_their_cli_name() {
        let v = with_cli_name(&json!({ "id": "1", "name": "Desk View" }), "Desk View").unwrap();
        assert_eq!(v["cli_name"], "desk-view");
        assert_eq!(v["id"], "1");
        assert_eq!(v["name"], "Desk View");
    }
}
