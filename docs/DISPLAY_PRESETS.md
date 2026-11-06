# Display presets

## Working scope

Capture the current Windows desktop configuration, give it a name, and save an independent display preset. View/rename and confirmed deletion manage the saved snapshot. None of these operations enable, disable, move, or reconfigure a monitor. The green check applies the saved layout with native confirmation and automatic reversion; see [applying display presets](DISPLAY_APPLY.md). The + button and preset pencil now open the custom extended-layout editor directly. HDR now has a capability-gated toggle; clone-group editing, color depth, VRR, and DPI scaling remain deferred.

From **Library › Displays**, choose **Capture current layout**, review the monitor details, enter a name, and choose **Save display preset**. The pencil opens the full saved preset editor directly; renaming preserves its captured layout and timestamp. Cancel discards the draft. Switching between audio and display pages preserves unsaved drafts independently. The cross removes only the chosen saved display preset after native confirmation.

Capture reads Windows again at that moment, rather than copying the last page refresh. Monitor positions and displayed dimensions are current desktop pixels. Optional HDR state is captured when available; see [HDR support](HDR.md). Current GDI dimensions are checked against the CCD source position, avoiding incorrect portrait dimensions on drivers that report an unrotated CCD source size. Raw CCD source dimensions are retained separately; refresh rates still come from the exact CCD fraction. The primary source is the one at (0, 0); mirrored targets can share that primary source. Refresh rates retain the exact numerator/denominator in storage and show two decimals in the UI. Mirroring retains shared source identity while keeping per-target refresh/orientation.

## Create and customize

Choose **+** to read the current monitors and open a configurable draft. The current arrangement is the starting point, so you do not have to rebuild it from scratch. **Capture current layout** remains a separate, quick snapshot flow. An existing preset's pencil opens all per-monitor controls directly.

Each monitor card includes enabled state, exclusive primary selection, resolution, dependent refresh-rate choices, visual orientation choices, and a boxed arrangement map. Drag screens in the map to reposition them, with edge snapping and overlap handling. Hover over a screen to see its size, refresh rate, primary status, and pixel position; there are no raw position fields. Selecting a new primary translates all enabled monitors to keep their relative positions, with the primary at 0, 0. Enabling a monitor places it to the right. The primary display's Enabled switch is disabled; hovering its enabled overlay explains that another primary must be selected first.

The compact orientation dropdown shows four rotated SVG monitor icons with the stand as its orientation reference and plain-language labels. Choices are left-aligned and selecting one closes the menu. Dropping a moved primary normalizes all positions around its new origin, preserving primary status. Pointer cancellation restores the pre-drag draft. Invalid coordinates, additional displays at the primary origin, and overlapping extended layouts are rejected. Captured clone groups remain supported by capture/Apply, but editing their shared settings is deferred.

**Save display preset** stores the draft; **Cancel** discards it. Neither changes the live setup. To switch, use the saved row's green Apply check. Windows validates the final topology at Apply time, so a mode being listed is not a guarantee that every combination of outputs works.

## Supported-mode discovery

The background worker queries DXGI Output1 mode lists for ordinary progressive SDR modes, mapped to the monitor device-interface path rather than its editable name. Resolution changes rebuild the refresh list, retaining exact rational rates. The current captured mode is preserved, including its raw CCD source dimensions. Match output device paths without requiring a historical adapter LUID. Accept unspecified scan-line ordering from the DXGI list as well as progressive ordering, while excluding explicit interlaced/stereo entries. DXGI enumeration retries mode-list changes with bounded buffers.

A connected disabled output may expose no DXGI mode list. In that case, only its exact CCD preferred mode is offered, with a visible explanation. If no mode is available, enabling it is disabled. Connect/enable the output in Windows and reopen the editor for more choices. Disconnected monitors remain visible, but cannot be newly enabled through the custom editor. This does not add remembered-monitor identity relinking or advanced color modes.

## Storage and identity

Display presets use schema version 1 in the Windows Local AppData known folder at %LOCALAPPDATA%\Director\display-presets.json. Sound settings live in each scene's Sound step, not in a preset file. A display preset has an internal ID, editable name, capture timestamp, and layout. Each monitor stores a Windows device path when available, current adapter/target locators, name, availability, enabled state, and an active desktop mode when enabled. A mode includes source identity, desktop position/dimensions, raw CCD source dimensions, exact refresh fraction, and orientation.

Names are nonblank, unique ignoring case within display presets, have a CLI name (lowercase with dashes) that no other display preset uses, and at most 160 UTF-8 bytes. Audio and display presets may use the same name. Shared atomic write and operation-lock mechanisms preserve corrupt/future files and reject stale writers. Capture errors never save partial layouts.

Adapter LUIDs, source IDs, and target IDs are current-session locators, not durable hardware identities. Device paths can change after hardware/driver changes; missing paths remain explicit. Apply re-enumerates, resolves device paths, and reports missing/ambiguous targets rather than applying these locators blindly or matching friendly names alone. This first schema is a captured desktop description, not a complete serialized driver configuration.

## Direct Windows capture

src/windows_displays.rs calls GetDisplayConfigBufferSizes, QueryDisplayConfig, DisplayConfigGetDeviceInfo, and EnumDisplaySettingsExW directly. Capture invokes no display-changing API or external executable. The separate Apply adapter uses SetDisplayConfig directly after validation and snapshotting. ALL_PATHS is needed to discover disabled targets; it can be expensive, so capture/refresh runs on background jobs only when requested. Possible routes are deduplicated by target; an active route takes priority. Unidentifiable unused ports are omitted. Named disconnected targets are retained when Windows reports them; the list is not a permanent monitor inventory.

Buffer sizing/query races retry up to four times. Active source-mode indices, types, and identities are checked before reading native unions. Invalid/incomplete active modes or refresh data are reported rather than silently filled in. Virtual-refresh-rate distinctions, headset displays, remote-session/legacy-driver compatibility, and stable hardware relinking are outside this slice. Hardware failures and hot-plug races still need wider physical validation.

## Headless commands

```text
director display screens
director display screens --json
director display list
director display list --json
director display capture "Desk monitors"
```

current reads the live layout; presets reads saved layouts; capture saves a new named snapshot. These inspection/capture commands do not change displays or audio. Commands run before GUI or audio-COM initialization. Exit codes follow the existing CLI contract. Display check and apply use the same model and transaction engine; see [confirmation and automation options](DISPLAY_APPLY.md#cli).

## Sources

- [QueryDisplayConfig](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-querydisplayconfig)
- [Desktop source mode and primary origin](https://learn.microsoft.com/en-us/windows/win32/api/wingdi/ns-wingdi-displayconfig_source_mode)
- [Current desktop display settings](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-enumdisplaysettingsexw)
- [Monitor names and device paths](https://learn.microsoft.com/en-us/windows/win32/api/wingdi/ns-wingdi-displayconfig_target_device_name)
## Windows starred refresh choices

Windows Advanced display can show rates with an asterisk that require changing the selected resolution. Director's refresh dropdown currently filters to the selected resolution, so a starred Windows alternative need not appear in that specific list. Do not insert such a rate into the native-resolution pair unless Windows actually reports that pair. [Microsoft explains the asterisk](https://support.microsoft.com/en-us/windows/hardware/display-graphics/change-the-refresh-rate-on-your-monitor-in-windows).

The user's P27 screenshot shows 59.95 Hz and 50 Hz*. The earlier explanation that the monitor had only one refresh option was too broad: it referred only to the selected-resolution list and omitted the cross-resolution distinction.
