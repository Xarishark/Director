//! Provider boundary shared by setup, controls, CLI, display preparation and scenes.
use crate::{
    lg_protocol::Input,
    tv_control::{Backend, PowerState},
    tv_network::NetworkAddresses,
    tvs::{Brand, Tv},
};
pub struct WindowsTvControls<'a> {
    inner: Provider<'a>,
    tv_id: &'a str,
}
enum Provider<'a> {
    Lg(crate::windows_lg::WindowsTvControls<'a>),
    Samsung(crate::windows_samsung::Controls<'a>),
    Android(Box<crate::windows_android_tv::Controls<'a>>),
}
impl<'a> WindowsTvControls<'a> {
    pub fn new(tv: &'a Tv) -> Self {
        Self {
            tv_id: &tv.id,
            inner: match tv.brand {
                Brand::Lg => Provider::Lg(crate::windows_lg::WindowsTvControls::new(tv)),
                Brand::Samsung => Provider::Samsung(crate::windows_samsung::Controls::new(tv)),
                Brand::TclAndroid => {
                    Provider::Android(Box::new(crate::windows_android_tv::Controls::new(tv)))
                }
            },
        }
    }
}
macro_rules! delegate{($self:ident,$method:ident $(,$arg:expr)*)=>{match &mut $self.inner{Provider::Lg(p)=>p.$method($($arg),*),Provider::Samsung(p)=>p.$method($($arg),*),Provider::Android(p)=>p.$method($($arg),*)}}}
impl Backend for WindowsTvControls<'_> {
    fn power(&mut self) -> Result<PowerState, String> {
        let start = std::time::Instant::now();
        let result = delegate!(self, power);
        crate::tv_report::record(
            self.tv_id,
            crate::tv_report::Action::PowerStatus,
            start,
            &result,
            result.as_ref().ok().copied(),
        );
        result
    }
    fn inputs(&mut self) -> Result<Vec<Input>, String> {
        delegate!(self, inputs)
    }
    fn current_input(&mut self) -> Result<Option<String>, String> {
        delegate!(self, current_input)
    }
    fn wake(&mut self) -> Result<crate::tv_network::WakeReport, String> {
        crate::tv_report::measure(self.tv_id, crate::tv_report::Action::Wake, || {
            delegate!(self, wake)
        })
    }
    fn off(&mut self) -> Result<(), String> {
        crate::tv_report::measure(self.tv_id, crate::tv_report::Action::TurnOff, || {
            delegate!(self, off)
        })
    }
    fn switch_input(&mut self, id: &str) -> Result<(), String> {
        crate::tv_report::measure(self.tv_id, crate::tv_report::Action::HdmiSwitch, || {
            delegate!(self, switch_input, id)
        })
    }
    fn input_readback(&self) -> bool {
        matches!(self.inner, Provider::Lg(_))
    }
    fn command_acknowledged(&self) -> bool {
        matches!(self.inner, Provider::Lg(_))
    }
}
#[derive(Clone)]
pub struct TvDetails {
    pub inputs: Vec<Input>,
    pub network: NetworkAddresses,
    pub network_warning: Option<String>,
    pub samsung: Option<crate::samsung::Info>,
}
impl TvDetails {
    pub fn remember(&self, tv: &mut Tv) {
        tv.network.merge_learned(&self.network);
        if tv.brand == Brand::Samsung {
            if let Some(info) = &self.samsung {
                tv.samsung.model = info.model.clone();
                tv.samsung.firmware = info.firmware.clone();
                tv.samsung.token_required = info.token_required;
            }
            tv.inputs = tv.samsung.inputs();
            if tv
                .pc_input
                .as_ref()
                .is_some_and(|pc| !tv.inputs.iter().any(|i| i.id == pc.id))
            {
                tv.pc_input = None;
            }
        } else if tv.brand == Brand::TclAndroid {
            if let Some(info) = &self.samsung {
                tv.android.model = info.model.clone();
                tv.android.firmware = info.firmware.clone();
            }
            tv.inputs = tv.android.inputs();
            if tv
                .pc_input
                .as_ref()
                .is_some_and(|pc| !tv.inputs.iter().any(|i| i.id == pc.id))
            {
                tv.pc_input = None;
            }
        } else {
            tv.inputs = self.inputs.clone();
        }
    }
}
pub fn pair(tv: &Tv) -> Result<Vec<u8>, String> {
    crate::tv_report::measure(&tv.id, crate::tv_report::Action::Pair, || match tv.brand {
        Brand::Lg => crate::windows_lg::pair(tv),
        Brand::Samsung => crate::windows_samsung::pair(tv),
        Brand::TclAndroid => {
            let pending = crate::windows_android_tv::Pairing::start(tv)?;
            println!("Enter the six-character code displayed on your TV:");
            let mut code = zeroize::Zeroizing::new(String::new());
            std::io::stdin()
                .read_line(&mut code)
                .map_err(|_| "Could not read the TV pairing code.")?;
            pending.finish(code.trim())
        }
    })
}
pub fn inspect(tv: &Tv) -> Result<TvDetails, String> {
    crate::tv_report::measure(&tv.id, crate::tv_report::Action::Inspect, || {
        match tv.brand{Brand::TclAndroid=>{let r=crate::windows_android_tv::Remote::connect(tv)?;Ok(TvDetails{inputs:tv.android.inputs(),network:Default::default(),network_warning:Some("Enable Screenless service / network standby on the TV to keep remote wake available. The protocol does not report MAC addresses; manual wake MAC is optional.".into()),samsung:Some(crate::samsung::Info{model:r.model,firmware:r.version,token_required:None,power:r.power,network:Default::default()})})},Brand::Lg=>{let d=crate::windows_lg::inspect(tv)?;Ok(TvDetails{inputs:d.inputs,network:d.network,network_warning:d.network_warning,samsung:None})},Brand::Samsung=>match crate::tv_report::measure(&tv.id,crate::tv_report::Action::MetadataRead,||crate::windows_samsung::info(tv)){Ok(info)=>Ok(TvDetails{inputs:tv.samsung.inputs(),network:info.network.clone(),network_warning:None,samsung:Some(info)}),Err(_)=>Ok(TvDetails{inputs:tv.samsung.inputs(),network:Default::default(),network_warning:Some("Pairing is retained. Model/firmware and reported power could not be read; compatibility testing remains optional.".into()),samsung:None})}}
    })
}
pub fn check(tv: &Tv) -> Result<Vec<Input>, String> {
    match tv.brand {
        Brand::TclAndroid => {
            crate::windows_android_tv::Remote::connect(tv)?;
            Ok(tv.android.inputs())
        }
        Brand::Lg => crate::windows_lg::check(tv),
        Brand::Samsung => {
            crate::windows_samsung::info(tv)?;
            Ok(tv.samsung.inputs())
        }
    }
}
pub fn pair_with_details(tv: &Tv) -> Result<(Tv, Option<String>), String> {
    let mut tv = tv.clone();
    tv.credential = Some(pair(&tv)?);
    if tv.brand != Brand::Lg {
        tv.compatibility_mut().tests.clear();
        tv.inputs.clear();
        tv.pc_input = None;
    }
    let d = inspect(&tv)?;
    d.remember(&mut tv);
    Ok((tv, d.network_warning))
}
pub fn watch_power(tv: Tv) -> crate::windows_lg::PowerWatch {
    if tv.brand == Brand::Lg {
        return crate::windows_lg::watch_power(tv);
    }
    use std::sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc,
    };
    let cancel = Arc::new(AtomicBool::new(false));
    let c = cancel.clone();
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        while !c.load(Ordering::Acquire) {
            let at = std::time::Instant::now();
            let result = if tv.brand == Brand::TclAndroid {
                crate::windows_android_tv::Remote::connect(&tv).map(|r| r.power)
            } else {
                crate::windows_samsung::info(&tv).map(|i| i.power)
            };
            if c.load(Ordering::Acquire) {
                break;
            }
            let event = match result {
                Ok(power) => crate::windows_lg::PowerEvent::Reported(power),
                Err(e) => crate::windows_lg::PowerEvent::Unavailable(e),
            };
            if tx
                .send(crate::windows_lg::PowerUpdate { at, event })
                .is_err()
            {
                break;
            }
            for _ in 0..40 {
                if c.load(Ordering::Acquire) {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
        }
    });
    crate::windows_lg::PowerWatch {
        cancel,
        receiver: Arc::new(Mutex::new(rx)),
    }
}
