# Architecture proposal

Status: Rust, WinUI 3, independent profiles, and a shared GUI/CLI engine are accepted. Module names, storage format, process model, and exact execution interfaces below are proposals for implementation.

## Shared engine

Keep profile capture, persistence, validation, planning, application, verification, and recovery in Rust code independent of WinUI 3. The GUI and CLI call this same engine so behavior does not drift between entry points.

The CLI path must not construct the GUI, start its event loop, or require an existing GUI process. Whether distribution uses one executable or separate GUI/CLI executables is still open.

## Proposed boundaries

| Component | Responsibility |
| --- | --- |
| Domain/model | Display profiles, audio profiles, combined setups, action definitions, device identities, failure policy, operation outcomes |
| Engine | Capture, validate, build an ordered plan, snapshot affected state, apply, verify, and recover |
| Windows displays | Enumerate/query layouts and apply/verify Windows display changes |
| Windows audio | Enumerate endpoints, read identity/name/availability, select defaults, read/set device volume, observe availability changes |
| LG integration | Pairing, Wake-on-LAN, webOS connection, input/power actions, bounded readiness waits |
| RTSS integration | Discover the installed component and read/set its global frame limit directly |
| Launch integration | Executable launch and URI dispatch; accurate dispatch outcomes |
| Persistence | Versioned profiles, remembered inventory, operation results, recovery state, separate credential storage |
| Notifications | Native Windows notifications and activation into result details |
| GUI | WinUI 3 views, user input, progress, results; no duplicated device-control policy |
| CLI | Argument parsing, shared engine invocation, output and exit status |

These are logical boundaries, not a committed crate/package layout.

## Proposed data relationships

- Give app profiles and setups stable internal IDs with editable display names.
- Keep app profile IDs distinct from Windows device identities.
- Let a setup reference optional display/audio profiles rather than copy their settings.
- Editing a referenced component would affect future applications of every setup that uses it; duplication would create an independent variant.
- Show dependent setups before deleting a component and prevent dangling references.
- Store optional fields explicitly so an omitted field means leave unchanged.

Reference/update/deletion semantics need user confirmation before the schema is finalized.

## Device identity

Audio endpoint identity is separate from its mutable Windows name and current availability. Persist the preferred Stable ID when available, plus the current ordinary endpoint ID and last observed name. Re-query properties when resolving a device; never parse opaque IDs or resolve ambiguity by friendly name alone.

The display identity strategy needs separate validation. Do not assume transient display numbering or adapter IDs survive hardware/driver changes.

## Proposed execution lifecycle

1. Resolve the requested profile/setup and obtain exclusive operation ownership.
2. Validate configuration and dependencies that can be checked without changing hardware.
3. Capture known previous state for affected resources before changing them.
4. Prepare devices needed by later steps, such as waking the TV and selecting its input.
5. Apply/verify display settings, wait for the requested audio endpoint, then apply/verify audio settings.
6. Apply other requested configuration, including RTSS.
7. Dispatch optional final actions if the agreed gating rules permit them.
8. Persist and report the result; notify according to settings.

On a required failure, use the selected policy. The default recovers affected settings using the captured state, keeps operation ownership during recovery, and reports the recovery result separately from the original failure.

The exact order is dependency-driven and must handle desk transitions as well as TV transitions. This sequence is illustrative, not a fixed requirement that every setup contains every stage.

## Responsiveness and resource use

- Keep network and device waits off the UI thread.
- Prefer device notifications and bounded waits over permanent polling.
- Run CLI operations without UI initialization.
- Keep tray residency and startup integration out of the first implementation.
- Measure release-build behavior before selecting rendering options or claiming resource targets.

## State and local files

Profiles, remembered devices, credentials, diagnostic logs, and recovery snapshots have different purposes. Do not treat the last requested/successful setup as proof of the current Windows state.

Choose storage locations, schema migration, credential protection, and interrupted-operation recovery during implementation planning. Real machine inventories, pairing keys, and runtime results must not be committed to the repository.

## Technical starting points

- [Rust for Windows](https://learn.microsoft.com/en-us/windows/dev-environment/rust/rust-for-windows)
- [QueryDisplayConfig](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-querydisplayconfig)
- [SetDisplayConfig](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-setdisplayconfig)
- [Audio endpoint Stable ID](https://learn.microsoft.com/en-us/windows/win32/coreaudio/pkey-audioendpoint-stableid)
- [Windows app notifications](https://learn.microsoft.com/en-us/windows/apps/develop/notifications/app-notifications/)

These are research references, not proof of a working integration. Pin dependency/tool versions only when implementation is authorized and they have been validated.
