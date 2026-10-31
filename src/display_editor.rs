//! Pure draft operations. Nothing in this module changes Windows settings.
use crate::displays::{DesktopMode, Layout, RefreshRate};

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Mode {
    pub width: u32,
    pub height: u32,
    pub refresh: RefreshRate,
}
#[derive(Clone, Debug, Default)]
pub struct Options {
    pub modes: Vec<Mode>,
    pub warning: Option<String>,
    pub hdr: Option<bool>,
}
#[derive(Clone, Debug)]
pub struct Editor {
    pub options: Vec<Options>,
    pub remembered: Vec<Option<DesktopMode>>,
}
#[derive(Clone, Debug)]
pub enum Change {
    Enabled(bool),
    Mirror(Option<usize>),
    Hdr(bool),
    Primary,
    Resolution(usize),
    Refresh(usize),
    Rotation(u16),
    X(f64),
    Y(f64),
}
impl Options {
    pub fn resolutions(&self) -> Vec<(u32, u32)> {
        let mut values = self
            .modes
            .iter()
            .map(|m| (m.width, m.height))
            .collect::<Vec<_>>();
        values.sort_unstable_by(|a, b| b.cmp(a));
        values.dedup();
        values
    }
    pub fn rates(&self, width: u32, height: u32) -> Vec<RefreshRate> {
        let mut rates = self
            .modes
            .iter()
            .filter(|m| m.width == width && m.height == height)
            .map(|m| m.refresh.clone())
            .collect::<Vec<_>>();
        rates.sort_by(|a, b| {
            ((b.numerator as u64) * (a.denominator as u64))
                .cmp(&((a.numerator as u64) * (b.denominator as u64)))
        });
        rates.dedup_by(|a, b| {
            a.numerator as u64 * b.denominator as u64 == b.numerator as u64 * a.denominator as u64
        });
        rates
    }
}
impl Editor {
    pub fn new(layout: &Layout, options: Vec<Options>) -> Self {
        Self {
            options,
            remembered: layout.monitors.iter().map(|m| m.desktop.clone()).collect(),
        }
    }
    pub fn change(
        &mut self,
        layout: &mut Layout,
        index: usize,
        change: Change,
    ) -> Result<(), String> {
        if index >= layout.monitors.len() || index >= self.options.len() {
            return Err("Display is no longer available in this draft.".into());
        }
        let mut next = layout.clone();
        let mut editor = self.clone();
        let group = crate::display_mirroring::members(layout, index);
        let effective = if let Change::Resolution(choice) = change {
            let common = crate::display_mirroring::common_options(&editor, layout, index);
            let dimensions = common
                .resolutions()
                .get(choice)
                .copied()
                .ok_or("Choose a resolution reported by every mirrored display.")?;
            let actual = editor.options[index]
                .resolutions()
                .iter()
                .position(|r| *r == dimensions)
                .ok_or("Reported resolution missing.")?;
            Change::Resolution(actual)
        } else {
            change.clone()
        };
        if let Change::Mirror(target) = change {
            crate::display_mirroring::set(&editor, &mut next, index, target)?;
        } else {
            editor.edit(&mut next, index, effective)?;
            crate::display_mirroring::propagate(&editor, &mut next, index, &change, &group)?;
        }
        if crate::display_mirroring::primary_groups(&next) != 1 {
            return Err(
                "Choose exactly one primary desktop. Use Make primary to change it.".into(),
            );
        }
        *self = editor;
        *layout = next;
        Ok(())
    }
    fn edit(&mut self, layout: &mut Layout, index: usize, change: Change) -> Result<(), String> {
        let enable_source = if matches!(change, Change::Enabled(true)) {
            Some(crate::display_mirroring::unique_source(layout, index)?)
        } else {
            None
        };
        let has_primary_peer = crate::display_mirroring::members(layout, index).len() > 1;
        let m = layout
            .monitors
            .get_mut(index)
            .ok_or("Monitor is no longer in this draft.")?;
        let options = self
            .options
            .get(index)
            .ok_or("Monitor capabilities are unavailable.")?;
        if let Change::Enabled(on) = change {
            if on == m.enabled {
                return Ok(());
            }
            if on {
                if !m.connected
                    && !options
                        .warning
                        .as_deref()
                        .is_some_and(|w| w.starts_with("Previously detected"))
                {
                    return Err(
                        "Connect this monitor before enabling it in a custom layout.".into(),
                    );
                }

                let desktop = self.remembered[index]
                    .clone()
                    .or_else(|| {
                        options.modes.first().map(|mode| DesktopMode {
                            source_adapter_luid: m.adapter_luid,
                            source_id: 0x80000000 + index as u32,
                            source_name: "Custom desktop".into(),
                            x: 0,
                            y: 0,
                            width: mode.width,
                            height: mode.height,
                            source_width: mode.width,
                            source_height: mode.height,
                            refresh_rate: mode.refresh.clone(),
                            rotation_degrees: 0,
                        })
                    })
                    .ok_or("Windows did not report a usable mode for this monitor.")?;
                m.desktop = Some(desktop);
            } else {
                if m.desktop.as_ref().is_some_and(DesktopMode::primary) && !has_primary_peer {
                    return Err("Choose another primary monitor before disabling this one.".into());
                }
                self.remembered[index] = m.desktop.take();
            }
            m.enabled = on;
            if on {
                // Place newly enabled displays to the right, with their own source.
                let right = layout
                    .monitors
                    .iter()
                    .enumerate()
                    .filter(|(i, _)| *i != index)
                    .filter_map(|(_, m)| m.desktop.as_ref())
                    .map(|d| i64::from(d.x) + i64::from(d.width))
                    .max()
                    .unwrap_or(0);
                let d = layout.monitors[index].desktop.as_mut().unwrap();
                d.x = i32::try_from(right).map_err(|_| "Layout position is out of range.")?;
                d.y = 0;
                let source = enable_source.ok_or("No separate source was allocated.")?;
                d.source_adapter_luid = source.0;
                d.source_id = source.1;
            }
            return Ok(());
        }
        if let Change::Hdr(on) = change {
            if !m.enabled {
                return Err("Enable this monitor before changing HDR.".into());
            }
            if options.hdr.is_none() {
                return Err("Windows does not expose HDR controls for this monitor.".into());
            }
            m.hdr = Some(on);
            return Ok(());
        }
        let d = m
            .desktop
            .as_mut()
            .ok_or("Enable this monitor before editing it.")?;
        if matches!(change, Change::Primary) {
            let (dx, dy) = (d.x, d.y);
            // Translate every enabled display; retain relative arrangement.
            for m in &mut layout.monitors {
                if let Some(d) = &mut m.desktop {
                    d.x = d.x.checked_sub(dx).ok_or("Position is out of range.")?;
                    d.y = d.y.checked_sub(dy).ok_or("Position is out of range.")?;
                }
            }
            return Ok(());
        }
        let d = layout.monitors[index].desktop.as_mut().unwrap();
        match change {
            Change::Resolution(choice) => {
                let (w, h) = *options
                    .resolutions()
                    .get(choice)
                    .ok_or("Choose a reported resolution.")?;
                let rates = options.rates(w, h);
                let rate = rates
                    .iter()
                    .find(|r| **r == d.refresh_rate)
                    .or(rates.first())
                    .ok_or("No refresh rate is available.")?;
                d.source_width = w;
                d.source_height = h;
                d.refresh_rate = rate.clone();
                dimensions(d);
            }
            Change::Refresh(choice) => {
                d.refresh_rate = options
                    .rates(d.source_width, d.source_height)
                    .get(choice)
                    .ok_or("Choose a reported refresh rate.")?
                    .clone();
            }
            Change::Rotation(degrees) => {
                if ![0, 90, 180, 270].contains(&degrees) {
                    return Err("Choose a supported orientation.".into());
                }
                d.rotation_degrees = degrees;
                dimensions(d);
            }
            Change::X(value) | Change::Y(value) => {
                if !value.is_finite()
                    || value.fract() != 0.0
                    || !(-100000.0..=100000.0).contains(&value)
                {
                    return Err("Positions must be whole pixels between -100000 and 100000.".into());
                }
                if d.primary() {
                    return Err(
                        "The primary display stays at 0, 0. Choose another primary to move it."
                            .into(),
                    );
                }
                if matches!(change, Change::X(_)) {
                    d.x = value as i32
                } else {
                    d.y = value as i32
                }
            }
            _ => {}
        }
        Ok(())
    }
    pub fn validate(&self, layout: &Layout) -> Result<(), String> {
        layout.validate()?;
        let active = layout
            .monitors
            .iter()
            .filter_map(|m| m.desktop.as_ref())
            .collect::<Vec<_>>();
        if crate::display_mirroring::primary_groups(layout) != 1 {
            return Err("Choose exactly one primary monitor.".into());
        }
        for (i, a) in active.iter().enumerate() {
            for b in active.iter().skip(i + 1) {
                if (a.source_adapter_luid, a.source_id) == (b.source_adapter_luid, b.source_id) {
                    if (a.x, a.y, a.width, a.height, a.source_width, a.source_height)
                        != (b.x, b.y, b.width, b.height, b.source_width, b.source_height)
                    {
                        return Err(
                            "Mirrored displays must share a position and desktop resolution."
                                .into(),
                        );
                    }
                    continue;
                }
                if i64::from(a.x) < i64::from(b.x) + i64::from(b.width)
                    && i64::from(b.x) < i64::from(a.x) + i64::from(a.width)
                    && i64::from(a.y) < i64::from(b.y) + i64::from(b.height)
                    && i64::from(b.y) < i64::from(a.y) + i64::from(a.height)
                {
                    return Err("Extended monitors overlap. Adjust their horizontal and vertical positions before saving.".into());
                }
            }
        }
        Ok(())
    }
}
fn dimensions(d: &mut DesktopMode) {
    (d.width, d.height) = if matches!(d.rotation_degrees, 90 | 270) {
        (d.source_height, d.source_width)
    } else {
        (d.source_width, d.source_height)
    };
}
#[cfg(test)]
mod tests {
    use super::*;
    fn setup() -> (Layout, Editor) {
        let mut layout = crate::displays::tests::layout();
        let mut second = layout.monitors[0].clone();
        second.target_id = 8;
        second.device_path = Some("second".into());
        let d = second.desktop.as_mut().unwrap();
        d.source_id = 1;
        d.x = 2560;
        layout.monitors.push(second);
        let options = Options {
            modes: vec![
                Mode {
                    width: 2560,
                    height: 1440,
                    refresh: RefreshRate {
                        numerator: 60000,
                        denominator: 1001,
                    },
                },
                Mode {
                    width: 1920,
                    height: 1080,
                    refresh: RefreshRate {
                        numerator: 60,
                        denominator: 1,
                    },
                },
            ],
            warning: None,
            hdr: None,
        };
        let editor = Editor::new(&layout, vec![options.clone(), options]);
        (layout, editor)
    }
    #[test]
    fn primary_translation_and_disable_are_safe() {
        let (mut l, mut e) = setup();
        assert!(e.change(&mut l, 0, Change::Enabled(false)).is_err());
        e.change(&mut l, 1, Change::Primary).unwrap();
        assert_eq!(l.monitors[0].desktop.as_ref().unwrap().x, -2560);
        assert!(l.monitors[1].desktop.as_ref().unwrap().primary());
        e.change(&mut l, 0, Change::Enabled(false)).unwrap();
        e.validate(&l).unwrap();
        e.change(&mut l, 0, Change::Enabled(true)).unwrap();
        assert_eq!(l.monitors[0].desktop.as_ref().unwrap().x, 2560);
    }
    #[test]
    fn second_primary_and_shared_clone_edits_do_not_corrupt_the_draft() {
        let (mut layout, mut editor) = setup();
        let before = layout.clone();
        assert!(editor.change(&mut layout, 1, Change::X(0.0)).is_err());
        assert_eq!(layout, before);
        let source = layout.monitors[0].desktop.as_ref().unwrap().source_id;
        layout.monitors[1].desktop.as_mut().unwrap().source_id = source;
        let before = layout.clone();
        assert!(editor.change(&mut layout, 1, Change::Rotation(90)).is_err());
        assert_eq!(layout, before);
    }
    #[test]
    fn dependent_modes_rotation_and_bad_edits() {
        let (mut l, mut e) = setup();
        e.change(&mut l, 1, Change::Resolution(1)).unwrap();
        assert_eq!(
            l.monitors[1]
                .desktop
                .as_ref()
                .unwrap()
                .refresh_rate
                .numerator,
            60
        );
        e.change(&mut l, 1, Change::Rotation(90)).unwrap();
        let d = l.monitors[1].desktop.as_ref().unwrap();
        assert_eq!(
            (d.width, d.height, d.source_width, d.source_height),
            (1080, 1920, 1920, 1080)
        );
        let before = l.clone();
        assert!(e.change(&mut l, 1, Change::X(f64::NAN)).is_err());
        assert_eq!(l, before);
        e.change(&mut l, 1, Change::X(100.0)).unwrap();
        assert!(e.validate(&l).is_err());
    }
}

