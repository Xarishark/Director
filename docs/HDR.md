# HDR control

HDR is an optional per-monitor setting in display presets and Now. New captures include Windows' reported HDR preference when the display/API supports it. The toggle is disabled with an explanation when that state is unavailable.

## Storage and application

The optional monitor hdr field is a boolean. Missing/null means leave HDR unchanged, preserving legacy presets. Opening a legacy preset in the editor initializes an unspecified HDR choice from the fresh Windows state; merely opening/cancelling never writes the file.

Display Apply validates topology, snapshots the actual configuration and known HDR state, applies topology first, then changes HDR by freshly resolved monitor identity. Readback verifies the requested HDR setting as well as geometry/modes. Commit retains the verified configuration; recovery restores known prior HDR states as well as the topology. A requested HDR change is rejected before mutation when its prior state cannot be read for recovery.

The adapter uses HDR-specific device-info types GET_ADVANCED_COLOR_INFO_2 (15) and SET_HDR_STATE (16), with checked WinSDK-compatible C layouts. It does not use the generic Advanced Color/WCG setter. Unsupported API versions or devices remain unavailable; there is no guessed fallback that could toggle WCG instead of HDR. See [Windows device-info types](https://learn.microsoft.com/en-us/windows/win32/api/wingdi/ne-wingdi-displayconfig_device_info_type).

The captured value/readback concerns Windows' user HDR setting, not a measurement of emitted light or game HDR rendering. Changing HDR can affect available mode combinations; strict post-apply readback and recovery still apply. Recovery after a crash or forced kill mid-change is not provided.

## Validation scope

Tests cover HDR readback requirements, legacy leave-unchanged behavior, commit failure recovery, and ABI sizes. Native UI checks cover supported/unsupported toggle state, draft persistence, and cancellation without changing live HDR. Physical HDR flips and HDR-specific native setter failures have not been injected on hardware.
