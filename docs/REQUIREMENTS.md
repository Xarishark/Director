# Requirements and implementation order

Status: planning. None of the following capabilities has been implemented or validated in the app.

## 1. Profile model

### Display profiles

Capture and apply enabled displays, primary display, resolution, refresh rate, rotation, and desktop positions. Preserve fractional refresh rates where supported. Report missing or ambiguous displays rather than selecting an unintended one.

A display-only operation makes no explicit audio, TV, Steam, or RTSS changes. Windows may naturally change HDMI audio availability when displays are enabled or disabled.

### Audio profiles

Capture and apply input selection, output selection, and device volume independently. A profile can change only an input, only an output, only an explicit device's volume, or a combination. Omitted settings remain unchanged. An audio-only operation makes no display, TV, Steam, or RTSS changes.

Expose which Windows default audio roles a profile controls. The initial default role selection is still undecided.

### Combined setups

A setup can select a display profile, an audio profile, either independently, and optional TV/RTSS/final actions. Components can be created independently in any order and reused by multiple setups.

References to saved components, update propagation, deletion behavior, and the final UI term for a combined setup remain design proposals; see [the decision record](DECISIONS.md).

## 2. Creation and management

First implement capture of the current display or audio state. Let the user review the result, select the audio settings to control, name it, and save it. Capturing/saving must not change device settings or apply the result.

Provide profile editing, duplication, deletion, and a readable summary. As basic workflows become available, allow independently captured profiles to be assembled into setups.

Add step-by-step custom profile creation later while keeping capture available. Its exact release milestone, including whether it blocks 1.0, is undecided. Capture does not infer desired TV power actions, launch actions, or timings from current state.

## 3. Remembered audio devices

- A device must be enumerated at least once before it can be chosen for a profile.
- Retain remembered devices when unavailable and show availability clearly.
- Display the Windows-friendly name and expose a copyable identity in device details.
- Names can change and are not unique. Renaming an endpoint must not change a profile's intended target.
- Prefer `PKEY_AudioEndpoint_StableId` when available on Windows 11 24H2 or later. Fall back to the ordinary endpoint ID when unavailable.
- Treat both IDs as opaque values. Stable IDs are case-sensitive and are not guaranteed permanent.
- Refresh mutable properties after resolving a remembered device. Retain the last observed name while unavailable.
- If the identity can no longer be resolved, allow explicit user relinking. Do not silently substitute an endpoint based on its name.
- Remembering a device does not itself enable automatic switching.

When a combined setup enables the TV, wait a bounded, configurable period for its remembered HDMI audio endpoint before applying audio settings. A timeout must not cause selection or volume changes on another device.

Technical references: [endpoint IDs](https://learn.microsoft.com/en-us/windows/win32/coreaudio/endpoint-id-strings), [Stable ID](https://learn.microsoft.com/en-us/windows/win32/coreaudio/pkey-audioendpoint-stableid), [friendly name](https://learn.microsoft.com/en-us/windows/win32/coreaudio/pkey-device-friendlyname).

## 4. Integrations

### Windows display and audio

Control Windows directly instead of requiring Display Changer II or NirCmd. Verify applied state where APIs permit it. API selection and compatibility details need validation during implementation.

### LG webOS

Configure/pair a TV, retain pairing credentials locally, wake it using Wake-on-LAN, select HDMI inputs, and turn it off. Support bounded retries and configurable settling delays. Implement these operations without ColorControl.

Coordinate TV readiness, display selection, and HDMI audio availability. Do not assume that successfully sending a command proves the final physical TV state.

### RTSS

Read/set the global frame limit through RTSS's interface without `rtss-cli` or `frltoggle`. RTSS remains the installed component that enforces frame limits. Report when it is unavailable; validate privileges and integration behavior during implementation.

### Ordered final actions

Allow optional ordered executable launches and URI dispatch, including Steam Big Picture. Executable actions support separate arguments and an optional working directory. Basic actions must not require batch files or a shell. Users can add, edit, remove, reorder, and enable/disable actions.

Run final actions after configuration. Define their gating after a failed required step and whether executable actions can wait for exit before implementation. URI dispatch cannot generally prove application readiness or completion. Arbitrary scripting is not an agreed initial requirement.

## 5. Execution and recovery

Prevent overlapping applications from the GUI and CLI, including during recovery. Report per-step progress and distinguish success, failure, partial application, and recovery results. Never report partial application as full success.

Failure behavior is user-selectable:

1. **Revert to previous settings** — the default.
2. **Stop and keep completed changes** — stop further setup actions and report partial application.

For recovery, snapshot the actual relevant pre-operation state rather than the last saved profile. Include each affected audio role and target device's volume. Track attempted changes, because a failed API operation may already have changed some state.

Restore reversible changes in an order that respects device dependencies. Verify restoration, continue independent recovery steps after a recovery error, and report incomplete recovery honestly. App launches and URI side effects may be irreversible. TV recovery requires known prior state and a reachable device.

A GUI display-confirmation countdown is a separate unconfirmed proposal. Per-setup and per-command failure-policy overrides are also undecided.

## 6. GUI, CLI, and notifications

- Provide a proper WinUI 3 desktop window for managing profiles and devices.
- Share application behavior and saved profiles between GUI and CLI.
- CLI application works with the GUI closed and does not initialize a window.
- Provide profile listing, status, capture, application, help, and meaningful error/exit reporting. Exact syntax remains proposed.
- Define how simultaneous GUI/CLI requests are handled before implementation.
- Use native Windows notifications rather than custom notification popups.
- Notifications should identify the setup and outcome and allow opening the relevant result details.
- Respect Windows notification preferences. Preserve local results and CLI output even when notifications are suppressed.
- Failure/success notification defaults and an explicit quiet option still need confirmation.

## 7. Implementation sequence

This sequence is a planning proposal, not authorization to start development.

1. Establish the Rust/WinUI 3 shell and shared model; enumerate and capture displays/audio without changing settings.
2. Save, review, and apply independent profiles with state verification and recovery.
3. Compose setups and coordinate TV/display/audio readiness, RTSS, and final actions.
4. Validate notification activation, headless CLI behavior, packaging, and real hardware workflows.
5. Add guided creation and other later features according to agreed release scope.

System tray mode and starting with Windows are explicitly deferred until after basic functionality. CRU and SpotBye are excluded.

## 8. Before 1.0

Validate display/audio-only isolation, actual desk and TV transitions, fractional refresh/rotation, remembered device reappearance, renamed/relinked endpoints, missing devices, timeouts, partial failures, both failure policies, incomplete recovery, RTSS, ordered launches/URIs, notification suppression, and CLI use with the GUI open and closed.

Measure cold startup, idle CPU, memory, and CLI startup on representative release builds. No performance budgets or comparative framework benchmarks have been agreed yet.

Set the supported Windows baseline, distribution/update model, app/WinUI 3 licensing, and release-blocking feature scope. Validate the packaged app without development tools installed and document working setup/build/test instructions and limitations.
