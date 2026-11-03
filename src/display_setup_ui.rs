//! Inline discovery progress. Native work and recovery stay on workers.
use crate::{DirectorApp, Message, display_ui::DisplayMessage};
use director::{
    display_detection::{self, Report, Stage},
    windows_audio::OperationLock,
    windows_display_apply::Snapshot,
};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use windows_reactor::*;
#[derive(Default)]
pub(crate) struct SetupState {
    pub focus: ElementRef<Button>,
    pub review: Option<usize>,
    pub progress: Option<Arc<std::sync::Mutex<std::sync::mpsc::Receiver<Stage>>>>,
    pub index: Option<usize>,
    pub token: u64,
    pub stage: Option<Stage>,
    pub cancel: Option<Arc<AtomicBool>>,
    pub note: String,
    pub details: String,
    pub recovery: Option<(director::displays::Layout, Snapshot)>,
}
#[derive(Clone)]
pub(crate) enum SetupMessage {
    Focus,
    Review(usize),
    Dismiss,
    Start(usize, bool),
    Cancel,
    Progress(u64, Option<Stage>),
    Finished(u64, Box<Report<Snapshot>>),
    RetryRestore,
}
fn finished(token: u64, report: Report<Snapshot>) -> SetupMessage {
    SetupMessage::Finished(token, Box::new(report))
}
fn msg(value: SetupMessage) -> Message {
    Message::Display(DisplayMessage::Setup(value))
}
impl DirectorApp {
    fn display_setup_tick(&self, token: u64, cx: &ComponentContext<Self>) {
        if let Some(receiver) = self.display.setup.progress.clone() {
            let _ = cx.spawn_background_with_rejection(
                move |_| {
                    msg(SetupMessage::Progress(
                        token,
                        receiver.lock().ok().and_then(|r| r.recv().ok()),
                    ))
                },
                msg(SetupMessage::Progress(token, None)),
            );
        }
    }
    pub(crate) fn update_display_setup(
        &mut self,
        message: SetupMessage,
        cx: &ComponentContext<Self>,
    ) {
        match message {
            SetupMessage::Focus => {
                let _ = self.display.setup.focus.request_focus();
            }
            SetupMessage::Review(index) if !self.busy && self.display.setup.recovery.is_none() => {
                self.display.setup.review = Some(index);
                self.display.setup.index = Some(index);
                self.display.setup.note.clear();
                self.display.setup.details.clear();
            }
            SetupMessage::Dismiss if !self.busy && self.display.setup.recovery.is_none() => {
                self.display.setup.review = None;
                self.display.setup.index = None;
            }
            SetupMessage::Cancel => {
                if let Some(cancel) = &self.display.setup.cancel {
                    cancel.store(true, Ordering::Release);
                }
            }
            SetupMessage::Start(index, wake)
                if !self.busy && self.display.setup.recovery.is_none() =>
            {
                let Some(path) = self
                    .display
                    .draft
                    .as_ref()
                    .and_then(|d| d.layout.monitors.get(index))
                    .and_then(|m| m.device_path.clone())
                else {
                    return;
                };
                let tv = if wake {
                    let Some(tv) = self
                        .tip
                        .tvs
                        .iter()
                        .find(|t| {
                            t.display
                                .as_ref()
                                .is_some_and(|link| link.device_path.eq_ignore_ascii_case(&path))
                        })
                        .cloned()
                    else {
                        return;
                    };
                    Some(tv)
                } else {
                    None
                };
                self.display.setup.token = self.display.setup.token.wrapping_add(1);
                let token = self.display.setup.token;
                let cancel = Arc::new(AtomicBool::new(false));
                self.display.setup.index = Some(index);
                self.display.setup.review = None;
                self.display.setup.cancel = Some(cancel.clone());
                self.display.setup.stage = Some(Stage::Preparing);
                self.display.setup.note.clear();
                self.display.setup.details.clear();
                self.busy = true;
                self.error = false;
                self.status =
                    "Temporary display setup is running. Your previous layout will be restored."
                        .into();
                self.details.clear();
                crate::close_guard::set_busy(true);
                let (sender, receiver) = std::sync::mpsc::channel();
                self.display.setup.progress = Some(Arc::new(std::sync::Mutex::new(receiver)));
                // Delivery failure requests cancellation; the engine still restores before returning.
                let _=cx.spawn_background_with_rejection(move |_| {
                    let result=(||{
                        #[cfg(test)]
                        if DETECTION_PREVIEW.load(Ordering::Acquire){return Ok(preview_run(&sender,&cancel,wake));}
                        let _lock=OperationLock::acquire()?;
                        if let Some(tv)=&tv {
                            let saved=director::tvs::load(&director::tvs::path()?)?;
                            if saved.iter().find(|t|t.id==tv.id)!=Some(tv) {return Err("The linked TV changed. Reopen this preset before detecting.".into());}
                        }
                        let mut backend=director::windows_display_detection::WindowsDetection::new(tv);
                        let mut report=display_detection::detect(&mut backend,&path,&cancel,|stage|{
                            director::diagnostics::event("display.detect.stage","progress",stage.label());
                            if sender.send(stage).is_err(){cancel.store(true,Ordering::Release);}
                        });
                        if let Some(learned)=&report.learned && let Err(e)=director::display_capabilities::path().and_then(|p|director::display_capabilities::remember(&p,learned)){
                            report.details.push(format!("Options are available in this window but could not be remembered: {e}"));
                        }
                        Ok::<_,String>(report)
                    })();
                    let report=result.unwrap_or_else(|e|Report {learned:None,message:"Detection could not start. Your screens are unchanged.".into(),details:vec![e],recovery:None});
                    msg(finished(token,report))
                },msg(finished(token,Report {learned:None,message:"The detection worker could not finish. Check your displays.".into(),details:vec![],recovery:None})));
                self.display_setup_tick(token, cx);
            }
            SetupMessage::Progress(token, Some(stage))
                if token == self.display.setup.token && self.display.setup.stage.is_some() =>
            {
                self.display.setup.stage = Some(stage);
                self.display_setup_tick(token, cx);
            }
            SetupMessage::Finished(token, report) if token == self.display.setup.token => {
                let mut report = *report;
                if self
                    .display
                    .setup
                    .cancel
                    .as_ref()
                    .is_some_and(|c| c.load(Ordering::Acquire))
                    && report.recovery.is_none()
                {
                    report.learned = None;
                    report.message = "Detection canceled. Your previous layout is restored.".into();
                }
                self.end();
                self.display.setup.cancel = None;
                self.display.setup.progress = None;
                self.display.setup.stage = None;
                self.display.setup.note = report.message;
                self.display.setup.details = report.details.join("\n");
                self.display.setup.recovery = report.recovery;
                self.error = self.display.setup.recovery.is_some();
                self.status = if self.error {
                    "Display setup needs attention. Use Restore previous layout."
                } else {
                    "Display detection finished. See the display card for details."
                }
                .into();
                self.details.clear();
                if let Some(learned) = report.learned
                    && let (Some(draft), Some(editor)) =
                        (&mut self.display.draft, &mut self.display.editor)
                {
                    match director::display_capabilities::apply_to_editor(&mut draft.layout,editor,&learned) {
                        Ok(())=>self.display.setup.note="Options detected · original layout restored. Choose your preset settings below.".into(),
                        Err(e)=>self.display.setup.details.push_str(&format!("\n{e}")),
                    }
                }
            }
            SetupMessage::RetryRestore if !self.busy => {
                let Some((before, snapshot)) = self.display.setup.recovery.clone() else {
                    return;
                };
                self.busy = true;
                self.error = false;
                self.status =
                    "Temporary display setup is running. Your previous layout will be restored."
                        .into();
                self.details.clear();
                crate::close_guard::set_busy(true);
                self.display.setup.stage = Some(Stage::Restoring);
                let token = self.display.setup.token;
                let (sender, receiver) = std::sync::mpsc::channel();
                self.display.setup.progress = Some(Arc::new(std::sync::Mutex::new(receiver)));
                let _ = cx.spawn_background_with_rejection(
                    move |_| {
                        let report = match OperationLock::acquire() {
                            Ok(_lock) => display_detection::restore(
                                &mut director::windows_display_detection::WindowsDetection::default(
                                ),
                                before,
                                snapshot,
                                |stage| {
                                    let _ = sender.send(stage);
                                },
                            ),
                            Err(e) => Report {
                                learned: None,
                                message: "Restoration could not start. Try again.".into(),
                                details: vec![e],
                                recovery: Some((before, snapshot)),
                            },
                        };
                        msg(finished(token, report))
                    },
                    msg(finished(
                        token,
                        Report {
                            learned: None,
                            message: "Restoration could not finish. Check your displays.".into(),
                            details: vec![],
                            recovery: self.display.setup.recovery.clone(),
                        },
                    )),
                );
                self.display_setup_tick(token, cx);
            }
            _ => {}
        }
    }
    pub(crate) fn display_setup_panel(&self, index: usize, cx: &ViewContext<Self>) -> View {
        let Some(m) = self
            .display
            .draft
            .as_ref()
            .and_then(|d| d.layout.monitors.get(index))
        else {
            return View::empty();
        };
        if m.device_path.is_none() {
            return View::empty();
        }
        let state = &self.display.setup;
        let selected = state.index == Some(index);
        let focus = if selected {
            state.focus.clone()
        } else {
            ElementRef::<Button>::new()
        };
        let configured = !self.display.editor.as_ref().is_none_or(|e| {
            e.options[index].modes.is_empty()
                || e.options[index]
                    .warning
                    .as_deref()
                    .is_some_and(|w| !w.starts_with("Previously detected"))
        });
        let active = selected && state.stage.is_some();
        let review = state.review == Some(index);
        let linked = self.tip.tvs.iter().find(|tv| {
            tv.display.as_ref().is_some_and(|link| {
                m.device_path
                    .as_ref()
                    .is_some_and(|p| link.device_path.eq_ignore_ascii_case(p))
            })
        });
        let wake = linked.is_some_and(|tv| {
            tv.credential.is_some() && !tv.network.targets().is_empty() && tv.pc_input.is_some()
        });
        let setup = |value| msg(value);
        if active {
            let canceling = state
                .cancel
                .as_ref()
                .is_some_and(|c| c.load(Ordering::Acquire));
            let status =
                if canceling && !matches!(state.stage, Some(Stage::Restoring | Stage::Verifying)) {
                    "Cancel requested · finishing this step, then restoring…"
                } else {
                    state.stage.unwrap().label()
                };
            return StackPanel::new().spacing(8.0).children((
                TextBlock::new()
                    .automation_name(format!("Display detection: {status}"))
                    .text(status)
                    .text_wrapping(TextWrapping::Wrap)
                    .font_weight(FontWeight::SEMI_BOLD),
                ProgressBar::new().is_indeterminate(true),
                Button::new()
                    .element_ref(&focus)
                    .automation_name("Cancel display detection")
                    .is_enabled(
                        !canceling
                            && state.cancel.is_some()
                            && !matches!(state.stage, Some(Stage::Restoring | Stage::Verifying)),
                    )
                    .on_click(cx.message(setup(SetupMessage::Cancel)))
                    .content("Cancel detection"),
            ));
        }
        if selected && state.recovery.is_some() {
            return StackPanel::new().spacing(8.0).children((
                TextBlock::new()
                    .text(&state.note)
                    .text_wrapping(TextWrapping::Wrap),
                TextBlock::new()
                    .text(&state.details)
                    .font_size(12.0)
                    .opacity(0.72)
                    .text_wrapping(TextWrapping::Wrap),
                Button::new()
                    .element_ref(&focus)
                    .automation_name("Restore previous display layout")
                    .is_enabled(!self.busy)
                    .on_click(cx.message(setup(SetupMessage::RetryRestore)))
                    .content("Restore previous layout"),
            ));
        }
        if review {
            return Border::new().padding(Thickness::uniform(12.0)).background(Color::argb(12,128,128,128)).corner_radius(CornerRadius::uniform(6.0)).content(
                StackPanel::new().spacing(10.0).children((
                    TextBlock::new().text("Detect this display’s options").font_weight(FontWeight::SEMI_BOLD),
                    TextBlock::new().text("Director will temporarily enable this display alongside your current screens, read its options, then restore your layout. Screens may briefly flicker. Turn the display on and select its PC input first.").text_wrapping(TextWrapping::Wrap).opacity(0.72),
                    if wake{TextBlock::new().text("Wake and detect also wakes the linked TV and selects its saved PC input. The TV stays on afterward.").text_wrapping(TextWrapping::Wrap).font_size(12.0).opacity(0.72).into()}else{View::empty()},
                    StackPanel::new().orientation(Orientation::Horizontal).spacing(8.0).children((
                        Button::new().element_ref(&focus).automation_name(format!("Detect display options for {}",m.name)).is_enabled(!self.busy).on_click(cx.message(setup(SetupMessage::Start(index,false)))).content("Detect options"),
                        if wake{Button::new().automation_name(format!("Wake and detect {}",m.name)).is_enabled(!self.busy).on_click(cx.message(setup(SetupMessage::Start(index,true)))).content("Wake and detect")}else{View::empty()},
                        Button::new().style(ButtonStyle::Subtle).automation_name("Cancel display setup").is_enabled(!self.busy).on_click(cx.message(setup(SetupMessage::Dismiss))).content("Not now"),
                    )),
                )));
        }
        StackPanel::new().spacing(6.0).children((
            if selected && !state.note.is_empty() && !state.note.starts_with("Options detected") {
                TextBlock::new()
                    .text(&state.note)
                    .text_wrapping(TextWrapping::Wrap)
                    .opacity(0.72)
                    .into()
            } else {
                View::empty()
            },
            if selected && !state.details.is_empty() {
                TextBlock::new()
                    .text(&state.details)
                    .text_wrapping(TextWrapping::Wrap)
                    .font_size(12.0)
                    .opacity(0.72)
                    .into()
            } else {
                View::empty()
            },
            Button::new()
                .element_ref(&focus)
                .automation_name(format!("Set up display {}", m.name))
                .style(if configured {
                    ButtonStyle::Subtle
                } else {
                    ButtonStyle::Default
                })
                .is_enabled(!self.busy && state.recovery.is_none())
                .on_click(cx.message(setup(SetupMessage::Review(index))))
                .content(if configured {
                    "Detect again"
                } else {
                    "Set up this display"
                }),
        ))
    }
}

