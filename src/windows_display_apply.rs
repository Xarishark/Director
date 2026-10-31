//! Resolve device paths against fresh CCD routes; never reuse stored adapter IDs.
use crate::{
    display_apply::{self, Backend},
    displays::{DesktopMode, Layout},
    windows_displays::{capture, luid, query_flags, target_name},
};
use std::collections::{BTreeMap, BTreeSet};
use windows::Win32::{
    Devices::Display::*,
    Foundation::LUID,
    Graphics::Gdi::{DISPLAYCONFIG_PATH_ACTIVE, DISPLAYCONFIG_PATH_SUPPORT_VIRTUAL_MODE},
};
type Key = (u64, u32);
#[derive(Clone)]
struct Config {
    paths: Vec<DISPLAYCONFIG_PATH_INFO>,
    modes: Vec<DISPLAYCONFIG_MODE_INFO>,
}
#[derive(Clone)]
pub struct Snapshot {
    config: Config,
    layout: Layout,
}
pub struct Plan {
    config: Config,
    wanted: Layout,
    identities: Vec<(Key, String)>,
    allow_changes: bool,
    cold_hdr: Vec<String>,
}
#[derive(Default)]
pub struct WindowsDisplays {
    expected: Option<Layout>,
    rollback_hdr: Vec<(String, bool)>,
}
fn same_config(a: &Config, b: &Config) -> bool {
    if a.paths.len() != b.paths.len() || a.modes.len() != b.modes.len() {
        return false;
    }
    for (left, right) in a.paths.iter().zip(&b.paths) {
        if source_key(left) != source_key(right)
            || target_key(left) != target_key(right)
            || left.flags != right.flags
            || left.targetInfo.rotation != right.targetInfo.rotation
            || left.targetInfo.scaling != right.targetInfo.scaling
            || left.targetInfo.scanLineOrdering != right.targetInfo.scanLineOrdering
            || left.targetInfo.refreshRate != right.targetInfo.refreshRate
            || unsafe {
                left.sourceInfo.Anonymous.modeInfoIdx != right.sourceInfo.Anonymous.modeInfoIdx
                    || left.targetInfo.Anonymous.modeInfoIdx
                        != right.targetInfo.Anonymous.modeInfoIdx
            }
        {
            return false;
        }
    }
    for (left, right) in a.modes.iter().zip(&b.modes) {
        if left.infoType != right.infoType
            || left.id != right.id
            || luid(left.adapterId) != luid(right.adapterId)
        {
            return false;
        }
        if left.infoType == DISPLAYCONFIG_MODE_INFO_TYPE_SOURCE {
            if unsafe { left.Anonymous.sourceMode != right.Anonymous.sourceMode } {
                return false;
            }
        } else if left.infoType == DISPLAYCONFIG_MODE_INFO_TYPE_TARGET {
            let l = unsafe { left.Anonymous.targetMode.targetVideoSignalInfo };
            let r = unsafe { right.Anonymous.targetMode.targetVideoSignalInfo };
            if l.pixelRate != r.pixelRate
                || l.hSyncFreq != r.hSyncFreq
                || l.vSyncFreq != r.vSyncFreq
                || l.activeSize != r.activeSize
                || l.totalSize != r.totalSize
                || l.scanLineOrdering != r.scanLineOrdering
                || unsafe { l.Anonymous.videoStandard != r.Anonymous.videoStandard }
            {
                return false;
            }
        } else {
            return false;
        }
    }
    true
}
fn read_active() -> Result<Config, String> {
    let (paths, modes) = query_flags(QDC_ONLY_ACTIVE_PATHS)?;
    if paths.is_empty() {
        return Err("Windows has no active desktop to snapshot.".into());
    }
    if paths
        .iter()
        .any(|p| p.flags & DISPLAYCONFIG_PATH_SUPPORT_VIRTUAL_MODE != 0)
    {
        return Err("This virtual-mode display topology is not supported for Apply yet.".into());
    }
    Ok(Config { paths, modes })
}
fn source_key(p: &DISPLAYCONFIG_PATH_INFO) -> Key {
    (luid(p.sourceInfo.adapterId), p.sourceInfo.id)
}
fn target_key(p: &DISPLAYCONFIG_PATH_INFO) -> Key {
    (luid(p.targetInfo.adapterId), p.targetInfo.id)
}
fn native_luid(value: u64) -> LUID {
    LUID {
        LowPart: value as u32,
        HighPart: (value >> 32) as u32 as i32,
    }
}
fn augment(
    index: usize,
    options: &[Vec<Key>],
    owners: &mut BTreeMap<Key, usize>,
    seen: &mut BTreeSet<Key>,
) -> bool {
    let mut candidates = options[index].clone();
    candidates.sort_by_key(|source| owners.contains_key(source));
    for source in candidates {
        if !seen.insert(source) {
            continue;
        }
        let previous = owners.get(&source).copied();
        if previous.is_none_or(|other| augment(other, options, owners, seen)) {
            owners.insert(source, index);
            return true;
        }
    }
    false
}
fn source_modes_agree(a: &DesktopMode, b: &DesktopMode) -> bool {
    (a.x, a.y, a.width, a.height, a.source_width, a.source_height)
        == (b.x, b.y, b.width, b.height, b.source_width, b.source_height)
}
fn build(
    wanted: &Layout,
    before: &Layout,
    paths: &[DISPLAYCONFIG_PATH_INFO],
    modes: &[DISPLAYCONFIG_MODE_INFO],
) -> Result<Plan, String> {
    wanted.validate()?;
    for m in wanted
        .monitors
        .iter()
        .filter(|m| m.enabled && m.hdr.is_some())
    {
        let id = display_apply::identity(m)?;
        let original = before
            .monitors
            .iter()
            .find(|b| display_apply::identity(b).is_ok_and(|v| v == id));
        if original.is_none_or(|b| b.enabled && b.hdr.is_none()) {
            return Err(format!(
                "HDR for “{}” cannot be safely read and restored. Connect/enable the display and reopen the editor.",
                m.name
            ));
        }
    }
    if wanted.monitors.len() > 64 {
        return Err("This layout has too many monitors to apply safely.".into());
    }
    let mut current = BTreeMap::new();
    for m in &before.monitors {
        match display_apply::identity(m) {
            Ok(id) => {
                if current.insert(id, m).is_some() {
                    return Err(
                        "Current monitor identities are ambiguous. Reconnect or recapture them."
                            .into(),
                    );
                }
            }
            Err(e) if m.enabled => return Err(e),
            Err(_) => {}
        }
    }
    let mut saved = BTreeSet::new();
    for m in &wanted.monitors {
        if let Ok(id) = display_apply::identity(m) {
            if !saved.insert(id) {
                return Err("The preset has duplicate monitor identities.".into());
            }
        } else if m.enabled {
            return Err(display_apply::identity(m).unwrap_err());
        }
    }
    for m in before.monitors.iter().filter(|m| m.enabled) {
        if !saved.contains(&display_apply::identity(m)?) {
            return Err(format!(
                "Active monitor “{}” was not in this preset. Recapture the layout before applying it.",
                m.name
            ));
        }
    }
    let mut groups: BTreeMap<Key, Vec<(crate::displays::Monitor, Key)>> = BTreeMap::new();
    let mut identities = vec![];
    for m in wanted.monitors.iter().filter(|m| m.enabled) {
        let identity = display_apply::identity(m)?;
        let now = current.get(&identity).copied().ok_or_else(|| {
            format!(
                "“{}” is missing. Connect it before applying this preset.",
                m.name
            )
        })?;
        if !now.connected {
            return Err(format!(
                "“{}” is unavailable. Turn it on and connect it before applying.",
                m.name
            ));
        }
        let target = (now.adapter_luid, now.target_id);
        let desktop = m.desktop.as_ref().ok_or("Saved display mode missing.")?;
        groups
            .entry((desktop.source_adapter_luid, desktop.source_id))
            .or_default()
            .push((m.clone(), target));
        identities.push((target, identity));
    }
    let primary = groups
        .values()
        .filter(|members| members[0].0.desktop.as_ref().unwrap().primary())
        .count();
    if primary != 1 {
        return Err("The preset must have exactly one primary desktop source.".into());
    }
    let groups = groups.into_values().collect::<Vec<_>>();
    let mut options = vec![];
    for members in &groups {
        let first = members[0].0.desktop.as_ref().unwrap();
        if members
            .iter()
            .any(|(m, _)| !source_modes_agree(first, m.desktop.as_ref().unwrap()))
        {
            return Err("Mirrored monitors have inconsistent saved source modes.".into());
        }
        let mut available: Option<BTreeSet<Key>> = None;
        for (_, target) in members {
            let candidates = paths
                .iter()
                .filter(|p| {
                    target_key(p) == *target
                        && p.targetInfo.targetAvailable.as_bool()
                        && p.flags & DISPLAYCONFIG_PATH_SUPPORT_VIRTUAL_MODE == 0
                })
                .map(source_key)
                .collect::<BTreeSet<_>>();
            available = Some(match available {
                None => candidates,
                Some(existing) => existing.intersection(&candidates).copied().collect(),
            });
        }
        let mut candidates = available
            .unwrap_or_default()
            .into_iter()
            .collect::<Vec<_>>();
        candidates.sort_by_key(|source| {
            (
                !paths.iter().any(|p| {
                    source_key(p) == *source
                        && p.flags & DISPLAYCONFIG_PATH_ACTIVE != 0
                        && members.iter().any(|(_, target)| target_key(p) == *target)
                }),
                *source,
            )
        });
        if candidates.is_empty() {
            return Err(
                "Windows has no compatible source route for the saved monitor grouping.".into(),
            );
        }
        options.push(candidates);
    }
    let mut owners = BTreeMap::new();
    for i in 0..options.len() {
        if !augment(i, &options, &mut owners, &mut BTreeSet::new()) {
            return Err("Windows cannot assign distinct sources to this display layout.".into());
        }
    }
    let assignments = owners
        .into_iter()
        .map(|(source, group)| (group, source))
        .collect::<BTreeMap<_, _>>();
    let mut config = Config {
        paths: vec![],
        modes: vec![],
    };
    for (i, members) in groups.iter().enumerate() {
        let assigned = assignments[&i];
        let d = members[0].0.desktop.as_ref().unwrap();
        let source_index = config.modes.len() as u32;
        let pixel = paths
            .iter()
            .find(|p| source_key(p) == assigned && p.flags & DISPLAYCONFIG_PATH_ACTIVE != 0)
            .and_then(|p| modes.get(unsafe { p.sourceInfo.Anonymous.modeInfoIdx } as usize))
            .filter(|m| m.infoType == DISPLAYCONFIG_MODE_INFO_TYPE_SOURCE)
            .map(|m| unsafe { m.Anonymous.sourceMode.pixelFormat })
            .unwrap_or(DISPLAYCONFIG_PIXELFORMAT_32BPP);
        let mut source = DISPLAYCONFIG_MODE_INFO {
            infoType: DISPLAYCONFIG_MODE_INFO_TYPE_SOURCE,
            id: assigned.1,
            adapterId: native_luid(assigned.0),
            ..Default::default()
        };
        source.Anonymous.sourceMode = DISPLAYCONFIG_SOURCE_MODE {
            width: d.source_width,
            height: d.source_height,
            pixelFormat: pixel,
            position: windows::Win32::Foundation::POINTL { x: d.x, y: d.y },
        };
        config.modes.push(source);
        for (monitor, target) in members {
            let mut path = *paths
                .iter()
                .find(|p| target_key(p) == *target && source_key(p) == assigned)
                .ok_or("A display route disappeared.")?;
            path.flags = DISPLAYCONFIG_PATH_ACTIVE;
            path.sourceInfo.statusFlags = 0;
            path.targetInfo.statusFlags = 0;
            path.sourceInfo.Anonymous.modeInfoIdx = source_index;
            path.targetInfo.Anonymous.modeInfoIdx = u32::MAX;
            let mode = monitor.desktop.as_ref().unwrap();
            path.targetInfo.rotation =
                DISPLAYCONFIG_ROTATION(i32::from(mode.rotation_degrees / 90) + 1);
            path.targetInfo.refreshRate = DISPLAYCONFIG_RATIONAL {
                Numerator: mode.refresh_rate.numerator,
                Denominator: mode.refresh_rate.denominator,
            };
            path.targetInfo.scanLineOrdering = DISPLAYCONFIG_SCANLINE_ORDERING_PROGRESSIVE;
            if path.targetInfo.scaling.0 == 0 {
                path.targetInfo.scaling = DISPLAYCONFIG_SCALING_PREFERRED;
            }
            // Preserve exact current target timings when only arrangement changes.
            if let Some(active) = paths
                .iter()
                .find(|p| target_key(p) == *target && p.flags & DISPLAYCONFIG_PATH_ACTIVE != 0)
            {
                let now = before
                    .monitors
                    .iter()
                    .find(|m| (m.adapter_luid, m.target_id) == *target)
                    .and_then(|m| m.desktop.as_ref());
                if now.is_some_and(|d| {
                    d.source_width == mode.source_width
                        && d.source_height == mode.source_height
                        && d.rotation_degrees == mode.rotation_degrees
                        && display_apply::same_rate(&d.refresh_rate, &mode.refresh_rate)
                }) && let Some(target_mode) = modes
                    .get(unsafe { active.targetInfo.Anonymous.modeInfoIdx } as usize)
                    .filter(|m| {
                        m.infoType == DISPLAYCONFIG_MODE_INFO_TYPE_TARGET
                            && m.id == target.1
                            && luid(m.adapterId) == target.0
                    })
                {
                    path.targetInfo.Anonymous.modeInfoIdx = config.modes.len() as u32;
                    config.modes.push(*target_mode);
                }
            }
            config.paths.push(path);
        }
    }
    // Primary paths first; mode indices remain valid when the path order changes.
    config.paths.sort_by_key(|p| {
        let index = unsafe { p.sourceInfo.Anonymous.modeInfoIdx } as usize;
        let mode = unsafe { config.modes[index].Anonymous.sourceMode };
        (mode.position.x != 0 || mode.position.y != 0, target_key(p))
    });
    Ok(Plan {
        config,
        wanted: wanted.clone(),
        identities,
        cold_hdr: wanted
            .monitors
            .iter()
            .filter(|m| m.enabled && m.hdr.is_some())
            .filter_map(|m| display_apply::identity(m).ok())
            .filter(|id| {
                before
                    .monitors
                    .iter()
                    .any(|b| !b.enabled && display_apply::identity(b).is_ok_and(|v| &v == id))
            })
            .collect(),
        allow_changes: false,
    })
}
fn set(config: &Config, flags: SET_DISPLAY_CONFIG_FLAGS) -> Result<(), String> {
    let status = unsafe {
        SetDisplayConfig(
            Some(&config.paths),
            Some(&config.modes),
            SDC_USE_SUPPLIED_DISPLAY_CONFIG | flags,
        )
    };
    if status == 0 {
        Ok(())
    } else {
        Err(format!(
            "Windows rejected the display configuration (error {status})."
        ))
    }
}
// A successful unsupported-HDR query proves SDR; an API failure proves nothing.
fn normalize_sdr(layout: &mut Layout, wanted: &Layout) {
    for actual in &mut layout.monitors {
        if actual.enabled
            && actual.hdr.is_none()
            && wanted.monitors.iter().any(|m| {
                m.enabled
                    && m.hdr == Some(false)
                    && display_apply::identity(m).ok() == display_apply::identity(actual).ok()
            })
            && matches!(
                crate::windows_hdr::state(actual.adapter_luid, actual.target_id),
                Ok(None)
            )
        {
            actual.hdr = Some(false);
        }
    }
}
fn capture_for(wanted: &Layout) -> Result<Layout, String> {
    let mut layout = capture()?;
    normalize_sdr(&mut layout, wanted);
    Ok(layout)
}
fn wait_hdrs(wanted: &Layout) -> Result<Layout, String> {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
    loop {
        let active = capture_for(wanted)?;
        let ready = wanted
            .monitors
            .iter()
            .filter(|m| m.enabled && m.hdr.is_some())
            .all(|m| {
                active.monitors.iter().any(|n| {
                    n.enabled
                        && n.hdr.is_some()
                        && display_apply::identity(n).ok() == display_apply::identity(m).ok()
                })
            });
        if ready {
            return Ok(active);
        }
        if std::time::Instant::now() >= deadline {
            return Err("Windows could not read the requested HDR state after enabling the layout. For mirrored displays, try HDR off or an extended desktop.".into());
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
}
fn set_hdrs(wanted: &Layout) -> Result<(), String> {
    let current = capture_for(wanted)?;
    let mut failures = vec![];
    for m in wanted
        .monitors
        .iter()
        .filter(|m| m.enabled && m.hdr.is_some())
    {
        let id = display_apply::identity(m)?;
        let desired = m.hdr.unwrap();
        let now = current
            .monitors
            .iter()
            .find(|n| n.enabled && display_apply::identity(n).is_ok_and(|v| v == id));
        match now {
            Some(n) if n.hdr == Some(desired) => {}
            Some(n) if n.hdr.is_some() => {
                if let Err(e) = crate::windows_hdr::set(n.adapter_luid, n.target_id, desired) {
                    failures.push(format!("{}: {e}", m.name));
                }
            }
            _ => failures.push(format!("HDR state for “{}” is unavailable.", m.name)),
        }
    }
    if failures.is_empty() {
        Ok(())
    } else {
        Err(failures.join(
            "
",
        ))
    }
}
impl Backend for WindowsDisplays {
    type Snapshot = Snapshot;
    type Plan = Plan;
    fn snapshot(&mut self) -> Result<(Layout, Snapshot), String> {
        let config = read_active()?;
        let layout = capture()?;
        let check = read_active()?;
        if !same_config(&config, &check) {
            return Err("The display configuration changed during preflight. Try again.".into());
        }
        set(&config, SDC_VALIDATE)?;
        self.rollback_hdr.clear();
        Ok((layout.clone(), Snapshot { config, layout }))
    }
    fn prepare(&mut self, wanted: &Layout, before: &Layout, _: &Snapshot) -> Result<Plan, String> {
        let (paths, modes) = query_flags(QDC_ALL_PATHS)?;
        let mut observed = before.clone();
        normalize_sdr(&mut observed, wanted);
        build(wanted, &observed, &paths, &modes)
    }
    fn validate(&mut self, plan: &mut Plan) -> Result<(), String> {
        let status = unsafe {
            SetDisplayConfig(
                Some(&plan.config.paths),
                Some(&plan.config.modes),
                SDC_USE_SUPPLIED_DISPLAY_CONFIG | SDC_VALIDATE,
            )
        };
        if status == 0 {
            return Ok(());
        }
        if status != 1610 {
            return Err(format!(
                "Windows cannot validate this display layout (error {status})."
            ));
        }
        // Best-mode completion is allowed only with exact post-apply verification.
        set(&plan.config, SDC_VALIDATE | SDC_ALLOW_CHANGES)?;
        plan.allow_changes = true;
        Ok(())
    }
    fn apply_temporary(&mut self, plan: &Plan) -> Result<(), String> {
        for (target, expected) in &plan.identities {
            let probe = DISPLAYCONFIG_PATH_INFO {
                targetInfo: DISPLAYCONFIG_PATH_TARGET_INFO {
                    adapterId: native_luid(target.0),
                    id: target.1,
                    ..Default::default()
                },
                ..Default::default()
            };
            if target_name(&probe)
                .1
                .as_ref()
                .map(|p| p.to_lowercase())
                .as_ref()
                != Some(expected)
            {
                return Err("A monitor identity changed before Apply. Capture again.".into());
            }
        }
        self.expected = Some(plan.wanted.clone());
        set(
            &plan.config,
            SDC_APPLY
                | if plan.allow_changes {
                    SDC_ALLOW_CHANGES
                } else {
                    SET_DISPLAY_CONFIG_FLAGS(0)
                },
        )?;
        // Cold displays can expose HDR only after their display path is active.
        // Capture the retained user preference before the first HDR write.
        let active = wait_hdrs(&plan.wanted)?;
        for id in &plan.cold_hdr {
            if !self.rollback_hdr.iter().any(|(saved, _)| saved == id) {
                let state = active
                    .monitors
                    .iter()
                    .find(|m| display_apply::identity(m).is_ok_and(|v| &v == id))
                    .and_then(|m| m.hdr)
                    .ok_or("HDR could not be read after enabling the display.")?;
                self.rollback_hdr.push((id.clone(), state));
            }
        }
        set_hdrs(&plan.wanted)
    }
    fn current(&mut self) -> Result<Layout, String> {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
        loop {
            let result = if let Some(expected) = &self.expected {
                capture_for(expected)
            } else {
                capture()
            };
            if std::time::Instant::now() >= deadline
                || result.as_ref().is_ok_and(|layout| {
                    self.expected
                        .as_ref()
                        .is_none_or(|expected| display_apply::verify(expected, layout).is_ok())
                })
            {
                return result;
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
    }
    fn commit(&mut self, plan: &Plan) -> Result<(), String> {
        display_apply::verify(&plan.wanted, &capture_for(&plan.wanted)?)?;
        let config = read_active()?;
        set(&config, SDC_APPLY | SDC_SAVE_TO_DATABASE)?;
        set_hdrs(&plan.wanted)?;
        Ok(())
    }
    fn restore(&mut self, snapshot: &Snapshot) -> Result<(), String> {
        self.expected = Some(snapshot.layout.clone());
        let cold_hdr = self.restore_cold_hdr();
        let topology = set(&snapshot.config, SDC_APPLY | SDC_SAVE_TO_DATABASE);
        let hdr = set_hdrs(&snapshot.layout);
        let hdr = cold_hdr.and(hdr);
        match (topology, hdr) {
            (Ok(()), Ok(())) => Ok(()),
            (Err(a), Err(b)) => Err(format!("{a}; HDR recovery: {b}")),
            (Err(e), _) | (_, Err(e)) => Err(e),
        }
    }
}
impl WindowsDisplays {
    fn restore_cold_hdr(&mut self) -> Result<(), String> {
        if self.rollback_hdr.is_empty() {
            return Ok(());
        }
        let mut active = capture()?;
        for m in &mut active.monitors {
            m.hdr = display_apply::identity(m).ok().and_then(|id| {
                self.rollback_hdr
                    .iter()
                    .find(|(saved, _)| *saved == id)
                    .map(|(_, on)| *on)
            });
        }
        set_hdrs(&active)?;
        self.rollback_hdr.clear();
        Ok(())
    }
    pub fn enable_for_detection(
        &mut self,
        path: &str,
        before: &Layout,
        snapshot: &Snapshot,
    ) -> Result<(), String> {
        let fresh = capture()?;
        let target = fresh
            .monitors
            .iter()
            .find(|m| {
                m.device_path
                    .as_deref()
                    .is_some_and(|p| p.eq_ignore_ascii_case(path))
            })
            .filter(|m| m.connected)
            .ok_or("Windows cannot see this display. Turn it on and select the PC input.")?
            .clone();
        let mut wanted = before.clone();
        if let Some(m) = wanted.monitors.iter_mut().find(|m| {
            m.device_path
                .as_deref()
                .is_some_and(|p| p.eq_ignore_ascii_case(path))
        }) {
            if !m.enabled {
                *m = target;
                m.enabled = false;
                m.desktop = None;
                m.hdr = None;
            }
        } else {
            let mut target = target;
            target.enabled = false;
            target.desktop = None;
            target.hdr = None;
            wanted.monitors.push(target);
        }
        let i = wanted
            .monitors
            .iter()
            .position(|m| {
                m.device_path
                    .as_deref()
                    .is_some_and(|p| p.eq_ignore_ascii_case(path))
            })
            .ok_or("The display disappeared.")?;
        let options = crate::windows_display_modes::discover_live(&wanted)?;
        let mut editor = crate::display_editor::Editor::new(&wanted, options);
        editor.change(&mut wanted, i, crate::display_editor::Change::Enabled(true))?;
        if let Some((w, h)) = crate::windows_display_modes::preferred_resolution(path)
            && let Some(d) = &mut wanted.monitors[i].desktop
        {
            d.source_width = w;
            d.source_height = h;
            (d.width, d.height) = if matches!(d.rotation_degrees, 90 | 270) {
                (h, w)
            } else {
                (w, h)
            };
        }
        wanted.monitors[i].hdr = None;
        let mut plan = self.prepare(&wanted, &fresh, snapshot)?;
        // Preserve supplied modes for existing screens; let Windows choose this target's timing.
        for (key, id) in &plan.identities {
            if id.eq_ignore_ascii_case(path)
                && let Some(p) = plan.config.paths.iter_mut().find(|p| target_key(p) == *key)
            {
                p.targetInfo.Anonymous.modeInfoIdx = u32::MAX;
                p.targetInfo.refreshRate = DISPLAYCONFIG_RATIONAL {
                    Numerator: 0,
                    Denominator: 0,
                };
                p.targetInfo.scanLineOrdering = DISPLAYCONFIG_SCANLINE_ORDERING_UNSPECIFIED;
            }
        }
        set(&plan.config, SDC_VALIDATE | SDC_ALLOW_CHANGES)?;
        plan.allow_changes = true;
        let result = self.apply_temporary(&plan);
        self.expected = None;
        result
    }
    pub fn restore_detection(&mut self, snapshot: &Snapshot) -> Result<(), String> {
        self.expected = Some(snapshot.layout.clone());
        let cold_hdr = self.restore_cold_hdr();
        let topology = set(&snapshot.config, SDC_APPLY);
        let hdr = set_hdrs(&snapshot.layout);
        let hdr = cold_hdr.and(hdr);
        match (topology, hdr) {
            (Ok(()), Ok(())) => Ok(()),
            (Err(a), Err(b)) => Err(format!("{a}; HDR recovery: {b}")),
            (Err(e), _) | (_, Err(e)) => Err(e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn source_assignment_can_reassign_instead_of_greedily_failing() {
        let a = (1, 0);
        let b = (1, 1);
        let options = vec![vec![a, b], vec![a]];
        let mut owners = BTreeMap::new();
        assert!(augment(0, &options, &mut owners, &mut BTreeSet::new()));
        assert!(augment(1, &options, &mut owners, &mut BTreeSet::new()));
        assert_eq!(owners[&a], 1);
        assert_eq!(owners[&b], 0);
    }
    #[test]
    fn impossible_source_assignment_fails() {
        let options = vec![vec![(1, 0)], vec![(1, 0)]];
        let mut owners = BTreeMap::new();
        assert!(augment(0, &options, &mut owners, &mut BTreeSet::new()));
        assert!(!augment(1, &options, &mut owners, &mut BTreeSet::new()));
    }
    #[test]
    fn snapshot_comparison_detects_mode_changes_with_the_same_routes() {
        let path = DISPLAYCONFIG_PATH_INFO {
            flags: DISPLAYCONFIG_PATH_ACTIVE,
            ..Default::default()
        };
        let mut mode = DISPLAYCONFIG_MODE_INFO {
            infoType: DISPLAYCONFIG_MODE_INFO_TYPE_SOURCE,
            ..Default::default()
        };
        mode.Anonymous.sourceMode = DISPLAYCONFIG_SOURCE_MODE {
            width: 1920,
            height: 1080,
            ..Default::default()
        };
        let a = Config {
            paths: vec![path],
            modes: vec![mode],
        };
        let mut b = a.clone();
        assert!(same_config(&a, &b));
        b.modes[0].Anonymous.sourceMode.width = 2560;
        assert!(!same_config(&a, &b));
    }
    #[test]
    fn cold_enable_uses_progressive_scan_with_exact_rate_and_raw_source_size() {
        let mut wanted = crate::display_apply::tests::layout(0);
        wanted.monitors[1].hdr = Some(true);
        let d = wanted.monitors[1].desktop.as_mut().unwrap();
        d.width = 1080;
        d.height = 1920;
        d.rotation_degrees = 90;
        let mut before = wanted.clone();
        before.monitors[1].enabled = false;
        before.monitors[1].desktop = None;
        before.monitors[1].hdr = None;
        let mut a = DISPLAYCONFIG_PATH_INFO {
            flags: DISPLAYCONFIG_PATH_ACTIVE,
            ..Default::default()
        };
        a.sourceInfo.adapterId = native_luid(1);
        a.sourceInfo.id = 0;
        a.sourceInfo.Anonymous.modeInfoIdx = 0;
        a.targetInfo.adapterId = native_luid(1);
        a.targetInfo.id = 1;
        a.targetInfo.targetAvailable = true.into();
        a.targetInfo.Anonymous.modeInfoIdx = u32::MAX;
        let mut b = a;
        b.flags = 0;
        b.sourceInfo.id = 1;
        b.targetInfo.id = 2;
        let mut mode = DISPLAYCONFIG_MODE_INFO {
            infoType: DISPLAYCONFIG_MODE_INFO_TYPE_SOURCE,
            adapterId: native_luid(1),
            ..Default::default()
        };
        mode.Anonymous.sourceMode = DISPLAYCONFIG_SOURCE_MODE {
            pixelFormat: DISPLAYCONFIG_PIXELFORMAT_32BPP,
            ..Default::default()
        };
        let plan = build(&wanted, &before, &[a, b], &[mode]).unwrap();
        let mut mirror = wanted.clone();
        mirror.monitors[1].desktop = mirror.monitors[0].desktop.clone();
        let mut clone_path = b;
        clone_path.sourceInfo.id = 0;
        let mirror_plan = build(&mirror, &before, &[a, b, clone_path], &[mode]).unwrap();
        assert_eq!(
            source_key(&mirror_plan.config.paths[0]),
            source_key(&mirror_plan.config.paths[1])
        );
        assert!(build(&mirror, &before, &[a, b], &[mode]).is_err());

        assert_eq!(
            plan.cold_hdr,
            vec![display_apply::identity(&wanted.monitors[1]).unwrap()]
        );
        let mut unreadable_active = before.clone();
        unreadable_active.monitors[1] = wanted.monitors[1].clone();
        unreadable_active.monitors[1].hdr = None;
        assert!(build(&wanted, &unreadable_active, &[a, b], &[mode]).is_err());
        let path = plan
            .config
            .paths
            .iter()
            .find(|p| p.targetInfo.id == 2)
            .unwrap();
        assert_eq!(
            path.targetInfo.scanLineOrdering,
            DISPLAYCONFIG_SCANLINE_ORDERING_PROGRESSIVE
        );
        assert_eq!(
            (
                path.targetInfo.refreshRate.Numerator,
                path.targetInfo.refreshRate.Denominator
            ),
            (60, 1)
        );
        let source = unsafe {
            plan.config.modes[path.sourceInfo.Anonymous.modeInfoIdx as usize]
                .Anonymous
                .sourceMode
        };
        assert_eq!((source.width, source.height), (1920, 1080));
    }
}
