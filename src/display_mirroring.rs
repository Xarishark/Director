//! Draft-only CCD source grouping. No device setters or guessed modes.
use crate::{
    display_editor::{Editor, Options},
    displays::{DesktopMode, Layout},
};
pub fn key(d: &DesktopMode) -> (u64, u32) {
    (d.source_adapter_luid, d.source_id)
}
pub fn members(layout: &Layout, index: usize) -> Vec<usize> {
    let Some(d) = layout.monitors.get(index).and_then(|m| m.desktop.as_ref()) else {
        return vec![];
    };
    layout
        .monitors
        .iter()
        .enumerate()
        .filter_map(|(i, m)| m.desktop.as_ref().filter(|b| key(b) == key(d)).map(|_| i))
        .collect()
}
pub fn primary_groups(layout: &Layout) -> usize {
    layout
        .monitors
        .iter()
        .filter_map(|m| m.desktop.as_ref())
        .filter(|d| d.primary())
        .map(key)
        .collect::<std::collections::BTreeSet<_>>()
        .len()
}
pub fn common_options(editor: &Editor, layout: &Layout, index: usize) -> Options {
    let mut option = editor.options.get(index).cloned().unwrap_or_default();
    let group = members(layout, index);
    option.modes.retain(|mode| {
        group.iter().all(|i| {
            editor.options[*i]
                .modes
                .iter()
                .any(|m| m.width == mode.width && m.height == mode.height)
        })
    });
    option
}
pub(crate) fn unique_source(layout: &Layout, index: usize) -> Result<(u64, u32), String> {
    let adapter = layout.monitors[index].adapter_luid;
    (0x80000000..0x80010000)
        .find(|id| {
            !layout
                .monitors
                .iter()
                .filter_map(|m| m.desktop.as_ref())
                .any(|d| key(d) == (adapter, *id))
        })
        .map(|id| (adapter, id))
        .ok_or("No unused desktop source is available.".into())
}
fn extent(d: &mut DesktopMode) {
    (d.width, d.height) = if matches!(d.rotation_degrees, 90 | 270) {
        (d.source_height, d.source_width)
    } else {
        (d.source_width, d.source_height)
    };
}
pub fn set(
    editor: &Editor,
    layout: &mut Layout,
    index: usize,
    target: Option<usize>,
) -> Result<(), String> {
    let old = layout
        .monitors
        .get(index)
        .and_then(|m| m.desktop.as_ref())
        .cloned()
        .ok_or("Enable this display before choosing a mirror layout.")?;
    let original = members(layout, index);
    let Some(target) = target else {
        if original.len() < 2 {
            return Ok(());
        }
        let source = unique_source(layout, index)?;
        let right = layout
            .monitors
            .iter()
            .filter_map(|m| m.desktop.as_ref())
            .map(|d| i64::from(d.x) + i64::from(d.width))
            .max()
            .unwrap_or(0);
        let d = layout.monitors[index].desktop.as_mut().unwrap();
        d.source_adapter_luid = source.0;
        d.source_id = source.1;
        d.source_name = "Separate desktop".into();
        d.x = i32::try_from(right).map_err(|_| "Display position is out of range.")?;
        d.y = 0;
        return Ok(());
    };
    if target == index {
        return Err("Choose another display to mirror.".into());
    }
    let anchor = layout
        .monitors
        .get(target)
        .and_then(|m| m.desktop.as_ref())
        .cloned()
        .ok_or("Choose an enabled display to mirror.")?;
    if key(&old) == key(&anchor) {
        return Ok(());
    }
    let mut group = members(layout, target);
    group.push(index);
    let mut resolutions = editor.options[target].resolutions();
    resolutions.retain(|(w, h)| {
        group.iter().all(|i| {
            editor.options[*i]
                .modes
                .iter()
                .any(|m| m.width == *w && m.height == *h)
        })
    });
    let (w,h)=resolutions.iter().find(|r|**r==(anchor.source_width,anchor.source_height)).copied()
  .or_else(||resolutions.iter().max_by_key(|(w,h)|u64::from(*w)*u64::from(*h)).copied())
  .ok_or("These displays have no reported resolution in common. Detect their options before mirroring.")?;
    for &i in &group {
        let d = layout.monitors[i].desktop.as_mut().unwrap();
        let rates = editor.options[i].rates(w, h);
        d.refresh_rate = rates
            .iter()
            .find(|r| crate::display_apply::same_rate(r, &d.refresh_rate))
            .or(rates.first())
            .cloned()
            .ok_or("No reported refresh rate is available for this mirror layout.")?;
        d.source_adapter_luid = anchor.source_adapter_luid;
        d.source_id = anchor.source_id;
        d.source_name = anchor.source_name.clone();
        d.x = anchor.x;
        d.y = anchor.y;
        d.rotation_degrees = anchor.rotation_degrees;
        d.source_width = w;
        d.source_height = h;
        extent(d);
    }
    // If the sole primary output joins another source, that new group becomes primary.
    if old.primary() && original.len() == 1 {
        for m in &mut layout.monitors {
            if let Some(d) = &mut m.desktop {
                d.x =
                    d.x.checked_sub(anchor.x)
                        .ok_or("Display position is out of range.")?;
                d.y =
                    d.y.checked_sub(anchor.y)
                        .ok_or("Display position is out of range.")?;
            }
        }
    }
    Ok(())
}
pub fn propagate(
    editor: &Editor,
    layout: &mut Layout,
    index: usize,
    change: &crate::display_editor::Change,
    group: &[usize],
) -> Result<(), String> {
    use crate::display_editor::Change;
    if !matches!(
        change,
        Change::Resolution(_) | Change::Rotation(_) | Change::X(_) | Change::Y(_)
    ) {
        return Ok(());
    }
    let selected = layout.monitors[index]
        .desktop
        .as_ref()
        .ok_or("Display mode missing.")?
        .clone();
    for &i in group.iter().filter(|i| **i != index) {
        let d = layout.monitors[i].desktop.as_mut().unwrap();
        if matches!(change, Change::Resolution(_)) {
            let rates = editor.options[i].rates(selected.source_width, selected.source_height);
            d.refresh_rate = rates
                .iter()
                .find(|r| crate::display_apply::same_rate(r, &d.refresh_rate))
                .or(rates.first())
                .cloned()
                .ok_or("This resolution is not reported by every mirrored display.")?;
            d.source_width = selected.source_width;
            d.source_height = selected.source_height;
        }
        if matches!(change, Change::Rotation(_)) {
            d.rotation_degrees = selected.rotation_degrees;
        }
        d.x = selected.x;
        d.y = selected.y;
        extent(d);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        display_editor::{Change, Drag, Mode},
        displays::{Monitor, RefreshRate},
    };
    pub(super) fn setup_for_sources() -> (Layout, Editor) {
        let mut layout = Layout { monitors: vec![] };
        for i in 0..3 {
            layout.monitors.push(Monitor {
                adapter_luid: 1,
                target_id: i as u32,
                device_path: Some(format!("screen-{i}")),
                name: format!("Screen {i}"),
                connected: true,
                enabled: true,
                hdr: Some(false),
                desktop: Some(DesktopMode {
                    source_adapter_luid: 1,
                    source_id: i as u32,
                    source_name: format!("DISPLAY{i}"),
                    x: i * 1920,
                    y: 0,
                    width: 1920,
                    height: 1080,
                    source_width: 1920,
                    source_height: 1080,
                    refresh_rate: RefreshRate {
                        numerator: if i == 1 { 120 } else { 60 },
                        denominator: 1,
                    },
                    rotation_degrees: 0,
                }),
            });
        }
        let options = (0..3)
            .map(|i| Options {
                modes: vec![
                    Mode {
                        width: 1920,
                        height: 1080,
                        refresh: RefreshRate {
                            numerator: if i == 1 { 120 } else { 60 },
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
                hdr: Some(false),
                warning: None,
            })
            .collect();
        let editor = Editor::new(&layout, options);
        (layout, editor)
    }
    #[test]
    fn join_edit_detach_and_disable_mirrors_keep_one_primary_source() {
        let (mut l, mut e) = setup_for_sources();
        e.change(&mut l, 1, Change::Mirror(Some(0))).unwrap();
        assert_eq!(members(&l, 0), vec![0, 1]);
        assert_eq!(primary_groups(&l), 1);
        e.validate(&l).unwrap();
        assert_eq!(
            l.monitors[1]
                .desktop
                .as_ref()
                .unwrap()
                .refresh_rate
                .numerator,
            120
        );
        e.change(&mut l, 0, Change::Resolution(1)).unwrap();
        for i in [0, 1] {
            assert_eq!(l.monitors[i].desktop.as_ref().unwrap().source_width, 1280);
        }
        e.change(&mut l, 1, Change::Rotation(90)).unwrap();
        for i in [0, 1] {
            assert_eq!(l.monitors[i].desktop.as_ref().unwrap().width, 720);
        }
        e.change(&mut l, 1, Change::Hdr(true)).unwrap();
        assert_eq!(l.monitors[0].hdr, Some(false));
        e.change(&mut l, 1, Change::Mirror(None)).unwrap();
        assert_eq!(members(&l, 0), vec![0]);
        e.validate(&l).unwrap();
        e.change(&mut l, 1, Change::Mirror(Some(0))).unwrap();
        e.change(&mut l, 0, Change::Enabled(false)).unwrap();
        assert_eq!(primary_groups(&l), 1);
        e.validate(&l).unwrap();
        assert!(e.change(&mut l, 1, Change::Enabled(false)).is_err());
    }
    #[test]
    fn incompatible_join_is_atomic_and_primary_can_join_another_group() {
        let (mut l, mut e) = setup_for_sources();
        let before = l.clone();
        let remembered = e.remembered.clone();
        e.options[1].modes = vec![Mode {
            width: 800,
            height: 600,
            refresh: RefreshRate {
                numerator: 60,
                denominator: 1,
            },
        }];
        assert!(e.change(&mut l, 1, Change::Mirror(Some(0))).is_err());
        assert_eq!(l, before);
        assert_eq!(e.remembered, remembered);
        let (mut l, mut e) = setup_for_sources();
        e.change(&mut l, 0, Change::Mirror(Some(1))).unwrap();
        assert_eq!(primary_groups(&l), 1);
        assert_eq!(members(&l, 1), vec![0, 1]);
        e.validate(&l).unwrap();
        assert!(e.change(&mut l, 0, Change::Mirror(Some(50))).is_err());
    }
    #[test]
    fn group_drag_moves_every_output_and_detach_allocates_a_distinct_source() {
        let (mut l, mut e) = setup_for_sources();
        e.change(&mut l, 2, Change::Mirror(Some(1))).unwrap();
        let drag = Drag::begin(
            &l,
            1,
            100.0,
            100.0,
            crate::display_editor::MapTransform::new(&l, 400.0, 180.0),
        )
        .unwrap();
        let moved = drag.moved(110.0, 100.0).unwrap();
        let done = drag.finish(&moved).unwrap();
        assert_eq!(
            done.monitors[1].desktop.as_ref().unwrap().x,
            done.monitors[2].desktop.as_ref().unwrap().x
        );
        let l = &mut l;
        e.change(l, 2, Change::Mirror(None)).unwrap();
        assert_ne!(
            key(l.monitors[1].desktop.as_ref().unwrap()),
            key(l.monitors[2].desktop.as_ref().unwrap())
        );
    }
}

#[cfg(test)]
#[test]
fn enabling_an_output_cannot_reuse_a_detached_groups_logical_source() {
    use crate::display_editor::Change;
    let (mut layout, mut editor) = tests::setup_for_sources();
    editor
        .change(&mut layout, 1, Change::Mirror(Some(0)))
        .unwrap();
    editor
        .change(&mut layout, 2, Change::Mirror(Some(0)))
        .unwrap();
    editor.change(&mut layout, 1, Change::Mirror(None)).unwrap();
    editor
        .change(&mut layout, 0, Change::Enabled(false))
        .unwrap();
    editor
        .change(&mut layout, 0, Change::Enabled(true))
        .unwrap();
    assert_eq!(members(&layout, 0), vec![0]);
    assert_eq!(members(&layout, 1), vec![1]);
    assert_eq!(members(&layout, 2), vec![2]);
    editor.validate(&layout).unwrap();
}
