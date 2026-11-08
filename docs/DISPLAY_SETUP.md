# Display option detection

In a display preset, choose **Set up this display** (or **Detect display options** for an already-active display). The explanation stays inside that display's card; nothing changes until **Detect options** is pressed. Turn the display on and select the PC input first.

When a paired TV is linked to that Windows display and has a saved wake address and PC input, **Wake and detect** is also available. It wakes that TV and selects the saved PC input. The TV stays on afterward; restoration applies to the Windows display layout, not the TV's power or selected input.

## Progress and cancellation

The card shows preparing, optional TV preparation, temporary enabling, reading, restoring and verification. An indeterminate native progress bar avoids inventing percentage estimates. Unrelated controls and Save are disabled while setup runs. Cancel detection is available until restoration starts; cancellation waits for the current native/network call to finish, then restores the captured layout. Normal window closing is protected while the operation runs.

Detection has a cooperative 30-second work deadline checked between steps. In-flight native calls use their existing behavior/timeouts and cannot be forcibly interrupted safely. Restoration runs even after cancellation/deadline and is not skipped merely because that deadline has elapsed.

The current primary/active displays are retained. Temporary activation is validated by Windows, and the original enabled displays are checked for changes before capabilities are accepted. Any attempted activation, including a setter that fails after a partial change, goes through restoration and readback verification. TV wake/arrival is also treated as potentially changing Windows topology. No temporary topology is committed to Windows' persistent display database. Existing HDR state is restored; the discovery action does not explicitly enable HDR on its target.

If restoration fails, the card keeps its recovery snapshot and offers **Restore previous layout**. Saving/canceling that draft is blocked until recovery succeeds. Recovery exists in memory; force termination, process crashes and reboot are outside this in-process restoration guarantee. A persistent recovery watchdog is not part of this implementation.

## Remembered choices

Only complete, active display inventories are remembered after successful restoration. Exact resolution/rational-refresh pairs and HDR availability are stored by Windows device-interface path in the known Local AppData Director directory, in versioned display-capabilities.json. Inactive fallback reports never replace a complete inventory. Corrupt/future cache files are preserved.

Inactive/disconnected cached displays show **Previously detected · checked again when applied**. These choices can be used to author a preset while the screen is off. A display without learned capabilities offers setup instead of a misleading fallback resolution dropdown. Newly detected displays are enabled in the draft only; the live layout is already restored. Existing draft positions/orientation are preserved when learned modes are installed.

Save still stores a preset without applying it. Apply uses the existing fresh Windows validation and exact post-apply verification; unsupported modes are not silently substituted. A changed driver, port, cable or display identity can require detection again.

## Native implementation

Windows selects the target timing using documented [unspecified refresh/scan-line rules](https://learn.microsoft.com/en-us/windows/win32/api/wingdi/ns-wingdi-displayconfig_path_target_info) and [SetDisplayConfig validation/best-mode completion](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-setdisplayconfig). Existing screens retain supplied modes and are checked afterward. Device/probe work runs on workers under the shared operation lock, with thread-safe progress messages to WinUI.

## Validation boundary

79 regular tests plus native in-memory UI checks cover success, partial writes, read failure, panic, cancellation, failed TV preparation, recovery retry, original-screen preservation, cache precision/corruption, explanation/progress and learned 4K/120 Hz/HDR controls. The UI fixture never activates a physical display, sends a TV command or writes a fake capability cache. Actual TV and driver activation and restoration need checking on real hardware.


Detection selects Windows' preferred resolution and the highest freshly reported refresh rate at that resolution in the draft. It does not guess native resolution from the largest driver mode (which could be virtual/supersampled). If Windows does not report a preferred resolution present in the inventory, the observed mode is retained. Explicit detection replaces even a supported fallback draft mode, preserving arrangement and orientation; it does not apply the chosen mode to the live desktop.

Monitor cards put the include switch beside the name. Disabled cards omit mode fields, the primary-display action, and the completed-detection message; warnings and recovery errors remain visible.

## Cold-display HDR and wide-window correction

Detection enables an inactive target at the preferred pixel dimensions reported by CCD, rather than probing a 640x480 desktop that may suppress HDR availability. It logs the actual probe dimensions, draft recommendation, inventory size and HDR result. Existing displays remain checked and the original layout is restored afterward.

Apply permits a previously disabled display whose HDR state is unreadable while inactive. After enabling its requested mode it waits up to three seconds for a fresh HDR reading, records its retained HDR preference before any HDR setter, and then changes HDR. On rollback that preference is restored while the target is still enabled, before restoring the original topology. Already-active displays with unreadable HDR still fail preflight. Cached capability data never bypasses live HDR validation.

The editor uses one to three monitor-card columns according to window width. Detected settings remain editable; completed setup copy disappears and detection becomes a subtle Detect again action. Initial setup, errors and recovery controls remain available.

## Mirror editing

Desktop mode offers Extend desktop or Mirror with another enabled display. Joining chooses a reported common resolution, keeps each output's reported refresh rate when available, and shares source size, orientation and position. Changing a shared resolution updates all group members. Detaching places the selected output to the right as a separate desktop. One arrangement tile represents the group and moves all members together. Save changes only the preset; applying still uses fresh CCD routes and exact verification.

HDR remains an explicit per-target request. Windows can restrict HDR in duplicated layouts; Director does not silently discard an HDR-on request. Select HDR off or use an extended layout when validation reports it unavailable. A successful unsupported-HDR API result is distinguished from a failed query. See [Microsoft's HDR guidance](https://support.microsoft.com/en-us/windows/hardware/display-graphics/hdr-settings-in-windows).
