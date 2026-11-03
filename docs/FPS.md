# FPS limits

Director can change the global FPS setting through the NVIDIA driver or an installed, running RTSS. Both work directly from the app; no rtss-cli, Profile Inspector or helper executable is required. Director does not need to stay open after applying. RTSS must stay running when its limiter is selected.

## Use it

- In **Current setup → FPS limit**, turn on **Change FPS limit**, choose NVIDIA or RTSS, select a cap or **Disable global limit**, and click **Apply FPS changes**.
- In a **scene**, use the same controls. Saving does not apply anything. The scene applies TV preparation → displays → audio → FPS → final apps/URIs. A scene containing only FPS is supported.
- **Change FPS limit off means leave all FPS settings unchanged.** To remove the selected provider’s global cap, turn the switch on and choose **Disable global limit**.
- Only available methods appear in the dropdown. Open RTSS first if using it. Check available methods can rediscover unavailable providers. Unavailable saved methods remain saved but fail preflight before scene device changes; there is no automatic provider fallback.

Headless commands:

    director fps status
    director fps set nvidia 117
    director fps set nvidia off
    director fps set rtss 120
    director fps set rtss off

Existing director scene apply commands use the saved FPS choice without opening the window.

## What the setting means

NVIDIA changes FRL_FPS in the driver’s global/base profile, not a particular monitor/GPU or a game-specific profile. RTSS changes its global profile. Game-specific overrides, in-game limits, other limiters, V-Sync and frame generation can affect the FPS you actually see. Selecting one method leaves the other unchanged. A running game may need restarting, particularly for driver settings. Readback verifies configuration, not measured game frame rate.

The first implementation supports NVIDIA integer caps 1–1023 through the documented setting domain (the installed driver can still reject values), and a conservative Director RTSS range of 1–1000. RTSS fractional or time-based global modes are rejected and left unchanged. Per-game targets, AMD/Intel providers and automatic refresh-rate offsets are outside this slice.

## Verification and recovery

The GUI and CLI share the same Rust providers and execution policy. FPS calls run on workers under the existing mutation guard. Provider discovery is read-only and serialized with SDK calls. A scene snapshots the selected provider before any TV/display/audio changes and validates availability up front. It writes and reads back FPS after audio, before any launches. Default failure/cancel recovery restores the original cap; NVIDIA predefined/inherited state and RTSS limiter-disable state are preserved. Stop and keep completed changes honors the existing scene policy.

RTSS has separate saved-cap and limiter-disabled states. Applying a cap clears only the limiter-disable bit when necessary; unrelated flags are preserved. Disabling its global cap writes zero without disabling game-specific limits globally. Save/reload and a separate disk check verify persistence. RTSS and Director may need matching privileges for settings notifications; Director reports blocked operations and never silently elevates or changes ACLs.

NVIDIA uses installed System32 nvapi64.dll and versioned DRS structures. RTSS is discovered from its machine installation registry entry and loads RTSSHooks64.dll from that installation. DLL dependency lookup is restricted to the installed directory and System32. Native interfaces are serialized; no resident Director service or automatic RTSS launch is added.

## Manual trial

Apply a cap such as 60 in Current setup, then check the saved setting in NVIDIA Control Panel / RTSS and test a game that can normally exceed 60. Try Disable global limit, then restore your preferred cap. Repeat through an FPS-only scene and CLI. Check that selecting a method or saving a scene alone changes nothing. A configured game profile can override a global limit, so choose a game without such an override for the first test.

## References

- [NVIDIA driver settings API](https://docs.nvidia.com/nvapi/group__drsapi.html), [setting definitions](https://github.com/NVIDIA/nvapi/blob/main/NvApiDriverSettings.h), [interface IDs / ABI](https://github.com/NVIDIA/nvapi).
- [rtss-cli source](https://github.com/xanderfrangos/rtss-cli), used as a protocol/interface reference. Implementation is independent Rust code; the CLI is not bundled or invoked.
- Installed RTSS SDK RTSSProfileInterface.h and RTSSHooksInterface.h are the local interface reference. RTSS itself is not redistributed.