#[cfg(test)]
pub(crate) static DETECTION_PREVIEW: AtomicBool = AtomicBool::new(false);
#[cfg(test)]
pub(crate) fn preview_editor() -> (
    director::displays::Layout,
    Vec<director::display_editor::Options>,
) {
    use director::{
        display_editor::{Mode, Options},
        displays::{DesktopMode, Layout, Monitor, RefreshRate},
    };
    let rate = RefreshRate {
        numerator: 60,
        denominator: 1,
    };
    let primary = DesktopMode {
        source_adapter_luid: 1,
        source_id: 0,
        source_name: "fixture".into(),
        x: 0,
        y: 0,
        width: 1920,
        height: 1080,
        source_width: 1920,
        source_height: 1080,
        refresh_rate: rate.clone(),
        rotation_degrees: 0,
    };
    (
        Layout {
            monitors: vec![
                Monitor {
                    adapter_luid: 1,
                    target_id: 0,
                    device_path: Some("fixture-desk".into()),
                    name: "Working display (test fixture)".into(),
                    connected: true,
                    hdr: None,
                    enabled: true,
                    desktop: Some(primary),
                },
                Monitor {
                    adapter_luid: 1,
                    target_id: 1,
                    device_path: Some("fixture-tv".into()),
                    name: "LG C1 (test fixture)".into(),
                    connected: true,
                    hdr: None,
                    enabled: false,
                    desktop: None,
                },
            ],
        },
        vec![
            Options {
                modes: vec![Mode {
                    width: 1920,
                    height: 1080,
                    refresh: rate.clone(),
                }],
                warning: None,
                hdr: None,
            },
            Options {
                modes: vec![Mode {
                    width: 640,
                    height: 480,
                    refresh: rate,
                }],
                warning: Some("Set up this display to detect its available modes.".into()),
                hdr: None,
            },
        ],
    )
}
#[cfg(test)]
pub(crate) fn preview_tv() -> director::tvs::Tv {
    director::tvs::Tv {
        id: "fixture-tv".into(),
        name: "Linked TV (test fixture)".into(),
        address: "192.0.2.80".into(),
        brand: Default::default(),
        samsung: Default::default(),
        android: Default::default(),
        connection: Default::default(),
        display: Some(director::tvs::DisplayLink {
            device_path: "fixture-tv".into(),
            name: "LG C1 (test fixture)".into(),
        }),
        audio: None,
        credential: Some(vec![0]),
        inputs: vec![],
        pc_input: Some(director::lg_protocol::Input {
            id: "HDMI_1".into(),
            label: "PC".into(),
        }),
        network: director::tv_network::NetworkAddresses {
            wired: Some("00:11:22:33:44:55".parse().unwrap()),
            ..Default::default()
        },
    }
}
#[cfg(test)]
fn preview_run(
    sender: &std::sync::mpsc::Sender<Stage>,
    cancel: &AtomicBool,
    wake: bool,
) -> Report<Snapshot> {
    use std::time::Duration;
    let mut canceled = false;
    for stage in [
        Stage::Preparing,
        if wake { Stage::Waking } else { Stage::Enabling },
        Stage::Reading,
    ] {
        let _ = sender.send(stage);
        for _ in 0..if stage == Stage::Reading { 60 } else { 24 } {
            std::thread::sleep(Duration::from_millis(50));
            if cancel.load(Ordering::Acquire) {
                canceled = true;
                break;
            }
        }
        if canceled {
            break;
        }
    }
    for stage in [Stage::Restoring, Stage::Verifying] {
        let _ = sender.send(stage);
        std::thread::sleep(Duration::from_millis(700));
    }
    let rate = director::displays::RefreshRate {
        numerator: 120,
        denominator: 1,
    };
    let learned = director::display_detection::Learned {
        device_path: "fixture-tv".into(),
        name: "LG C1 (test fixture)".into(),
        options: director::display_editor::Options {
            modes: vec![
                director::display_editor::Mode {
                    width: 3840,
                    height: 2160,
                    refresh: rate.clone(),
                },
                director::display_editor::Mode {
                    width: 3840,
                    height: 2160,
                    refresh: director::displays::RefreshRate {
                        numerator: 60,
                        denominator: 1,
                    },
                },
            ],
            warning: None,
            hdr: Some(false),
        },
        desktop: director::displays::DesktopMode {
            source_adapter_luid: 1,
            source_id: 1,
            source_name: "fixture".into(),
            x: 1920,
            y: 0,
            width: 3840,
            height: 2160,
            source_width: 3840,
            source_height: 2160,
            refresh_rate: rate,
            rotation_degrees: 0,
        },
    };
    Report {
        learned: (!canceled).then_some(learned),
        message: if canceled {
            "Detection canceled. Your previous layout is restored."
        } else {
            "Display options detected. Your previous layout is restored."
        }
        .into(),
        details: vec![],
        recovery: None,
    }
}
#[cfg(test)]
mod native_preview {
    use super::*;
    #[test]
    #[ignore = "In-memory display discovery workflow; never changes Windows or contacts a TV"]
    fn detection_fixture() {
        director::diagnostics::init();
        DETECTION_PREVIEW.store(true, Ordering::Release);
        App::run_component::<DirectorApp>(()).unwrap();
        director::diagnostics::flush();
    }
}
