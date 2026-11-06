//! Setup progress and validation; no device actions live here.
use crate::tvs::Tv;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    Brand,
    Name,
    Address,
    Pair,
    Input,
    Wake,
    Display,
    Review,
}
impl Step {
    /// How many steps the guide has; the last is Review.
    pub const COUNT: usize = Self::Review.number();
    pub const fn number(self) -> usize {
        match self {
            Self::Brand => 1,
            Self::Name => 2,
            Self::Address => 3,
            Self::Pair => 4,
            Self::Input => 5,
            Self::Wake => 6,
            Self::Display => 7,
            Self::Review => 8,
        }
    }
    pub fn title(self) -> &'static str {
        match self {
            Self::Brand => "Choose your TV brand",
            Self::Name => "Name your TV",
            Self::Address => "Connect to your TV",
            Self::Pair => "Pair your TV",
            Self::Input => "Which input is your PC?",
            Self::Wake => "Set up wake",
            Self::Display => "Link a Windows display",
            Self::Review => "Your TV is ready to save",
        }
    }
}
#[derive(Clone)]
pub struct Setup {
    pub step: Step,
    pub inputs_loaded: bool,
    pub input_chosen: bool,
    pub manual_mac: String,
    pub manual_mac_enabled: bool,
    pub network_warning: Option<String>,
}
impl Default for Setup {
    fn default() -> Self {
        Self {
            step: Step::Brand,
            inputs_loaded: false,
            input_chosen: false,
            manual_mac: String::new(),
            manual_mac_enabled: false,
            network_warning: None,
        }
    }
}
impl Setup {
    pub fn invalidate_connection(&mut self, tv: &mut Tv) {
        tv.credential = None;
        tv.samsung = Default::default();
        tv.android = Default::default();
        tv.inputs.clear();
        tv.pc_input = None;
        tv.network = Default::default();
        self.inputs_loaded = false;
        self.input_chosen = false;
        self.manual_mac.clear();
        self.manual_mac_enabled = false;
        self.network_warning = None;
    }
    pub fn valid_step(&self, tv: &Tv) -> bool {
        match self.step {
            Step::Brand => true,
            Step::Name => !tv.name.trim().is_empty() && tv.name.trim().len() <= 160,
            Step::Address => {
                let mut tv = tv.clone();
                tv.address = tv.address.trim().into();
                tv.validate().is_ok()
            }
            Step::Pair => tv.credential.is_some() && self.inputs_loaded,
            Step::Input => {
                (tv.brand != crate::tvs::Brand::Lg || self.input_chosen)
                    && (tv.pc_input.is_none()
                        || tv.inputs.iter().any(|i| Some(i) == tv.pc_input.as_ref()))
            }
            Step::Wake => {
                !self.manual_mac_enabled
                    || self.manual_mac.trim().is_empty()
                    || self
                        .manual_mac
                        .parse::<crate::tv_network::MacAddress>()
                        .is_ok()
            }
            Step::Display => true,
            Step::Review => {
                tv.credential.is_some()
                    && self.inputs_loaded
                    && (tv.brand != crate::tvs::Brand::Lg || self.input_chosen)
                    && tv.validate().is_ok()
            }
        }
    }
    pub fn advance(&mut self, tv: &mut Tv) -> Result<(), String> {
        if !self.valid_step(tv) {
            return Err("Complete this step before continuing.".into());
        }
        self.step = match self.step {
            Step::Brand => Step::Name,
            Step::Name => {
                tv.name = tv.name.trim().into();
                Step::Address
            }
            Step::Address => {
                tv.address = tv.address.trim().into();
                Step::Pair
            }
            Step::Pair => Step::Input,
            Step::Input => Step::Wake,
            Step::Wake => {
                tv.network.manual = if !self.manual_mac_enabled || self.manual_mac.trim().is_empty()
                {
                    None
                } else {
                    Some(self.manual_mac.parse()?)
                };
                Step::Display
            }
            Step::Display => Step::Review,
            Step::Review => Step::Review,
        };
        Ok(())
    }
    pub fn back(&mut self) {
        self.step = match self.step {
            Step::Brand => Step::Brand,
            Step::Name => Step::Brand,
            Step::Address => Step::Name,
            Step::Pair => Step::Address,
            Step::Input => Step::Pair,
            Step::Wake => Step::Input,
            Step::Display => Step::Wake,
            Step::Review => Step::Display,
        };
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn tv() -> Tv {
        Tv {
            id: "test".into(),
            name: "TV".into(),
            address: "192.0.2.50".into(),
            brand: Default::default(),
            samsung: Default::default(),
            android: Default::default(),
            connection: Default::default(),
            audio: None,
            credential: None,
            display: None,
            inputs: vec![],
            pc_input: None,
            network: Default::default(),
        }
    }
    #[test]
    fn setup_requires_pairing_inputs_and_explicit_pc_choice() {
        let mut s = Setup {
            step: Step::Name,
            ..Default::default()
        };
        let mut tv = tv();
        s.advance(&mut tv).unwrap();
        s.advance(&mut tv).unwrap();
        assert!(s.advance(&mut tv).is_err());
        tv.credential = Some(vec![1]);
        assert!(!s.valid_step(&tv));
        s.inputs_loaded = true;
        s.advance(&mut tv).unwrap();
        assert!(!s.valid_step(&tv));
        s.input_chosen = true;
        s.advance(&mut tv).unwrap();
        s.manual_mac_enabled = true;
        s.manual_mac = "bad".into();
        assert!(!s.valid_step(&tv));
        s.manual_mac = "00:11:22:33:44:55".into();
        s.advance(&mut tv).unwrap();
        assert!(tv.network.manual.is_some());
        s.advance(&mut tv).unwrap();
        assert_eq!(s.step, Step::Review);
        assert!(s.valid_step(&tv));
        s.back();
        assert_eq!(s.step, Step::Display);
    }
    #[test]
    fn address_change_discards_authorization_and_learned_information() {
        let mut s = Setup {
            inputs_loaded: true,
            input_chosen: true,
            ..Default::default()
        };
        let mut tv = tv();
        tv.credential = Some(vec![1]);
        tv.network.wired = Some("00:11:22:33:44:55".parse().unwrap());
        s.invalidate_connection(&mut tv);
        assert!(tv.credential.is_none());
        assert!(tv.network.targets().is_empty());
        assert!(!s.inputs_loaded);
    }
}
