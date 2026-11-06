//! TV definitions, guided addition, and explicit device actions.
use crate::{DirectorApp, Message, work};
use director::{
    lg_protocol::Input,
    tv_network,
    tv_setup::{Setup, Step},
    tvs::{self, Connection, DisplayLink, Tv},
    windows_audio::OperationLock,
    windows_displays, windows_tv,
};
use windows_reactor::*;
#[derive(Clone)]
pub(crate) enum TvMessage {
    Load,
    Loaded(Result<Vec<Tv>, String>, Result<Vec<DisplayLink>, String>),
    Add,
    Edit(String),
    Cancel,
    Name(String),
    Brand(Option<usize>),
    TestPort(Option<usize>),
    TestHdmi,
    TestReady(bool),
    Tested(u8, Result<(), String>),
    ConfirmTest(bool),
    ExportReport,
    ReportModel(String),
    ReportSoftware(String),
    ReportNetwork(Option<usize>),
    ReportAnswer(u8, Option<usize>),
    ReportExported(Result<String, String>),
    Address(String),
    Legacy(bool),
    Link(Option<usize>),
    AudioLink(Option<usize>),
    AudioLinks(Result<Vec<director::audio::Output>, String>),
    Next,
    Back,
    SetupPair,
    AndroidCode(String),
    AndroidStarted(
        Result<
            std::sync::Arc<std::sync::Mutex<Option<director::windows_android_tv::Pairing>>>,
            String,
        >,
    ),
    SetupPaired(Result<Vec<u8>, String>),
    ReadDetails,
    Details(Result<windows_tv::TvDetails, String>),
    PcInput(Option<usize>),
    ManualMac(String),
    ManualMacEnabled(bool),
    Save,
    Saved(Result<Vec<Tv>, String>),

