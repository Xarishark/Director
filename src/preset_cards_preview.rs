//! Native QA fixture with no device setters or persistence.
use crate::{DirectorApp, Message, Page, display_ui::DisplayMessage};
use director::{
    audio::Output,
    display::display_editor::{Change, Editor, Mode, Options},
    display::displays::{self, DesktopMode, Layout, Monitor, RefreshRate},
};
use std::sync::atomic::{AtomicBool, Ordering};
pub(crate) static ENABLED: AtomicBool = AtomicBool::new(false);

/// Where preview fixtures write state for inspection: `target/preview/<name>`.
pub(crate) fn output(name: &str) -> std::path::PathBuf {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("target/preview");
    std::fs::create_dir_all(&dir).unwrap();
    dir.join(name)
}
fn layout() -> Layout {
    let mut monitors = vec![];
    for (i, name) in ["Desk monitor", "Portrait monitor", "Living room TV"]
        .iter()
        .enumerate()
    {
        monitors.push(Monitor {
            adapter_luid: 1,
            target_id: i as u32,
            device_path: Some(format!("preview-{i}")),
            name: (*name).into(),
            connected: true,
            enabled: i < 2,
            hdr: Some(i == 0),
            desktop: if i < 2 {
                Some(DesktopMode {
                    source_adapter_luid: 1,
                    source_id: i as u32,
                    source_name: format!("DISPLAY{i}"),
                    x: if i == 0 { 0 } else { 1920 },
                    y: 0,
                    width: if i == 0 { 1920 } else { 1080 },
                    height: if i == 0 { 1080 } else { 1920 },
                    source_width: 1920,
                    source_height: 1080,
                    refresh_rate: RefreshRate {
                        numerator: if i == 0 { 120 } else { 60 },
                        denominator: 1,
                    },
                    rotation_degrees: if i == 0 { 0 } else { 90 },
                })
            } else {
                None
            },
        });
    }
    Layout { monitors }
}
fn options() -> Vec<Options> {
    (0..3)
        .map(|i| Options {
            warning: None,
            hdr: if i == 1 { None } else { Some(false) },
            modes: vec![
                Mode {
                    width: 1920,
                    height: 1080,
                    refresh: RefreshRate {
                        numerator: if i == 0 { 120 } else { 60 },
                        denominator: 1,
                    },
                },
                Mode {
                    width: 1280,
                    height: 720,
                    refresh: RefreshRate {
                        numerator: 60,
                        denominator: 1,
                    },
                },
            ],
        })
        .collect()
}
fn displays() -> Vec<displays::Preset> {
    let desk = layout();
    let mut mirror = desk.clone();
    let mut editor = Editor::new(&mirror, options());
    editor
        .change(&mut mirror, 1, Change::Mirror(Some(0)))
        .unwrap();
    let mut tv = desk.clone();
    tv.monitors[2].enabled = true;
    let mut d = desk.monitors[0].desktop.clone().unwrap();
    d.source_id = 2;
    d.width = 3840;
    d.height = 2160;
    d.source_width = 3840;
    d.source_height = 2160;
    tv.monitors[2].desktop = Some(d);
    tv.monitors[2].hdr = Some(true);
    for m in &mut tv.monitors[..2] {
        m.enabled = false;
        m.desktop = None;
    }
    [
        ("Desk working", desk),
        ("TV gaming", tv),
        ("Mirrored desk", mirror),
    ]
    .into_iter()
    .enumerate()
    .map(|(i, (name, layout))| displays::Preset {
        id: format!("fixture-display-{i}"),
        name: name.into(),
        captured_at_unix_seconds: 0,
        confirm_changes: false,
        layout,
    })
    .collect()
}
impl DirectorApp {
    pub(crate) fn preview_cards_message(&mut self, message: &Message) -> bool {
        match message {
            // The first scene load seeds every page's preview data.
            Message::Scene(
                crate::scene_ui::SceneMessage::Load | crate::scene_ui::SceneMessage::Loaded(..),
            ) if !self.scene.ready => {
                self.end();
                self.display.store_ready = true;
                self.outputs = vec![Output {
                    communications: false,
                    id: "fixture-output".into(),
                    name: "Desk speakers".into(),
                    active: true,
                    state: "Active".into(),
                    console: true,
                    multimedia: true,
                    volume_percent: Some(45),
                    muted: None,
                }];
                self.outputs.push(Output {
                    id: "fixture-chat".into(),
                    name: "Chat headset".into(),
                    state: "Active".into(),
                    active: true,
                    console: false,
                    multimedia: false,
                    communications: true,
                    volume_percent: Some(35),
                    muted: None,
                });
                self.inputs = vec![Output {
                    id: "fixture-mic".into(),
                    name: "Desk microphone".into(),
                    state: "Active".into(),
                    active: true,
                    console: true,
                    multimedia: true,
                    communications: true,
                    volume_percent: Some(55),
                    muted: None,
                }];
                self.display.presets = displays();
                self.display.current = Some(layout());
                self.scene.displays = self.display.presets.clone();
                self.scene.tvs = vec![director::tv::tvs::Tv {
                    id: "fixture-tv".into(),
                    name: "Living room TV".into(),
                    address: "192.0.2.50".into(),
                    brand: Default::default(),
                    samsung: Default::default(),
                    android: Default::default(),
                    connection: Default::default(),
                    display: Some(director::tv::tvs::DisplayLink {
                        device_path: "preview-2".into(),
                        name: "Living room TV".into(),
                    }),
                    audio: Some(director::audio::DeviceSelection {
                        id: "fixture-tv-output".into(),
                        name: "TV HDMI audio".into(),
                    }),
                    credential: Some(vec![1]),
                    inputs: vec![director::tv::lg_protocol::Input {
                        id: "HDMI_1".into(),
                        label: "PC HDMI 1".into(),
                    }],
                    pc_input: Some(director::tv::lg_protocol::Input {
                        id: "HDMI_1".into(),
                        label: "PC HDMI 1".into(),
                    }),
                    network: Default::default(),
                }];
                let mut scene =
                    director::scene::scenes::Scene::new("fixture-scene", "TV gaming scene");
                scene.display = Some("fixture-display-1".into());
                scene.sound = Some(director::scene::scenes::Sound {
                    output: Some(director::scene::scenes::SoundOutput::Tv {
                        id: "fixture-tv".into(),
                    }),
                    volume: Some(35),
                    ..Default::default()
                });
                scene.tv = Some(director::scene::scenes::TvPreparation {
                    off_when_leaving: false,
                    id: "fixture-tv".into(),
                    wake: true,
                    input: Some("HDMI_1".into()),
                });
                scene.actions = vec![director::scene::scenes::Action::Uri {
                    target: "steam://open/bigpicture".into(),
                }];
                scene.leave = vec![director::scene::scenes::Action::Uri {
                    target: "steam://close/bigpicture".into(),
                }];
                let mut headphones =
                    director::scene::scenes::Scene::new("fixture-headphones", "Headphones");
                headphones.sound = Some(director::scene::scenes::Sound {
                    output: Some(director::scene::scenes::SoundOutput::Device {
                        id: "fixture-chat".into(),
                        name: "Chat headset".into(),
                    }),
                    volume: Some(30),
                    mic: Some(director::scene::scenes::Mic {
                        device: director::audio::DeviceSelection {
                            id: "fixture-mic".into(),
                            name: "Desk microphone".into(),
                        },
                        volume: Some(80),
                        mute: None,
                    }),
                    ..Default::default()
                });
                self.scene.scenes = vec![headphones, scene];
                self.scene.ready = true;
                self.fps = crate::fps_ui::FpsState::fixture();
                self.tv.tvs = self.scene.tvs.clone();
                self.tv.ready_for_controls_preview();
                if crate::samsung_ui::PREVIEW.load(Ordering::Acquire) {
                    self.page = Page::Tvs;
                    let mut tv = self.tv.tvs[0].clone();
                    tv.id = "samsung-setup-fixture".into();
                    tv.name = String::new();
                    tv.address = String::new();
                    tv.credential = None;
                    tv.inputs.clear();
                    tv.pc_input = None;
                    tv.audio = None;
                    tv.display = None;
                    tv.samsung = Default::default();
                    self.tv.draft = Some(tv);
                    self.tv.setup = Some(director::tv::tv_setup::Setup::default());
                    self.tv.test_port = 2;
                }
                self.status = "Ready.".into();
                true
            }
            Message::Display(DisplayMessage::Refresh | DisplayMessage::Loaded(..)) => {
                self.end();
                self.display.store_ready = true;
                self.status = "Ready.".into();
                true
            }
            Message::Display(DisplayMessage::Edit(id)) => {
                self.display.draft = self.display.presets.iter().find(|p| &p.id == id).cloned();
                self.display.editor = self
                    .display
                    .draft
                    .as_ref()
                    .map(|d| Editor::new(&d.layout, options()));
                self.display.setup = Default::default();
                self.status = "Draft preview. Your screens are unchanged.".into();
                true
            }
            Message::Display(
                DisplayMessage::Apply(_)
                | DisplayMessage::Save
                | DisplayMessage::DeleteClosed(_)
                | DisplayMessage::Create
                | DisplayMessage::Capture
                | DisplayMessage::Setup(_),
            ) => {
                self.status = "Preview only. No devices or settings changed.".into();
                true
            }
            Message::Scene(
                crate::scene_ui::SceneMessage::Load | crate::scene_ui::SceneMessage::Loaded(..),
            ) => {
                self.end();
                self.scene.displays = self.display.presets.clone();
                self.scene.tvs = self.tv.tvs.clone();
                self.scene.ready = true;
                self.status = "Ready.".into();
                true
            }
            Message::Scene(crate::scene_ui::SceneMessage::Apply(_)) => {
                self.status = "Preview only. No devices or settings changed.".into();
                true
            }
            Message::Tv(crate::tv_ui::TvMessage::Load | crate::tv_ui::TvMessage::Loaded(..)) => {
                self.end();
                self.status = "Ready.".into();
                true
            }
            Message::Tv(crate::tv_ui::TvMessage::SetupPair)
                if crate::samsung_ui::PREVIEW.load(Ordering::Acquire) =>
            {
                if let Some(tv) = &self.tv.draft
                    && tv.address == "192.0.2.98"
                {
                    let result: Result<(), String> =
                        Err("fixture timeout (0x80072EE2), secret=do-not-export 192.0.2.98".into());
                    director::tv::tv_report::record(
                        &tv.id,
                        director::tv::tv_report::Action::Pair,
                        std::time::Instant::now(),
                        &result,
                        None,
                    );
                    self.failure("Fixture pairing failed. No real network connection.".into());
                    return true;
                }
                let android = self
                    .tv
                    .draft
                    .as_ref()
                    .is_some_and(|t| t.brand == director::tv::tvs::Brand::TclAndroid);
                if android && !self.tv.android_fixture_waiting {
                    self.tv.android_fixture_waiting = true;
                    return true;
                }
                if android && director::tv::android_tv::code(&self.tv.android_code).is_err() {
                    return true;
                }
                self.tv.android_fixture_waiting = false;
                if let Some(tv) = &mut self.tv.draft {
                    tv.credential = Some(vec![1]);
                    tv.compatibility_mut().model = Some(
                        if android {
                            "TCL fixture"
                        } else {
                            "S90C fixture"
                        }
                        .into(),
                    );
                    tv.compatibility_mut().firmware = Some("1234".into());
                }
                if let Some(setup) = &mut self.tv.setup {
                    setup.inputs_loaded = true;
                    setup.input_chosen = true;
                    setup.step = director::tv::tv_setup::Step::Input;
                }
                self.status = "Fixture paired. No real network connection.".into();
                true
            }
            Message::Tv(crate::tv_ui::TvMessage::TestHdmi)
                if crate::samsung_ui::PREVIEW.load(Ordering::Acquire) =>
            {
                if self.tv.test_ready
                    && let Some(tv) = &mut self.tv.draft
                {
                    let index = tv
                        .compatibility_mut()
                        .record(self.tv.test_port.clamp(1, 4), true);
                    self.tv.test_confirmation = Some(index);
                    self.tv.test_ready = false;
                }
                self.status = "Fixture direct command sent. No real TV was contacted.".into();
                true
            }
            Message::Tv(crate::tv_ui::TvMessage::ExportReport)
                if crate::samsung_ui::PREVIEW.load(Ordering::Acquire) =>
            {
                if let Some(tv) = &self.tv.draft {
                    let data = director::tv::samsung::report(tv).unwrap();
                    std::fs::write(
                        output(if tv.brand == director::tv::tvs::Brand::TclAndroid {
                            "android-fixture-report.json"
                        } else {
                            "samsung-fixture-report.json"
                        }),
                        serde_json::to_vec_pretty(&data).unwrap(),
                    )
                    .unwrap();
                }
                self.status = "Fixture redacted report exported.".into();
                true
            }
            Message::Tv(crate::tv_ui::TvMessage::Edit(id)) => {
                self.tv.draft = self.tv.tvs.iter().find(|t| &t.id == id).cloned();
                true
            }
            Message::Tv(crate::tv_ui::TvMessage::Save) => {
                if let Some(d) = self.tv.draft.take() {
                    if let Some(t) = self.tv.tvs.iter_mut().find(|t| t.id == d.id) {
                        *t = d;
                    } else {
                        self.tv.tvs.push(d);
                    }
                }
                self.tv.setup = None;
                self.status = "TV link saved in preview memory only.".into();
                true
            }
            Message::Tv(crate::tv_ui::TvMessage::Control(
                crate::tv_controls_ui::ControlMessage::Open(_)
                | crate::tv_controls_ui::ControlMessage::Close
                | crate::tv_controls_ui::ControlMessage::Details(_),
            )) if crate::samsung_ui::PREVIEW.load(Ordering::Acquire) => false,
            Message::Tv(
                crate::tv_ui::TvMessage::ReadDetails
                | crate::tv_ui::TvMessage::SetupPair
                | crate::tv_ui::TvMessage::Control(_),
            ) => true,
            Message::Navigate(Some(page))
                if !["audio", "displays", "scenes", "tvs", "current"].contains(&page.as_str()) =>
            {
                true
            }
            _ => false,
        }
    }
}
#[test]
#[ignore = "Owned-window preset/mirror fixture; no device setters or settings writes"]
fn cards_fixture() {
    ENABLED.store(true, Ordering::Release);
    director::diagnostics::init();
    windows_reactor::App::run_component::<DirectorApp>(()).unwrap();
    director::diagnostics::flush();
}
