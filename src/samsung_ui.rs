//! Explicit, optional direct-command compatibility testing for Samsung and Android TV.
use crate::{DirectorApp, Message, tv_ui::TvMessage};
use director::tv::tvs::Tv;
use windows_reactor::*;
fn msg(m: TvMessage) -> Message {
    Message::Tv(m)
}
use crate::ui::note;
impl DirectorApp {
    pub(crate) fn tv_test_report(&self, cx: &ViewContext<Self>, tv: &Tv, checks: bool) -> View {
        let f = &tv.compatibility().feedback;
        StackPanel::new().spacing(12.0).children((
            TextBlock::new().text("Test report").font_size(crate::ui::SUBTITLE).font_weight(FontWeight::SEMI_BOLD),
            note("Enter the TV model and software version from its Settings → About page if you can find them. Use only model/version information here."),
            TextBox::new().automation_name("Report TV model").text(f.tv_model.clone()).placeholder_text(tv.compatibility().model.as_deref().unwrap_or("TV model, for example TCL 55C745")).is_enabled(!self.busy).on_text_changed(cx.callback(|v|msg(TvMessage::ReportModel(v)))).slot(TextBoxSlot::Header,"TV model (optional if already detected)"),
            TextBox::new().automation_name("Report TV software").text(f.tv_software.clone()).placeholder_text("Version from the TV’s About page, if available").is_enabled(!self.busy).on_text_changed(cx.callback(|v|msg(TvMessage::ReportSoftware(v)))).slot(TextBoxSlot::Header,"TV software version (optional)"),
            ComboBox::new().automation_name("Report TV network").items_source(["Not specified","Wi-Fi","Ethernet cable"]).selected_index(Some(f.network.index())).horizontal_alignment(HorizontalAlignment::Stretch).is_enabled(!self.busy).on_selection_changed(cx.callback(|v|msg(TvMessage::ReportNetwork(v)))).slot(ComboBoxSlot::Header,"How is the TV connected to the router?"),
            if checks{StackPanel::new().spacing(12.0).children((
                note("After trying TV controls, mark what you actually saw. Leave anything you did not try as Not tested."),
                ComboBox::new().automation_name("Report power status correct").items_source(["Not tested","Yes","No"]).selected_index(Some(f.power_status_correct.index())).horizontal_alignment(HorizontalAlignment::Stretch).is_enabled(!self.busy).on_selection_changed(cx.callback(|v|msg(TvMessage::ReportAnswer(0,v)))).slot(ComboBoxSlot::Header,"Did Director show the correct On/Off status?"),
                ComboBox::new().automation_name("Report turn off worked").items_source(["Not tested","Yes","No"]).selected_index(Some(f.turn_off_worked.index())).horizontal_alignment(HorizontalAlignment::Stretch).is_enabled(!self.busy).on_selection_changed(cx.callback(|v|msg(TvMessage::ReportAnswer(1,v)))).slot(ComboBoxSlot::Header,"Did Turn off actually turn the TV off?"),
                ComboBox::new().automation_name("Report wake worked").items_source(["Not tested","Yes","No"]).selected_index(Some(f.wake_worked.index())).horizontal_alignment(HorizontalAlignment::Stretch).is_enabled(!self.busy).on_selection_changed(cx.callback(|v|msg(TvMessage::ReportAnswer(2,v)))).slot(ComboBoxSlot::Header,"Did Wake actually turn the TV on?"),
            ))}else{View::empty()},
            note("Export one file to send back. It includes test results and sanitized diagnostics, with no pairing secrets, IP/MAC addresses or raw logs. Nothing is uploaded."),
            Button::new().automation_name(if tv.brand==director::tv::tvs::Brand::TclAndroid{"Export Android TV compatibility report"}else{"Export Samsung compatibility report"}).is_enabled(!self.busy).on_click(cx.message(msg(TvMessage::ExportReport))).content("Export report…"),
        ))
    }
    pub(crate) fn samsung_compatibility(&self, cx: &ViewContext<Self>, tv: &Tv) -> View {
        let paired = tv.credential.is_some()
            && self
                .tv
                .tvs
                .iter()
                .find(|old| old.id == tv.id)
                .is_none_or(|old| old.same_endpoint(tv));
        let confirmed = tv.compatibility().inputs();
        let port = self.tv.test_port.clamp(1, 4);
        let pending = self
            .tv
            .test_confirmation
            .and_then(|i| tv.compatibility().tests.get(i));
        StackPanel::new().spacing(16.0).children((
 TextBlock::new().text(if tv.brand==director::tv::tvs::Brand::TclAndroid{"Android TV compatibility"}else{"Samsung compatibility"}).font_size(crate::ui::SUBTITLE).font_weight(FontWeight::SEMI_BOLD),
 note(format!("Model: {} · {}: {}",tv.compatibility().model.as_deref().unwrap_or("Not reported"),if tv.brand==director::tv::tvs::Brand::TclAndroid{"Remote Service"}else{"Firmware"},tv.compatibility().firmware.as_deref().unwrap_or("Not reported"))),
 note("Pairing enables local power control. Input switching is optional. These TVs do not reliably acknowledge remote keys or report the selected input. Only commands you confirm will appear in controls and scenes."),
 if paired{StackPanel::new().spacing(12.0).children((
 TextBlock::new().text("Test a direct HDMI command").font_weight(FontWeight::SEMI_BOLD),
 ComboBox::new().automation_name(if tv.brand==director::tv::tvs::Brand::TclAndroid{"Android TV HDMI test port"}else{"Samsung HDMI test port"}).items_source(["HDMI 1","HDMI 2","HDMI 3","HDMI 4"]).selected_index(Some(usize::from(port-1))).horizontal_alignment(HorizontalAlignment::Stretch).is_enabled(!self.busy&&pending.is_none()).on_selection_changed(cx.callback(|i|msg(TvMessage::TestPort(i)))),
 note("Connect something visible to the chosen port. Using the physical remote, switch the TV to a different input before testing. Director sends one direct command; it does not navigate menus."),
 CheckBox::new().automation_name("TV is on a different input").is_checked(Some(self.tv.test_ready)).is_enabled(!self.busy&&pending.is_none()).on_is_checked_changed(cx.callback(|v|msg(TvMessage::TestReady(v)))).content("The TV is on a different input"),
 Button::new().automation_name(if tv.brand==director::tv::tvs::Brand::TclAndroid{"Test direct Android TV HDMI command"}else{"Test direct Samsung HDMI command"}).is_enabled(!self.busy&&self.tv.test_ready&&pending.is_none()).on_click(cx.message(msg(TvMessage::TestHdmi))).content(format!("Test HDMI {port}")),
 if let Some(test)=pending{Border::new().padding(16.0).corner_radius(crate::ui::RADIUS_L).border_brush(crate::ui::card_stroke()).border_thickness(1.0).content(StackPanel::new().spacing(12.0).children((TextBlock::new().text(format!("Did the TV switch to HDMI {}?",test.port)).font_weight(FontWeight::SEMI_BOLD).text_wrapping(TextWrapping::Wrap),note("Confirm only if the TV actually changed from the other input to the selected HDMI port."),StackPanel::new().orientation(Orientation::Horizontal).spacing(8.0).children((Button::new().automation_name(if tv.brand==director::tv::tvs::Brand::TclAndroid{"Android TV HDMI test succeeded"}else{"Samsung HDMI test succeeded"}).style(ButtonStyle::Accent).is_enabled(!self.busy).on_click(cx.message(msg(TvMessage::ConfirmTest(true)))).content("Yes, it switched"),Button::new().automation_name(if tv.brand==director::tv::tvs::Brand::TclAndroid{"Android TV HDMI test failed"}else{"Samsung HDMI test failed"}).is_enabled(!self.busy).on_click(cx.message(msg(TvMessage::ConfirmTest(false)))).content("No"))))))}else{View::empty()},
 note(if confirmed.is_empty(){"No direct inputs confirmed. Input controls stay hidden; you can skip this test and use power control.".into()}else{format!("Confirmed: {}",confirmed.iter().map(|i|i.label.as_str()).collect::<Vec<_>>().join(", "))}),
 if confirmed.is_empty(){View::empty()}else{ComboBox::new().automation_name(if tv.brand==director::tv::tvs::Brand::TclAndroid{"Android TV PC input"}else{"Samsung PC input"}).items_source(std::iter::once("No automatic PC input".into()).chain(confirmed.iter().map(|i|i.label.clone())).collect::<Vec<String>>()).selected_index(Some(tv.pc_input.as_ref().and_then(|pc|confirmed.iter().position(|i|i.id==pc.id)).map_or(0,|i|i+1))).horizontal_alignment(HorizontalAlignment::Stretch).is_enabled(!self.busy).on_selection_changed(cx.callback(|i|msg(TvMessage::PcInput(i)))).slot(ComboBoxSlot::Header,"PC input for scenes")},
 ))}else{note("Pair this TV to run compatibility tests.")},
 self.tv_test_report(cx,tv,true),
 ))
    }
}
#[cfg(test)]
pub(crate) static PREVIEW: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);
#[cfg(test)]
#[test]
#[ignore = "Owned-window Samsung setup fixture; no network, commands or real settings writes"]
fn samsung_setup_fixture() {
    PREVIEW.store(true, std::sync::atomic::Ordering::Release);
    crate::preset_cards_preview::ENABLED.store(true, std::sync::atomic::Ordering::Release);
    director::diagnostics::init();
    windows_reactor::App::run_component::<DirectorApp>(()).unwrap();
    director::diagnostics::flush();
}

#[cfg(test)]
#[test]
#[ignore = "Owned TCL setup fixture; no real network or settings writes"]
fn android_setup_fixture() {
    PREVIEW.store(true, std::sync::atomic::Ordering::Release);
    crate::preset_cards_preview::ENABLED.store(true, std::sync::atomic::Ordering::Release);
    director::diagnostics::init();
    windows_reactor::App::run_component::<DirectorApp>(()).unwrap();
    director::diagnostics::flush();
}