    Control(crate::tv_controls_ui::ControlMessage),
    Delete(String),
    DeleteClosed(ContentDialogResult),
}
#[derive(Default)]
pub(crate) struct TvState {
    pub(crate) tvs: Vec<Tv>,
    pub(crate) add_after_load: bool,
    pub(crate) controls: Option<crate::tv_controls_ui::Controls>,
    pub(crate) control_revision: u64,
    displays: Vec<DisplayLink>,
    pub draft: Option<Tv>,
    pub(crate) setup: Option<Setup>,
    manual_mac: String,
    manual_mac_enabled: bool,
    ready: bool,
    pub(crate) test_port: u8,
    pub(crate) test_ready: bool,
    pub(crate) test_confirmation: Option<usize>,
    pending_delete: Option<String>,
    pub(crate) android_pairing:
        Option<std::sync::Arc<std::sync::Mutex<Option<director::windows_android_tv::Pairing>>>>,
    pub(crate) android_code: String,
    #[cfg(test)]
    pub(crate) android_fixture_waiting: bool,
}
fn msg(m: TvMessage) -> Message {
    Message::Tv(m)
}
fn input_label(input: &Input) -> String {
    if input.label == input.id {
        input.label.clone()
    } else {
        format!("{} ({})", input.label, input.id)
    }
}
use crate::ui::note;
impl TvState {
    #[cfg(test)]
    pub(crate) fn ready_for_controls_preview(&mut self) {
        self.ready = true;
    }
    pub(crate) fn android_waiting(&self) -> bool {
        #[cfg(test)]
        if self.android_fixture_waiting {
            return true;
        }
        self.android_pairing.is_some()
    }
    fn links(&self) -> Vec<DisplayLink> {
        let mut links = self.displays.clone();
        if let Some(link) = self.draft.as_ref().and_then(|d| d.display.as_ref())
            && !links
                .iter()
                .any(|l| l.device_path.eq_ignore_ascii_case(&link.device_path))
        {
            links.push(link.clone());
        }
        links
    }
    fn inputs(tv: &Tv) -> Vec<Input> {
        let mut inputs = tv.control_inputs();
        if let Some(pc) = &tv.pc_input
            && tv.brand == tvs::Brand::Lg
            && !inputs.iter().any(|i| i.id == pc.id)
        {
            inputs.push(pc.clone());
        }
        inputs
    }
}
impl DirectorApp {
    pub(crate) fn tv_ready(&self) -> bool {
        !self.busy && self.tv.ready && self.guard_ready
    }
    pub(crate) fn update_tv(&mut self, message: TvMessage, cx: &ComponentContext<Self>) {
        #[cfg(test)]
        if crate::preset_cards_preview::ENABLED.load(std::sync::atomic::Ordering::Acquire)
            && matches!(message, TvMessage::Load)
        {
            self.end();
            self.status = "Ready.".into();
            return;
        }
        let ready = self.tv_ready();
        if matches!(
            &message,
            TvMessage::Cancel
                | TvMessage::Back
                | TvMessage::Address(_)
                | TvMessage::Brand(_)
                | TvMessage::Add
                | TvMessage::Edit(_)
        ) {
            self.tv.android_pairing = None;
            #[cfg(test)]
            {
                self.tv.android_fixture_waiting = false;
            }
            use zeroize::Zeroize;
            self.tv.android_code.zeroize();
        }
        match message {
            TvMessage::Brand(Some(index)) if !self.busy => {
                if let Some(tv) = &mut self.tv.draft {
                    let brand = if index == 2 {
                        tvs::Brand::TclAndroid
                    } else if index == 1 {
                        tvs::Brand::Samsung
                    } else {
                        tvs::Brand::Lg
                    };
                    if tv.brand != brand {
                        if let Some(setup) = &mut self.tv.setup {
                            setup.invalidate_connection(tv);
                        }
                        tv.brand = brand;
                        if brand == tvs::Brand::TclAndroid {
                            tv.connection = Connection::Encrypted;
                        }
                        tv.samsung = Default::default();
                        tv.android = Default::default();
                        self.tv.test_confirmation = None;
                    }
                }
            }
            TvMessage::TestReady(v) if !self.busy => {
                self.tv.test_ready = v;
            }
            TvMessage::TestPort(Some(index)) if !self.busy => {
                self.tv.test_port = (index as u8 + 1).clamp(1, 4);
                self.tv.test_ready = false;
            }
            TvMessage::TestHdmi if ready && self.tv.test_ready => {
                if let Some(tv) = self.tv.draft.clone()
                    && tv.brand != tvs::Brand::Lg
                    && tv.credential.is_some()
                {
                    if self
                        .tv
                        .tvs
                        .iter()
                        .find(|old| old.id == tv.id)
                        .is_some_and(|old| !old.same_endpoint(&tv))
                    {
                        self.failure(
                            "Continue setup to pair the changed connection before testing.".into(),
                        );
                        return;
                    }
                    let port = self.tv.test_port.clamp(1, 4);
                    self.tv.test_confirmation = None;
                    self.begin(format!("Sending one direct HDMI {port} command…"));
                    work(cx, move || {
                        msg(TvMessage::Tested(
                            port,
                            (|| {
                                let _lock = OperationLock::acquire()?;
                                if tv.brand == tvs::Brand::TclAndroid {
                                    director::windows_android_tv::test_hdmi(&tv, port)
                                } else {
                                    director::windows_samsung::test_hdmi(&tv, port)
                                }
                            })(),
                        ))
                    });
                }
            }
            TvMessage::Tested(port, result) => {
                self.end();
                self.tv.test_ready = false;
                if let Some(tv) = &mut self.tv.draft
                    && tv.brand != tvs::Brand::Lg
                {
                    let index = tv.compatibility_mut().record(port, result.is_ok());
                    self.tv.test_confirmation = result.is_ok().then_some(index);
                    tv.inputs = tv.compatibility().inputs();
                    if tv
                        .pc_input
                        .as_ref()
                        .is_some_and(|pc| !tv.inputs.iter().any(|i| i.id == pc.id))
                    {
                        tv.pc_input = None;
                    }
                    if let Err(e) = result {
                        self.error = true;
                        self.status = "Direct HDMI test could not be sent.".into();
                        self.details = e;
                    } else {
                        self.error = false;
                        self.status =
                            "Command sent. Please confirm what happened on the TV.".into();
                        self.details.clear();
                        self.announce(cx);
                    }
                }
            }
            TvMessage::ConfirmTest(yes) if !self.busy => {
                if let Some(index) = self.tv.test_confirmation.take()
                    && let Some(tv) = &mut self.tv.draft
                    && let Some(test) = tv.compatibility_mut().tests.get_mut(index)
                {
                    if test.sent {
                        test.confirmed = Some(yes);
                    }
                    tv.inputs = tv.compatibility().inputs();
                    if tv
                        .pc_input
                        .as_ref()
                        .is_some_and(|pc| !tv.inputs.iter().any(|i| i.id == pc.id))
                    {
                        tv.pc_input = None;
                    }
                    self.error = false;
                    self.status = if yes {
                        "Direct HDMI support confirmed. Save the TV to remember it."
                    } else {
                        "Recorded. This input control will remain hidden."
                    }
                    .into();
                    self.details.clear();
                    self.announce(cx);
                }
            }
            TvMessage::ReportModel(v) if !self.busy => {
                if let Some(tv) = &mut self.tv.draft {
                    tv.compatibility_mut().feedback.tv_model = v.chars().take(160).collect();
                }
            }
            TvMessage::ReportSoftware(v) if !self.busy => {
                if let Some(tv) = &mut self.tv.draft {
                    tv.compatibility_mut().feedback.tv_software = v.chars().take(160).collect();
                }
            }
            TvMessage::ReportNetwork(Some(i)) if !self.busy => {
                if let Some(tv) = &mut self.tv.draft {
                    tv.compatibility_mut().feedback.network =
                        director::tv_report::Network::from_index(i);
                }
            }
            TvMessage::ReportAnswer(which, Some(i)) if !self.busy => {
                if let Some(tv) = &mut self.tv.draft {
                    let f = &mut tv.compatibility_mut().feedback;
                    let a = director::tv_report::Answer::from_index(i);
                    match which {
                        0 => f.power_status_correct = a,
                        1 => f.turn_off_worked = a,
                        _ => f.wake_worked = a,
                    }
                }
            }
            TvMessage::ExportReport if !self.busy => {
                if let Some(tv) = self.tv.draft.clone()
                    && tv.brand != tvs::Brand::Lg
                {
                    self.begin("Saving a redacted TV compatibility report…");
                    work(cx, move || {
                        msg(TvMessage::ReportExported((|| {
                            let path = director::samsung::export(&tv)?;
                            let folder = path.parent().ok_or("Report folder was unavailable.")?;
                            let opened = crate::options_ui::open_folder(folder);
                            Ok(format!(
                                "Report: {}{}",
                                path.display(),
                                opened.err().map_or(String::new(), |e| format!(
                                    "\nCould not open its folder: {e}"
                                ))
                            ))
                        })()))
                    });
                }
            }
            TvMessage::ReportExported(result) => {
                self.end();
                match result {
                    Ok(path) => {
                        self.error = false;
                        self.status = "Redacted compatibility report saved.".into();
                        self.announce(cx);
                        self.details = path;
                    }
                    Err(e) => self.failure(e),
                }
            }
            TvMessage::Control(message) => self.update_tv_controls(message, cx),
            TvMessage::AudioLinks(Ok(outputs)) => self.outputs = outputs,
            TvMessage::AudioLink(index) if !self.busy && index.is_some() => {
                let links = self.tv_audio_links();
                let link = index
                    .and_then(|i| i.checked_sub(1))
                    .and_then(|i| links.get(i).cloned());
                if let Some(d) = &mut self.tv.draft {
                    d.audio = link;
                }
            }
            TvMessage::Load if !self.busy => {
                work(cx, || {
                    msg(TvMessage::AudioLinks(
                        director::windows_audio::WindowsAudio::new()
                            .and_then(|mut a| a.picker_devices(false)),
                    ))
                });
                self.begin("Reading saved TVs…");
                work(cx, || {
                    msg(TvMessage::Loaded(
                        tvs::path().and_then(|p| tvs::load(&p)),
                        windows_displays::capture().map(|l| {
                            l.monitors
                                .into_iter()
                                .filter_map(|m| {
                                    m.device_path.map(|device_path| DisplayLink {
                                        device_path,
                                        name: m.name,
                                    })
                                })
                                .collect()
                        }),
                    ))
                });
            }
            TvMessage::Loaded(tvs, displays) => {
                self.end();
                self.tv.ready = false;
                match tvs {
                    Ok(tvs) => {
                        self.tv.tvs = tvs;
                        self.tip.tvs = self.tv.tvs.clone();
                        self.tip.catalog_loaded = true;
                        self.tv.ready = true;
                        self.status = "Ready.".into();
                    }
                    Err(e) => {
                        self.tv.add_after_load = false;
                        self.failure(e);
                        return;
                    }
                }
                match displays {
                    Ok(d) => self.tv.displays = d,
                    Err(e) => {
                        self.tv.displays.clear();
                        self.warn(
                            "Windows display links could not be read.",
                            format!("TV setup is still available. {e}"),
                        );
                    }
                }
                if self.tv.add_after_load {
                    self.tv.add_after_load = false;
                    self.update_tv(TvMessage::Add, cx);
                }
            }
            TvMessage::Add if ready => {
                self.tv.draft = Some(Tv {
                    id: format!(
                        "tv-{}-{}",
                        std::process::id(),
                        std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_nanos()
                    ),
                    name: String::new(),
                    address: String::new(),
                    brand: Default::default(),
                    samsung: Default::default(),
                    android: Default::default(),
                    connection: Connection::Encrypted,
                    display: None,
                    audio: None,
                    credential: None,
                    inputs: vec![],
                    pc_input: None,
                    network: Default::default(),
                });
                self.tv.setup = Some(Setup::default());
                self.tv.test_port = 2;
                self.tv.test_ready = false;
                self.tv.test_confirmation = None;
                self.tv.manual_mac.clear();
                self.tv.manual_mac_enabled = false;
                self.error = false;
                self.status = "Choose your TV brand to begin.".into();
                self.details.clear();
            }
            TvMessage::Edit(id) if ready => {
                self.tv.draft = self.tv.tvs.iter().find(|t| t.id == id).cloned();
                self.tv.setup = None;
                self.tv.test_port = 2;
                self.tv.test_confirmation = None;
                self.tv.manual_mac_enabled = self
                    .tv
                    .draft
                    .as_ref()
                    .is_some_and(|tv| tv.network.manual.is_some());
                self.tv.manual_mac = self
                    .tv
                    .draft
                    .as_ref()
                    .and_then(|t| t.network.manual)
                    .map_or(String::new(), |m| m.to_string());
                self.error = false;
                self.status = "Edit TV.".into();
                self.details.clear();
                if self
                    .tv
                    .draft
                    .as_ref()
                    .is_some_and(|tv| tv.credential.is_some())
                {
                    self.update_tv(TvMessage::ReadDetails, cx);
                }
            }
            TvMessage::Cancel if !self.busy => {
                let paired = self.tv.setup.is_some()
                    && self
                        .tv
                        .draft
                        .as_ref()
                        .is_some_and(|t| t.credential.is_some());
                self.tv.draft = None;
                self.tv.setup = None;
                self.error = false;
                self.status = "Ready.".into();
                self.details = if paired {
                    "Setup cancelled. Any permission already granted on the TV remains until you remove it in the TV’s settings.".into()
                } else {
                    String::new()
                };
                if paired {
                    // Stay here so the pairing note stays visible.
                    self.warn(
                        "Setup cancelled.",
                        "Any permission already granted on the TV remains until you remove it in the TV’s settings.",
                    );
                    self.flow.navigated();
                } else {
                    self.library_cancelled(crate::library_flow::Slot::Tv, cx);
                }
            }
            TvMessage::Name(v) if !self.busy => {
                if let Some(d) = &mut self.tv.draft {
                    d.name = v;
                }
            }
            TvMessage::Address(v) if !self.busy => {
                if let Some(d) = &mut self.tv.draft {
                    if d.address.trim() != v.trim()
                        && let Some(s) = &mut self.tv.setup
                    {
                        s.invalidate_connection(d);
                    }
                    d.address = v;
                }
            }
            TvMessage::Legacy(v) if !self.busy => {
                if let Some(d) = &mut self.tv.draft {
                    if (d.connection == Connection::Legacy) != v
                        && let Some(s) = &mut self.tv.setup
                    {
                        s.invalidate_connection(d);
                    }
                    d.connection = if v {
                        Connection::Legacy
                    } else {
                        Connection::Encrypted
                    };
                }
            }
            TvMessage::Link(index) if !self.busy => {
                let link = index
                    .and_then(|i| i.checked_sub(1))
                    .and_then(|i| self.tv.links().get(i).cloned());
                if let Some(d) = &mut self.tv.draft {
                    d.display = link;
                }
            }
            TvMessage::Next if ready => {
                self.tv.test_confirmation = None;
                if let (Some(s), Some(tv)) = (&mut self.tv.setup, &mut self.tv.draft) {
                    if s.step == Step::Name
                        && self
                            .tv
                            .tvs
                            .iter()
                            .any(|t| t.id != tv.id && t.name.eq_ignore_ascii_case(tv.name.trim()))
                    {
                        self.failure("A TV with this name already exists.".into());
                        return;
                    }
                    if s.step == Step::Address
                        && self
                            .tv
                            .tvs
                            .iter()
                            .any(|t| t.id != tv.id && t.address == tv.address.trim())
                    {
                        self.failure(
                            "A TV with this address already exists. Edit it from the TV list."
                                .into(),
                        );
                        return;
                    }
                    if let Err(e) = s.advance(tv) {
                        self.failure(e);
                    } else {
                        self.error = false;
                        self.details.clear();
                        self.status = "Continue setting up your TV.".into();
                    }
                }
            }
            TvMessage::Back if !self.busy => {
                self.tv.test_confirmation = None;
                if let Some(s) = &mut self.tv.setup {
                    s.back();
                    self.error = false;
                    self.details.clear();
                }
            }
            TvMessage::AndroidCode(code) if !self.busy => {
                self.tv.android_code = code.chars().take(6).collect();
            }
            TvMessage::AndroidStarted(result) => {
                self.end();
                match result {
                    Ok(p) => {
                        self.tv.android_pairing = Some(p);
                        self.tv.android_code.clear();
                        self.status = "Enter the six-character code displayed on your TV.".into();
                    }
                    Err(e) => self.failure(e),
                }
            }
            TvMessage::SetupPair if ready => {
                if self.tv.setup.as_ref().is_some_and(|s| s.step == Step::Pair)
                    && let Some(tv) = self.tv.draft.clone()
                {
                    if tv.credential.is_some() {
                        self.update_tv(TvMessage::ReadDetails, cx);
                        return;
                    }
                    if tv.brand == tvs::Brand::TclAndroid {
                        if let Some(pending) = self.tv.android_pairing.take() {
                            let code = std::mem::take(&mut self.tv.android_code);
                            self.begin("Confirming the TV pairing code…");
                            work(cx, move || {
                                msg(TvMessage::SetupPaired((|| {
                                    let _lock = OperationLock::acquire()?;
                                    let pairing = pending
                                        .lock()
                                        .map_err(|_| "Pairing session unavailable.")?
                                        .take()
                                        .ok_or("Start pairing again.")?;
                                    let code = zeroize::Zeroizing::new(code);
                                    pairing.finish(&code)
                                })()))
                            });
                        } else {
                            self.begin("Connecting… The TV will display a pairing code.");
                            work(cx, move || {
                                msg(TvMessage::AndroidStarted((|| {
                                    let _lock = OperationLock::acquire()?;
                                    let p = director::windows_android_tv::Pairing::start(&tv)?;
                                    Ok(std::sync::Arc::new(std::sync::Mutex::new(Some(p))))
                                })(
                                )))
                            });
                        }
                        return;
                    }
                    self.begin("Pairing… Accept the connection prompt on your TV.");
                    work(cx, move || {
                        msg(TvMessage::SetupPaired((|| {
                            let _lock = OperationLock::acquire()?;
                            windows_tv::pair(&tv)
                        })()))
                    });
                }
            }
            TvMessage::SetupPaired(result) => match result {
                Ok(credential) => {
                    self.end();
                    if let Some(tv) = &mut self.tv.draft {
                        tv.credential = Some(credential);
                        self.update_tv(TvMessage::ReadDetails, cx);
                    }
                }
                Err(e) => self.failure(e),
            },
            TvMessage::ReadDetails if ready => {
                if let Some(tv) = self.tv.draft.clone() {
                    if self.tv.setup.is_none()
                        && self
                            .tv
                            .tvs
                            .iter()
                            .find(|t| t.id == tv.id)
                            .is_some_and(|old| !old.same_endpoint(&tv))
                    {
                        self.failure(
                            "Save the changed address first, then pair this TV again.".into(),
                        );
                        return;
                    }
                    self.begin("Reading the TV’s inputs and wake addresses…");
                    work(cx, move || {
                        msg(TvMessage::Details((|| {
                            let _lock = OperationLock::acquire()?;
                            windows_tv::inspect(&tv)
                        })()))
                    });
                }
            }
            TvMessage::Details(result) => match result {
                Ok(details) => {
                    self.end();
                    if let Some(tv) = &mut self.tv.draft {
                        details.remember(tv);
                    }
                    if let Some(s) = &mut self.tv.setup {
                        s.inputs_loaded = true;
                        s.step = Step::Input;
                        s.input_chosen = self
                            .tv
                            .draft
                            .as_ref()
                            .is_some_and(|t| t.brand != tvs::Brand::Lg);
                        s.network_warning = details.network_warning.clone();
                    }
                    self.status = if self
                        .tv
                        .draft
                        .as_ref()
                        .is_some_and(|t| t.brand != tvs::Brand::Lg)
                    {
                        "TV paired. Direct HDMI testing is optional."
                    } else {
                        "TV connected. Its inputs are ready."
                    }
                    .into();
                    self.details = details.network_warning.unwrap_or_default();
                    if self.tv.setup.is_none() {
                        // Refreshed data for a saved TV isn't an edit.
                        self.rebase_tv();
                    }
                    if self.details.is_empty() {
                        self.announce(cx);
                    } else {
                        self.warn(self.status.clone(), self.details.clone());
                    }
                }
                Err(e) => {
                    if self.tv.setup.is_some() {
                        self.failure(e);
                    } else {
                        self.end();
                        self.error = false;
                        self.warn(
                            "Showing saved TV details.",
                            format!(
                                "Live TV information is unavailable. Saved choices remain editable. {e}"
                            ),
                        );
                    }
                }
            },
            TvMessage::PcInput(index) if !self.busy => {
                let choices = self
                    .tv
                    .draft
                    .as_ref()
                    .map(TvState::inputs)
                    .unwrap_or_default();
                if let Some(tv) = &mut self.tv.draft
                    && let Some(i) = index
                {
                    tv.pc_input = if i == 0 {
                        None
                    } else {
                        choices.get(i - 1).cloned()
                    };
                    if let Some(s) = &mut self.tv.setup {
                        s.input_chosen = i == 0 || tv.pc_input.is_some();
                    }
                }
            }
            TvMessage::ManualMacEnabled(value) if !self.busy => {
                if let Some(s) = &mut self.tv.setup {
                    s.manual_mac_enabled = value;
                } else {
                    self.tv.manual_mac_enabled = value;
                }
            }
            TvMessage::ManualMac(value) if !self.busy => {
                if let Some(s) = &mut self.tv.setup {
                    s.manual_mac = value;
                } else {
                    self.tv.manual_mac = value;
                }
            }
            TvMessage::Save if ready => {
                if let Some(mut tv) = self.tv.draft.clone() {
                    if let Some(s) = &self.tv.setup {
                        if s.step != Step::Review || !s.valid_step(&tv) {
                            return;
                        }
                    } else {
                        tv.network.manual = if !self.tv.manual_mac_enabled
                            || self.tv.manual_mac.trim().is_empty()
                        {
                            None
                        } else {
                            match self.tv.manual_mac.parse() {
                                Ok(mac) => Some(mac),
                                Err(e) => {
                                    self.failure(e);
                                    return;
                                }
                            }
                        };
                    }
                    let previous = self.tv.tvs.iter().find(|t| t.id == tv.id);
                    let reconnect = self.tv.setup.is_none()
                        && previous.is_some_and(|old| !old.same_endpoint(&tv));
                    // A completed guide has already paired this exact draft endpoint.
                    let previous_for_save = if self.tv.setup.is_some() {
                        None
                    } else {
                        previous
                    };
                    if let Err(e) = tv.prepare_for_save(previous_for_save) {
                        self.failure(e);
                        return;
                    }
                    if reconnect {
                        if self.tv.tvs.iter().any(|other| {
                            other.id != tv.id
                                && (other.address == tv.address
                                    || other.name.eq_ignore_ascii_case(&tv.name))
                        }) {
                            self.failure("Each TV needs a unique name and IP address.".into());
                            return;
                        }
                        self.tv.setup = Some(Setup {
                            step: Step::Pair,
                            manual_mac: self.tv.manual_mac.clone(),
                            manual_mac_enabled: self.tv.manual_mac_enabled,
                            ..Default::default()
                        });
                        self.tv.draft = Some(tv);
                        self.error = false;
                        self.status = "Set up the new TV connection.".into();
                        self.details.clear();
                        return;
                    }
                    let expected = self.tv.tvs.clone();
                    let mut next = expected.clone();
                    if let Some(old) = next.iter_mut().find(|t| t.id == tv.id) {
                        *old = tv;
                    } else {
                        next.push(tv);
                    }
                    next.sort_by_key(|t| t.name.to_lowercase());
                    self.begin("Saving TV…");
                    work(cx, move || {
                        msg(TvMessage::Saved((|| {
                            let _lock = OperationLock::acquire()?;
                            for old in &expected {
                                if !next.iter().any(|t| t.id == old.id) {
                                    director::scenes::check_delete("tv", &old.id)?;
                                }
                            }
                            tvs::save(&tvs::path()?, &expected, &next)?;
                            Ok(next)
                        })()))
                    });
                }
            }
            TvMessage::Saved(result) => match result {
                Ok(tvs) => {
                    self.end();
                    let added = crate::library_flow::added_id(
                        self.tv.tvs.iter().map(|t| t.id.as_str()),
                        tvs.iter().map(|t| t.id.as_str()),
                    );
                    self.tv.tvs = tvs;
                    self.tip.tvs = self.tv.tvs.clone();
                    self.tip.catalog_loaded = true;
                    self.tv.draft = None;
                    self.tv.setup = None;
                    self.tv.pending_delete = None;
                    self.status = "TVs saved.".into();
                    self.guard_saved(true, cx);
                    self.announce(cx);
                    self.library_saved(crate::library_flow::Slot::Tv, added, cx);
                }
                Err(e) => self.failure(e),
            },
            TvMessage::Delete(id) if ready => self.tv.pending_delete = Some(id),
            TvMessage::DeleteClosed(result) if !self.busy => {
                if let Some(id) = self.tv.pending_delete.take()
                    && result == ContentDialogResult::Primary
                {
                    let expected = self.tv.tvs.clone();
                    let next = expected
                        .iter()
                        .filter(|t| t.id != id)
                        .cloned()
                        .collect::<Vec<_>>();
                    self.begin("Removing TV…");
                    work(cx, move || {
                        msg(TvMessage::Saved((|| {
                            let _lock = OperationLock::acquire()?;
                            tvs::save(&tvs::path()?, &expected, &next)?;
                            Ok(next)
                        })()))
                    });
                }
            }
            _ => {}
        }
    }
    pub(crate) fn tv_view(&self, cx: &ViewContext<Self>) -> View {
        if let Some(d) = &self.tv.draft {
            return if let Some(s) = &self.tv.setup {
                self.tv_setup_view(cx, d, s)
            } else {
                Grid::new().keyed_children([KeyedView::new(
                    format!("edit-tv-{}", d.id),
                    self.tv_editor(cx, d),
                )])
            };
        }
        let ready = self.tv_ready();
        let line = |text: String, size: f64, opacity: f64| -> View {
            TextBlock::new()
                .text(text)
                .font_size(size)
                .opacity(opacity)
                .text_trimming(TextTrimming::CharacterEllipsis)
                .into()
        };
        let cards = self
            .tv
            .tvs
            .iter()
            .map(|tv| {
                let controls = tv.id.clone();
                let edit = tv.id.clone();
                let delete = tv.id.clone();
                let wake = if tv.brand == tvs::Brand::TclAndroid {
                    "Remote wake"
                } else if tv.network.targets().is_empty() {
                    "Wake not set up"
                } else {
                    "Wake available"
                };
                let body = StackPanel::new().spacing(8.0).children((
                    TextBlock::new()
                        .text("CONNECTION")
                        .font_size(10.0)
                        .opacity(crate::ui::SECONDARY),
                    Grid::new()
                        .columns([GridLength::Auto, GridLength::Star(1.0)])
                        .column_spacing(12.0)
                        .children((
                            FontIcon::new()
                                .glyph("\u{e7f4}")
                                .vertical_alignment(VerticalAlignment::Center),
                            TextBlock::new()
                                .grid_column(1)
                                .text(tv.brand.label())
                                .font_size(17.0)
                                .text_trimming(TextTrimming::CharacterEllipsis),
                        )),
                    line(
                        format!(
                            "{} · {} · {wake}",
                            tv.address,
                            if tv.credential.is_some() {
                                "Paired"
                            } else {
                                "Not paired"
                            }
                        ),
                        13.0,
                        0.8,
                    ),
                    line(
                        format!(
                            "PC input: {}",
                            tv.pc_input
                                .as_ref()
                                .map_or("Not linked", |i| i.label.as_str())
                        ),
                        12.0,
                        0.8,
                    ),
                    line(
                        format!(
                            "Windows display: {}",
                            tv.display
                                .as_ref()
                                .map_or("Not linked", |d| d.name.as_str())
                        ),
                        12.0,
                        0.8,
                    ),
                    line(
                        format!(
                            "Audio: {}",
                            tv.audio.as_ref().map_or("Not linked", |a| a.name.as_str())
                        ),
                        12.0,
                        0.8,
                    ),
                ));
                KeyedView::new(
                    tv.id.clone(),
                    crate::preset_cards_ui::card(
                        &tv.name,
                        View::empty(),
                        body,
                        vec![
                            crate::preset_cards_ui::card_icon_button(Button::new())
                                .automation_name(format!("Control {}", tv.name))
                                .is_enabled(ready)
                                .on_click(cx.callback(move |()| {
                                    msg(TvMessage::Control(
                                        crate::tv_controls_ui::ControlMessage::Open(
                                            controls.clone(),
                                        ),
                                    ))
                                }))
                                .content(crate::icons::remote())
                                .tooltip("TV controls"),
                            crate::preset_cards_ui::card_icon_button(Button::new())
                                .automation_name(format!("Edit {}", tv.name))
                                .is_enabled(ready)
                                .on_click(cx.callback(move |()| msg(TvMessage::Edit(edit.clone()))))
                                .content(crate::preset_cards_ui::card_edit_icon())
                                .tooltip("Edit TV"),
                            crate::preset_cards_ui::card_icon_button(Button::new())
                                .automation_name(format!("Remove {}", tv.name))
                                .is_enabled(ready)
                                .on_click(
                                    cx.callback(move |()| msg(TvMessage::Delete(delete.clone()))),
                                )
                                .content(crate::icons::delete_small())
                                .tooltip("Remove TV"),
                        ],
                        crate::preset_cards_ui::TV_CARD_HEIGHT,
                    ),
                )
            })
            .collect::<Vec<_>>();
        let list = if self.tv.tvs.is_empty() {
            Border::new().grid_row(1).content(crate::ui::empty_state(
                if self.busy {
                    "Loading your TVs…"
                } else if !self.tv.ready {
                    "Your TVs could not be loaded"
                } else {
                    "Add your first TV"
                },
                "Choose + and Director will guide you through pairing, choosing your PC input, and setting up wake.",
                None,
            ))
        } else {
            crate::preset_cards_ui::card_grid(1, crate::preset_cards_ui::TV_CARD_HEIGHT, cards)
        };
        Grid::new()
            .rows([GridLength::Auto, GridLength::Star(1.0)])
            .children((
                crate::ui::header(
                    "TVs",
                    "Paired TVs that scenes can wake and switch.",
                    vec![
                        Button::new()
                            .style(ButtonStyle::Accent)
                            .automation_name("Add TV")
                            .is_enabled(ready)
                            .on_click(cx.message(msg(TvMessage::Add)))
                            .content(FontIcon::new().glyph("\u{e710}"))
                            .tooltip("Add TV"),
                    ],
                ),
                list,
            ))
    }
    fn tv_input_picker(&self, cx: &ViewContext<Self>, tv: &Tv, chosen: bool) -> View {
        let choices = TvState::inputs(tv);
        let index = if chosen {
            Some(
                tv.pc_input
                    .as_ref()
                    .and_then(|pc| choices.iter().position(|i| i.id == pc.id))
                    .map_or(0, |i| i + 1),
            )
        } else {
            None
        };
        ComboBox::new()
            .automation_name("PC input")
            .horizontal_alignment(HorizontalAlignment::Stretch)
            .items_source(
                std::iter::once("No PC input".to_string()).chain(choices.iter().map(input_label)),
            )
            .selected_index(index)
            .is_enabled(!self.busy)
            .on_selection_changed(cx.callback(|i| msg(TvMessage::PcInput(i))))
            .into()
    }
    fn tv_audio_links(&self) -> Vec<director::audio::DeviceSelection> {
        let mut links = self
            .outputs
            .iter()
            .map(|o| director::audio::DeviceSelection {
                id: o.id.clone(),
                name: o.name.clone(),
            })
            .collect::<Vec<_>>();
        for tv in &self.tv.tvs {
            if let Some(link) = &tv.audio
                && !links.iter().any(|a| a.id == link.id)
            {
                links.push(link.clone());
            }
        }
        if let Some(link) = self.tv.draft.as_ref().and_then(|t| t.audio.as_ref())
            && !links.iter().any(|a| a.id == link.id)
        {
            links.push(link.clone());
        }
        links
    }
    fn tv_audio_picker(&self, cx: &ViewContext<Self>, tv: &Tv) -> View {
        let links = self.tv_audio_links();
        let index = tv
            .audio
            .as_ref()
            .and_then(|a| links.iter().position(|l| l.id == a.id))
            .map_or(0, |i| i + 1);
        let labels = std::iter::once("None — leave audio independent".into())
            .chain(links.iter().map(|l| {
                format!(
                    "{}{}",
                    if links
                        .iter()
                        .filter(|other| other.name.eq_ignore_ascii_case(&l.name))
                        .count()
                        > 1
                    {
                        format!("{} · {}", l.name, l.id)
                    } else {
                        l.name.clone()
                    },
                    if self.outputs.iter().any(|o| o.id == l.id && o.active) {
                        ""
                    } else {
                        " — remembered / unavailable"
                    }
                )
            }))
            .collect::<Vec<String>>();
        StackPanel::new().spacing(8.0).children((
            TextBlock::new().text("TV audio output (optional)").font_weight(FontWeight::SEMI_BOLD),
            ComboBox::new().automation_name("Linked TV audio output").items_source(labels).selected_index(Some(index)).horizontal_alignment(HorizontalAlignment::Stretch).is_enabled(!self.busy).on_selection_changed(cx.callback(|i|msg(TvMessage::AudioLink(i)))),
            if let Some(audio) = &tv.audio { self.audio_relink_button(cx, &audio.id, Some(&audio.name), false) } else { View::empty() },
            note("Choose the TV’s Windows HDMI audio output. Previously seen outputs are available in this picker while offline."),
        ))
    }
    fn tv_display_picker(&self, cx: &ViewContext<Self>, tv: &Tv) -> View {
        let links = self.tv.links();
        let index = tv
            .display
            .as_ref()
            .and_then(|d| {
                links
                    .iter()
                    .position(|l| l.device_path.eq_ignore_ascii_case(&d.device_path))
            })
            .map_or(0, |i| i + 1);
        let labels = std::iter::once("None — control this TV independently".into())
            .chain(links.iter().map(|d| {
                format!(
                    "{}{}",
                    d.name,
                    if self
                        .tv
                        .displays
                        .iter()
                        .any(|l| l.device_path.eq_ignore_ascii_case(&d.device_path))
                    {
                        ""
                    } else {
                        " — unavailable"
                    }
                )
            }))
            .collect::<Vec<String>>();
        ComboBox::new()
            .automation_name("Linked Windows display")
            .horizontal_alignment(HorizontalAlignment::Stretch)
            .items_source(labels)
            .selected_index(Some(index))
            .is_enabled(!self.busy)
            .on_selection_changed(cx.callback(|i| msg(TvMessage::Link(i))))
            .into()
    }
    fn android_pairing_view(&self, cx: &ViewContext<Self>, tv: &Tv) -> View {
        let waiting = self.tv.android_waiting();
        StackPanel::new().spacing(16.0).children((
            TextBlock::new().text(tv.name.clone()).font_size(crate::ui::SUBTITLE),
            note(if waiting{"Enter the six-character code shown on your TV. The code is used only for this pairing and is never saved or logged."}else{"Turn on the TV, then choose Pair TV. Director will ask Android TV Remote Service to show a pairing code. No Android developer settings are needed."}),
            if waiting{StackPanel::new().spacing(8.0).children((TextBlock::new().text("TV pairing code").font_weight(FontWeight::SEMI_BOLD),TextBox::new().automation_name("Android TV pairing code").text(self.tv.android_code.clone()).placeholder_text("e.g. A1B2C3").is_enabled(!self.busy).on_text_changed(cx.callback(|s|msg(TvMessage::AndroidCode(s)))),note("Codes expire after two minutes. Use Back, then Pair TV to request a new code.")))}else{View::empty()},
            note("Only TCL Android / Google TV is supported here. Roku and Fire TV models use different control systems."),
            self.tv_test_report(cx,tv,false),
        ))
    }
    fn tv_connection_options(&self, cx: &ViewContext<Self>, tv: &Tv) -> View {
        if tv.brand == tvs::Brand::TclAndroid {
            return note(
                "Encrypted local connection · Android TV Remote v2. No developer mode or ADB required.",
            );
        }
        StackPanel::new().spacing(8.0).children((
   ToggleSwitch::new().automation_name("Use legacy TV connection").is_on(tv.connection==Connection::Legacy).is_enabled(!self.busy).on_toggled(cx.callback(|v|msg(TvMessage::Legacy(v)))).slot(ToggleSwitchSlot::Header,"Use legacy connection").tooltip(if tv.brand==tvs::Brand::Samsung{"Off: encrypted port 8002. On: legacy unencrypted port 8001. No automatic downgrade."}else{"Off: encrypted connection on port 3001. On: unencrypted port 3000 for older TVs. No automatic downgrade."}),
   note(if tv.connection==Connection::Legacy{"Legacy mode sends pairing data unencrypted. Use it only on a trusted local network when required by the TV."}else{"Leave this off unless your TV requires the older connection method."}),
  ))
    }
    fn tv_mac_field(&self, cx: &ViewContext<Self>, value: &str, enabled: bool) -> View {
        StackPanel::new().spacing(12.0).children((
            ToggleSwitch::new().automation_name("Enter MAC address manually").is_on(enabled).is_enabled(!self.busy).on_toggled(cx.callback(|v|msg(TvMessage::ManualMacEnabled(v)))).slot(ToggleSwitchSlot::Header,"Enter MAC address manually").tooltip("Advanced: add a wake address when automatic learning is unavailable. Off uses only automatically learned addresses after saving."),
            if enabled{self.tv_manual_mac_input(cx,value)}else{View::empty()},
        ))
    }
    fn tv_manual_mac_input(&self, cx: &ViewContext<Self>, value: &str) -> View {
        StackPanel::new().spacing(8.0).children((
   TextBlock::new().text("Manual MAC address (optional)").font_weight(FontWeight::SEMI_BOLD),
   TextBox::new().automation_name("Manual TV MAC address").text(value).placeholder_text("00:11:22:33:44:55").is_enabled(!self.busy).on_text_changed(cx.callback(|v|msg(TvMessage::ManualMac(v)))),
   note(if value.trim().is_empty(){"Use this if the TV cannot report its address. Find it in the TV’s network settings.".into()}else{value.parse::<tv_network::MacAddress>().err().unwrap_or_default()}),
  ))
    }
    fn tv_setup_view(&self, cx: &ViewContext<Self>, tv: &Tv, s: &Setup) -> View {
        let content=match s.step {
   Step::Brand=>StackPanel::new().spacing(16.0).children((note("Choose the TV’s local control platform. Director connects directly on your network."),ComboBox::new().automation_name("TV brand").items_source(["LG webOS","Samsung Tizen","TCL — Android / Google TV"]).selected_index(Some(match tv.brand{tvs::Brand::Lg=>0,tvs::Brand::Samsung=>1,tvs::Brand::TclAndroid=>2})).horizontal_alignment(HorizontalAlignment::Stretch).is_enabled(!self.busy).on_selection_changed(cx.callback(|i|msg(TvMessage::Brand(i)))),note("Samsung and TCL HDMI controls appear only after a direct-command test succeeds and you confirm it. Director does not navigate input menus."))),
   Step::Name=>StackPanel::new().spacing(14.0).children((
    note("Give this TV a name you’ll recognize in Director and scenes."),
    TextBox::new().automation_name("TV name").text(tv.name.clone()).placeholder_text("e.g. Living room TV").is_enabled(!self.busy).on_text_changed(cx.callback(|v|msg(TvMessage::Name(v)))),
   )),
   Step::Address=>StackPanel::new().spacing(16.0).children((
    note("Enter the TV’s IPv4 address from its network settings. Keep the TV on and connected to your network."),
    TextBox::new().automation_name("TV IP address").text(tv.address.clone()).placeholder_text("192.168.1.50").is_enabled(!self.busy).on_text_changed(cx.callback(|v|msg(TvMessage::Address(v)))),
    self.tv_connection_options(cx,tv),
   )),
   Step::Pair if tv.brand==tvs::Brand::TclAndroid=>self.android_pairing_view(cx,tv),
   Step::Pair=>StackPanel::new().spacing(16.0).children((
    TextBlock::new().text(format!("{} · {}",tv.name,tv.address)).font_size(crate::ui::SUBTITLE),
    note(if tv.credential.is_some(){"Pairing succeeded. Director will read the TV’s inputs and network addresses. If that read failed, retry below without pairing again."}else{"Choose Pair TV, then accept the connection prompt on the TV. Director will read its inputs and available wake addresses automatically."}),
    note("If the TV cannot be reached, use Back to correct its IP address or connection method."),
    if tv.brand!=tvs::Brand::Lg{self.tv_test_report(cx,tv,false)}else{View::empty()},
   )),
   Step::Input if tv.brand!=tvs::Brand::Lg=>self.samsung_compatibility(cx,tv),
   Step::Input=>StackPanel::new().spacing(16.0).children((
    note("Select the connection used by this PC. This saves the connection’s identity; it does not switch the TV’s input."),
    self.tv_input_picker(cx,tv,s.input_chosen),
    note(if tv.inputs.is_empty(){"The TV reported no external inputs. You can explicitly choose No PC input and continue."}else{"The list comes directly from your TV. Choose No PC input if this TV will only be used for other actions."}),
   )),
   Step::Wake=>StackPanel::new().spacing(16.0).children((
    note(if tv.network.targets().is_empty(){"No wake address was reported. You can enter one below, or skip wake setup and add it later."}else{"Director remembered these TV network addresses. The Wake action will send a request to the saved adapters."}),
    TextBlock::new().text(tv.network.summary()).text_wrapping(TextWrapping::Wrap),
    note(s.network_warning.clone().unwrap_or_default()),
    self.tv_mac_field(cx,&s.manual_mac,s.manual_mac_enabled),
    if tv.brand==tvs::Brand::TclAndroid{note("Enable Screenless service / network standby in the TV’s power settings. Director uses the direct Wake command while reachable, with optional Wake-on-LAN fallback if a MAC is saved.")}else{View::empty()},
    note("For wake to work, keep the TV connected to the network and enable its network/mobile power-on setting. Availability depends on the TV and connection."),
   )),
   Step::Display=>StackPanel::new().spacing(16.0).children((
    note("Optionally identify which Windows monitor entry belongs to this TV."),
    self.tv_display_picker(cx,tv),
    self.tv_audio_picker(cx,tv),
    note("This link helps coordinate scenes. It does not change your screens or bind the TV to a display preset."),
   )),
   Step::Review=>StackPanel::new().spacing(16.0).children((
    TextBlock::new().text(tv.name.clone()).font_size(24.0).font_weight(FontWeight::SEMI_BOLD).text_wrapping(TextWrapping::Wrap),
    note(format!("Address: {}\nPairing: complete\nPC input: {}\nWindows display: {}\nWake: {}",tv.address,tv.pc_input.as_ref().map_or("None",|i|i.label.as_str()),tv.display.as_ref().map_or("Not linked",|d|d.name.as_str()),if tv.brand==tvs::Brand::TclAndroid{"Direct remote; network standby required"}else if tv.network.targets().is_empty(){"Not configured"}else{"Address saved"})),
    note("Save this TV to finish. Wake, input switching, and other actions will be chosen separately when configuring a scene."),
   )),
  };
        let pair = s.step == Step::Pair && !s.valid_step(tv);
        let label = if pair && tv.brand == tvs::Brand::TclAndroid && self.tv.android_waiting() {
            "Confirm code"
        } else if pair {
            if tv.credential.is_some() {
                "Retry"
            } else {
                "Pair TV"
            }
        } else if s.step == Step::Review {
            "Save TV"
        } else if s.step == Step::Wake
            && tv.network.targets().is_empty()
            && (!s.manual_mac_enabled || s.manual_mac.trim().is_empty())
        {
            "Skip wake setup"
        } else {
            if s.step == Step::Input
                && tv.brand != tvs::Brand::Lg
                && tv.compatibility().inputs().is_empty()
            {
                "Skip input control"
            } else {
                "Next"
            }
        };
        let action = if pair {
            TvMessage::SetupPair
        } else if s.step == Step::Review {
            TvMessage::Save
        } else {
            TvMessage::Next
        };
        let enabled = self.tv_ready()
            && (pair || s.valid_step(tv))
            && (tv.brand != tvs::Brand::TclAndroid
                || !pair
                || !self.tv.android_waiting()
                || director::android_tv::code(&self.tv.android_code).is_ok());
        crate::ui::page(
            crate::ui::header(
                s.step.title(),
                &format!("TV setup · Step {} of {}", s.step.number(), Step::COUNT),
                vec![],
            ),
            Border::new()
                .max_width(crate::ui::FORM_WIDTH)
                .horizontal_alignment(HorizontalAlignment::Left)
                .content(content),
            Some(crate::ui::footer(
                Button::new()
                    .horizontal_alignment(HorizontalAlignment::Left)
                    .is_enabled(!self.busy && s.step != Step::Brand)
                    .on_click(cx.message(msg(TvMessage::Back)))
                    .content("Back"),
                vec![
                    Button::new()
                        .is_enabled(!self.busy)
                        .on_click(cx.message(msg(TvMessage::Cancel)))
                        .content("Cancel"),
                    Button::new()
                        .style(ButtonStyle::Accent)
                        .is_enabled(enabled)
                        .on_click(cx.message(msg(action)))
                        .content(label),
                ],
            )),
        )
    }
    fn tv_editor(&self, cx: &ViewContext<Self>, tv: &Tv) -> View {
        let original = self.tv.tvs.iter().find(|t| t.id == tv.id);
        let changed = original.is_some_and(|o| !o.same_endpoint(tv));
        let valid = tv.clone().prepare_for_save(original).is_ok()
            && (!self.tv.manual_mac_enabled
                || self.tv.manual_mac.trim().is_empty()
                || self.tv.manual_mac.parse::<tv_network::MacAddress>().is_ok());
        Grid::new().rows([GridLength::Auto,GridLength::Star(1.0),GridLength::Auto]).children((
   crate::ui::header("Edit TV","Device connections · Scene actions are configured separately.",vec![]),
   ScrollViewer::new().grid_row(1).horizontal_scroll_bar_visibility(ScrollBarVisibility::Disabled).content(StackPanel::new().spacing(14.0).max_width(650.0).horizontal_alignment(HorizontalAlignment::Left).children((
    TextBlock::new().text("TV name").font_weight(FontWeight::SEMI_BOLD),
    TextBox::new().automation_name("TV name").text(tv.name.clone()).is_enabled(!self.busy).on_text_changed(cx.callback(|v|msg(TvMessage::Name(v)))),
    TextBlock::new().text("IP address").font_weight(FontWeight::SEMI_BOLD),
    TextBox::new().automation_name("TV IP address").text(tv.address.clone()).is_enabled(!self.busy).on_text_changed(cx.callback(|v|msg(TvMessage::Address(v)))),
    self.tv_connection_options(cx,tv),
    note(if changed{"Continue through setup to pair the new connection. Your saved TV stays unchanged until you finish."}else{"Pairing is retained when you edit the name, PC input, wake addresses, or display link."}),
    TextBlock::new().text("PC input").font_weight(FontWeight::SEMI_BOLD),
    if tv.brand==tvs::Brand::Lg{self.tv_input_picker(cx,tv,true)}else{self.samsung_compatibility(cx,tv)},
    TextBlock::new().text("Wake addresses").font_weight(FontWeight::SEMI_BOLD),
    note(if tv.network.targets().is_empty(){"No wake address saved. You can enter a manual MAC below.".into()}else{tv.network.summary()}),
    self.tv_mac_field(cx,&self.tv.manual_mac,self.tv.manual_mac_enabled),
    TextBlock::new().text("Windows display (optional)").font_weight(FontWeight::SEMI_BOLD),
    self.tv_display_picker(cx,tv),
    self.tv_audio_picker(cx,tv),
    note("Connection choices are saved only. No input, power, or display change is applied here."),
   ))),
   Grid::new().grid_row(2).columns([GridLength::Star(1.0),GridLength::Auto]).margin(Thickness::new(0.0,20.0,0.0,0.0)).children((
    TextBlock::new().grid_column(0).text("Saving remembers your choices.").opacity(0.72).text_wrapping(TextWrapping::Wrap).vertical_alignment(VerticalAlignment::Center),
    StackPanel::new().grid_column(1).orientation(Orientation::Horizontal).spacing(8.0).children((
     Button::new().is_enabled(!self.busy).on_click(cx.message(msg(TvMessage::Cancel))).content("Cancel"),
     Button::new().style(ButtonStyle::Accent).is_enabled(self.tv_ready()&&valid&&(changed||self.draft_changed(crate::Page::Tvs))).on_click(cx.message(msg(TvMessage::Save))).content(if changed{"Continue setup"}else{"Save TV"}),
    )),
   )),
  ))
    }
    pub(crate) fn tv_dialog(&self, cx: &ViewContext<Self>) -> View {
        if self.tv.controls.is_some() {
            return self.tv_controls_dialog(cx);
        }
        self.tv.pending_delete.as_ref().map(|id|{
   let name=self.tv.tvs.iter().find(|t|&t.id==id).map_or("this TV",|t|t.name.as_str());
   ContentDialog::new().is_open(true).title(format!("Remove “{name}”?")).primary_button_text("Remove TV").close_button_text("Cancel").on_closed(cx.callback(|r|msg(TvMessage::DeleteClosed(r)))).content("This removes the TV and its saved pairing from Director. To revoke access on the TV too, remove Director in the TV’s connection settings.")
  }).unwrap_or_else(View::empty)
    }
}