/// Map geometry is held fixed during a drag to avoid jumping as bounds change.
#[derive(Clone, Copy, Debug)]
pub struct MapTransform {
    pub scale: f64,
    pub left: f64,
    pub top: f64,
}
impl MapTransform {
    pub fn new(layout: &Layout) -> Self {
        let active = layout
            .monitors
            .iter()
            .filter_map(|m| m.desktop.as_ref())
            .collect::<Vec<_>>();
        let left = active.iter().map(|d| d.x as f64).fold(0.0, f64::min);
        let top = active.iter().map(|d| d.y as f64).fold(0.0, f64::min);
        let right = active
            .iter()
            .map(|d| d.x as f64 + d.width as f64)
            .fold(1.0, f64::max);
        let bottom = active
            .iter()
            .map(|d| d.y as f64 + d.height as f64)
            .fold(1.0, f64::max);
        let scale = (290.0 / (right - left)).min(100.0 / (bottom - top));
        Self {
            scale,
            left: (400.0 - (right - left) * scale) / 2.0 - left * scale,
            top: (180.0 - (bottom - top) * scale) / 2.0 - top * scale,
        }
    }
}
#[derive(Clone, Debug)]
pub struct Drag {
    pub index: usize,
    pub before: Layout,
    pub map: MapTransform,
    pub pointer_x: f64,
    pub pointer_y: f64,
}
impl Drag {
    pub fn begin(layout: &Layout, index: usize, x: f64, y: f64) -> Result<Self, String> {
        let _d = layout
            .monitors
            .get(index)
            .and_then(|m| m.desktop.as_ref())
            .ok_or("Choose an enabled display.")?;
        Ok(Self {
            index,
            before: layout.clone(),
            map: MapTransform::new(layout),
            pointer_x: x,
            pointer_y: y,
        })
    }
    pub fn moved(&self, x: f64, y: f64) -> Result<Layout, String> {
        if !x.is_finite() || !y.is_finite() {
            return Err("Invalid pointer position.".into());
        }
        let mut layout = self.before.clone();
        let d = layout.monitors[self.index].desktop.as_mut().unwrap();
        let px = (self.map.left + d.x as f64 * self.map.scale + x - self.pointer_x)
            .clamp(6.0, (394.0 - d.width as f64 * self.map.scale).max(6.0));
        let py = (self.map.top + d.y as f64 * self.map.scale + y - self.pointer_y)
            .clamp(6.0, (174.0 - d.height as f64 * self.map.scale).max(6.0));
        d.x = ((px - self.map.left) / self.map.scale).round() as i32;
        d.y = ((py - self.map.top) / self.map.scale).round() as i32;
        let (dx, dy) = (d.x, d.y);
        for i in crate::display_mirroring::members(&self.before, self.index) {
            let d = layout.monitors[i].desktop.as_mut().unwrap();
            d.x = dx;
            d.y = dy;
        }
        Ok(layout)
    }
    pub fn finish(&self, layout: &Layout) -> Result<Layout, String> {
        let mut next = layout.clone();
        let d = next.monitors[self.index]
            .desktop
            .as_ref()
            .ok_or("Dragged display became unavailable.")?
            .clone();
        let others = next
            .monitors
            .iter()
            .enumerate()
            .filter(|(i, _)| *i != self.index)
            .filter_map(|(_, m)| m.desktop.as_ref())
            .filter(|other| {
                crate::display_mirroring::key(other) != crate::display_mirroring::key(&d)
            })
            .collect::<Vec<_>>();
        let mut x = d.x;
        let mut y = d.y;
        let threshold = (8.0 / self.map.scale).ceil() as i64;
        if let Some(v) = others
            .iter()
            .flat_map(|b| {
                [
                    b.x as i64 - d.width as i64,
                    b.x as i64 + b.width as i64,
                    b.x as i64,
                ]
            })
            .min_by_key(|v| (*v - d.x as i64).abs())
            && (v - d.x as i64).abs() <= threshold
        {
            x = i32::try_from(v).map_err(|_| "Position is out of range.")?
        }
        if let Some(v) = others
            .iter()
            .flat_map(|b| {
                [
                    b.y as i64 - d.height as i64,
                    b.y as i64 + b.height as i64,
                    b.y as i64,
                ]
            })
            .min_by_key(|v| (*v - d.y as i64).abs())
            && (v - d.y as i64).abs() <= threshold
        {
            y = i32::try_from(v).map_err(|_| "Position is out of range.")?
        }
        let valid = |x: i32, y: i32| {
            !others.iter().any(|b| {
                (x as i64) < b.x as i64 + b.width as i64
                    && (b.x as i64) < x as i64 + d.width as i64
                    && (y as i64) < b.y as i64 + b.height as i64
                    && (b.y as i64) < y as i64 + d.height as i64
            })
        };
        if !valid(x, y) {
            let candidates = others
                .iter()
                .flat_map(|b| {
                    [
                        (b.x as i64 - d.width as i64, y as i64),
                        (b.x as i64 + b.width as i64, y as i64),
                        (x as i64, b.y as i64 - d.height as i64),
                        (x as i64, b.y as i64 + b.height as i64),
                    ]
                })
                .filter_map(|(x, y)| Some((i32::try_from(x).ok()?, i32::try_from(y).ok()?)))
                .filter(|(x, y)| valid(*x, *y))
                .min_by_key(|(x, y)| {
                    let dx = *x as i128 - d.x as i128;
                    let dy = *y as i128 - d.y as i128;
                    dx * dx + dy * dy
                });
            (x, y) = candidates
                .ok_or("No free position was found. Try moving this display further out.")?;
        }
        let moved = next.monitors[self.index].desktop.as_mut().unwrap();
        moved.x = x;
        moved.y = y;
        for i in crate::display_mirroring::members(&self.before, self.index) {
            let d = next.monitors[i].desktop.as_mut().unwrap();
            d.x = x;
            d.y = y;
        }
        let primary = self
            .before
            .monitors
            .iter()
            .position(|m| m.desktop.as_ref().is_some_and(DesktopMode::primary))
            .ok_or("The primary display is missing.")?;
        let p = next.monitors[primary].desktop.as_ref().unwrap();
        let (dx, dy) = (p.x, p.y);
        for m in &mut next.monitors {
            if let Some(d) = &mut m.desktop {
                d.x = d.x.checked_sub(dx).ok_or("Position is out of range.")?;
                d.y = d.y.checked_sub(dy).ok_or("Position is out of range.")?;
                if d.x.unsigned_abs() > 100000 || d.y.unsigned_abs() > 100000 {
                    return Err("Keep the display layout within the supported area.".into());
                }
            }
        }
        next.validate()?;
        Ok(next)
    }
}
#[cfg(test)]
mod drag_tests {
    use super::*;
    fn layout() -> Layout {
        let mut l = crate::displays::tests::layout();
        let mut b = l.monitors[0].clone();
        b.target_id = 8;
        b.device_path = Some("second".into());
        let d = b.desktop.as_mut().unwrap();
        d.source_id = 1;
        d.x = 2560;
        l.monitors.push(b);
        l
    }
    #[test]
    fn dragging_primary_preserves_primary_and_snaps_overlaps() {
        let l = layout();
        let drag = Drag::begin(&l, 0, 100.0, 100.0).unwrap();
        let moved = drag.moved(130.0, 140.0).unwrap();
        let finished = drag.finish(&moved).unwrap();
        assert!(finished.monitors[0].desktop.as_ref().unwrap().primary());
        let editor = Editor::new(&finished, vec![Options::default(); 2]);
        editor.validate(&finished).unwrap();
        assert_eq!(l.monitors[1].desktop.as_ref().unwrap().x, 2560);
    }
    #[test]
    fn secondary_drag_snaps_to_edges_and_rejects_invalid_input() {
        let l = layout();
        let drag = Drag::begin(&l, 1, 100.0, 100.0).unwrap();
        assert!(drag.moved(f64::NAN, 0.0).is_err());
        let mut moved = l.clone();
        let d = moved.monitors[1].desktop.as_mut().unwrap();
        d.x = -2500;
        d.y = 12;
        let result = drag.finish(&moved).unwrap();
        let d = result.monitors[1].desktop.as_ref().unwrap();
        assert_eq!((d.x, d.y), (-2560, 0));
        assert_eq!(l.monitors[1].desktop.as_ref().unwrap().x, 2560);
    }
}