#[cfg(test)]
pub(crate) static GUIDE_PREVIEW: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);
#[cfg(test)]
impl DirectorApp {
    pub(crate) fn install_tv_guide_preview(&mut self) {
        self.page = crate::Page::Tvs;
        self.tv.ready = true;
        self.tv.displays = vec![DisplayLink {
            device_path: "test-preview-display".into(),
            name: "Preview Windows display (test fixture)".into(),
        }];
        self.tv.draft = Some(Tv {
            id: "test-guide-preview".into(),
            name: "TV guide preview (test fixture)".into(),
            address: "192.0.2.50".into(),
            brand: Default::default(),
            samsung: Default::default(),
            android: Default::default(),
            connection: Connection::Encrypted,
            audio: None,
            credential: Some(vec![1]),
            display: None,
            inputs: vec![
                Input {
                    id: "HDMI_1".into(),
                    label: "PC".into(),
                },
                Input {
                    id: "HDMI_2".into(),
                    label: "Console".into(),
                },
            ],
            pc_input: None,
            network: tv_network::NetworkAddresses {
                wired: Some("00:11:22:33:44:55".parse().unwrap()),
                ..Default::default()
            },
        });
        self.tv.setup = Some(Setup {
            step: Step::Input,
            inputs_loaded: true,
            ..Default::default()
        });
    }
}
#[cfg(test)]
mod native_preview {
    use super::*;
    #[test]
    #[ignore = "Owned-window visual fixture; run explicitly with UI automation, never contact devices or save it"]
    fn guide_fixture() {
        GUIDE_PREVIEW.store(true, std::sync::atomic::Ordering::Release);
        App::run_component::<DirectorApp>(()).unwrap();
    }
}
