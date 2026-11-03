# Current implementation

Updated: 2026-11-03. This covers the current setup view, audio/display presets, and guided LG/Samsung TV setup, MAC learning, and basic status/power/input controls, not the full pre-1.0 plan.

The leading summary reflects the current build; dated implementation notes below retain historical validation limits. See [Scenes](SCENES.md) for the current composition scope.

## Implemented

- Native Samsung Tizen provider and brand selection, optional direct HDMI compatibility tests with explicit user confirmations, hidden unsupported controls and redacted JSON report export. GUI, CLI, display preparation and scenes share provider dispatch. Physical Samsung validation is pending. See [Samsung preview](SAMSUNG.md).

- Native Scenes cards/editor and shared headless runner, independent preset/TV references, optional TV preparation, bounded HDMI audio readiness, native executable/folder pickers and ordered app/URI dispatch. Progress, cancellation, display confirmation and scene-wide Windows recovery are included. [Current scene scope](SCENES.md).
- Explicit remembered TV audio endpoint metadata in setup/editing. No name-based automatic pairing.


- One native WinUI 3 frontend through Microsoft Windows Reactor 0.100.0, selected by the user after comparing interactions. The root package builds director.exe; there is no separate frontend package or alternate GUI feature.
- Native Audio presets main view, compact selectable rows with visible Apply/Edit/Delete icon buttons and tooltips, Add/Edit page, Save/Cancel, native deletion dialog, and InfoBar results.
- Name, output dropdown, optional volume, failure policy, retained unavailable output identity, and device-ID disclosure/Copy. Navigation and disclosure state survive form updates.
- Shared versioned persistence in the Windows Local AppData known folder, stable preset IDs, case-insensitive unique names, stale-writer detection, and corrupt-file preservation.
- Direct Core Audio reads and volume control, isolated PolicyConfig default switching, all three output roles with an optional separate communications device, optional microphone/capture roles, snapshot/readback/recovery, and keep-changes alternative. Mute is preserved.
- Background work, message-based UI updates, cross-process operation mutex, and native close protection while work is active.
- Headless presets/list/check/apply commands in the same executable, using the same backend without creating a window.
- SVG source artwork, embedded executable icon, adjacent window icon, and machine-local Director desktop shortcut.

## Display capture

- Native Display presets navigation/list with capture, name/save, view/rename, Cancel, and confirmed deletion. Audio and display drafts survive page switches independently; saving never changes hardware.
- Direct Windows CCD enumeration for active and named inactive targets; exact refresh fractions, source/target identities, names/device paths, enabled state, position, primary source, and orientation. Current GDI settings supply actual desktop extents; raw CCD sizes are retained separately to handle portrait driver differences correctly.
- Independent versioned display-presets.json in the existing Local AppData folder, with the same atomic writes, operation ownership, stale-writer checks, and corrupt/future-file preservation.
- Headless display current/presets inspection and named capture, without GUI or audio initialization. Native Apply and timed confirmation/recovery are implemented; see [Apply behavior and validation](DISPLAY_APPLY.md). See [display capture scope](DISPLAY_PRESETS.md).

## Validation

Before promotion, the native frontend passed its release build, strict linting, formatting, and hidden-window close-guard test. The backend passed 13 tests for transactions, recovery, persistence, validation, and concurrency.

Native accessibility automation covered Add/Save, optional volume persistence, Edit/Cancel, duplicate-name errors, deletion cancellation/confirmation, restart persistence, pane state, output dropdown, and device-ID disclosure. Owned-window captures were inspected at 1000 x 700 and 760 x 600 logical client sizes. Temporary fixtures were removed and original preset data preserved. The only automated Apply targeted a guaranteed nonexistent endpoint and returned No changes made before any Windows audio write.

Headless checks covered help, preset listing, read-only Windows audio access, missing names, and invalid volume. Earlier user reports accepted the shared backend workflow for real output switching, optional volume, saved presets, and terminal application. The user subsequently reported that native WinUI interactions are substantially better and selected this frontend; that is not a separate claim of observed physical Apply or recovery through every new UI path.

After promotion, the canonical director.exe passed the optimized release build, all 14 tests (13 backend plus one native close-guard test), strict all-target Clippy, and formatting. Its native UI automation again passed preset CRUD, optional volume, dialogs, persistence, pane/dropdown/device-ID state, default/narrow captures, and guaranteed-missing-endpoint Apply. Original presets were preserved; the unsaved-form check left the settings file byte-for-byte unchanged. Headless checks passed, the main Director shortcut was verified against the new executable, and the obsolete comparison shortcut was removed. Source/configuration/docs/lockfile scans found no removed frontend dependencies or references. Machine-specific logs, captures, fixtures, and resource samples remain under globally ignored .localonly.

Display-capture validation on 2026-10-31: the optimized release build, strict all-target Clippy, and formatting pass. All 20 tests pass (19 backend/model/storage/native-parsing tests plus the native close-guard test). Added coverage includes negative positions, cloned sources, fractional rates, invalid mode indices/types/identity, captured-data preservation during rename, and portrait desktop extents while retaining raw CCD sizes.

Live read-only Windows capture found two enabled monitors and one named disabled TV. Independently read Windows desktop bounds confirmed the primary monitor and the portrait monitor position/dimensions. Native UI automation exercised navigation, independent drafts, capture/save, view/rename/cancel, duplicate errors, deletion confirmation/cancellation, restart persistence, and default/narrow layouts. CLI capture/listing and duplicate rejection passed; duplicate capture preserved file bytes. Captures of the native editor/list/error/dialog were inspected at 1000 x 700 and 760 x 600 logical client sizes. These were native accessibility checks, not a claim of full real mouse/keyboard or assistive-technology testing.

Temporary display fixtures were removed, original display data/absence was restored, audio presets were unchanged, and live display snapshots before/after matched. Existing audio UI regression checks passed again, including a guaranteed-missing endpoint preflight without audio writes. No physical monitor change was performed. Machine-specific evidence stays in ignored .localonly.

Display Apply validation: all 31 tests, strict lint/format, and the optimized release build pass. Native identity/routing validation and no-op checks passed. Real secondary-monitor enable/offset, 15-second automatic reversion, native Revert, Keep/persistence, guarded close, cross-process exclusion, and headless verified restoration were exercised; the original single-screen desktop was restored. The native confirmation dialog was captured and inspected at default/narrow sizes. Missing-monitor rejection made no changes. Owned test presets were cleaned while preserving the existing user preset. See [detailed scope and limits](DISPLAY_APPLY.md).

## Deferred and limited

Remaining work includes physical display Identify, DPI/color editing, scenes, scene-level TV orchestration, driver/FPS control, system notifications, guided creation, installer/updater, tray, and startup integration. Mirror editing and standalone TV power/input controls are implemented as detailed below. Drag arrangement, capability-gated HDR, and initial TV setup/pairing are implemented as detailed below. See [Audio](AUDIO.md) for remembered endpoint identities and relinking.

PolicyConfig is undocumented. Readback does not prove audible playback or force explicitly routed applications to migrate. Forced termination has no recovery journal; stalled display/audio native calls cannot safely be canceled. TV WebSocket receives have their own bounded cancellation. Hardware failure/recovery and deliberately slow device calls need physical validation. Runtime packaging, clean-PC testing, and release licensing remain outstanding; see [runtime notes](WINDOWS_GUI.md).

## Repository state

Work remains local and uncommitted. Commit/push only with explicit authorization and retain the existing signing configuration. Desktop shortcuts and machine-specific development material are not repository source.
### Initial confirmation preference — 2026-10-31 (default superseded below)

Display presets now store an Ask to keep display changes switch. New and legacy presets default to on. GUI and CLI may keep verified changes automatically when it is off; CLI --yes and --confirm override the saved preference for one invocation. Verification, persistence checks, busy-close protection, and failure recovery remain active.

All 33 tests, strict all-target Clippy, formatting, and the release build pass. Native accessibility checks verified default-on, opt-out saving, restart persistence, Cancel, re-enabling, CLI listing, and conflicting-flag rejection. Default/narrow window captures were inspected. Test presets were removed; the existing preset identity, name, timestamp, and exact layout were preserved, with only the explicit default-on field added during serialization. Audio preset bytes and the live display layout were unchanged. These preference checks did not apply a layout; previous live Apply validation above predates this preference change.

### Initial custom display creation — 2026-10-31 (interaction revised below)

Create preset, separate from Capture current layout, opens native per-monitor controls. Existing presets can use Customize layout. Background mode discovery uses DXGI exact refresh fractions and physical interface paths, with an explained CCD preferred-mode fallback for disabled outputs. The editor provides enabled/primary selection, dependent resolution/refresh dropdowns, SVG orientation choices, numeric positions, and a read-only arrangement preview. Pure draft validation protects primary selection, invalid positions, and overlapping extended layouts. Saving never applies hardware changes.

All 36 tests, strict all-target Clippy, formatting, and release build pass. Native accessibility checks exercised Create, supported resolution selection, the rendered SVG orientation flyout, secondary-disable editing, navigation draft preservation, Save, restart, Customize, and Cancel. Windows validate-only accepted the saved 1920x1080/180-degree custom fixture without applying it. Toolbar, editor/preview, flyout, scrolling controls, and minimum-width window states were inspected using captures of owned windows. Fixture cleanup preserved existing presets, audio file bytes, and the exact live layout.

The first native run exposed an unsupported Reactor rich-flyout target; it now uses the supported native Button/flyout combination. Checks above cover the corrected build. Creation changes were not physically applied during this slice; earlier display Apply/recovery hardware validation remains separately recorded. Broader monitor/GPU mode changes, disabled-output combinations, and actual assistive-technology/physical input validation remain outstanding.

### Cohesive direct display editor — 2026-10-31

Both preset lists now share the same heading/toolbar component, with aligned top-right refresh and accent + buttons. Display capture is a secondary camera icon before that pair. The display pencil loads all controls directly; Customize is removed. Audio and display edit footers both put Cancel then Save on the right. New GUI, quick-captured, and CLI-captured display presets default confirmation off; existing explicit preferences and legacy presets are preserved.

The arrangement box supports pointer-captured dragging. A fixed transform during each gesture avoids rescaling under the cursor; dropping snaps nearby edges, resolves overlaps, and normalizes the primary origin. Coordinates are screen-hover details, not editable form fields. The primary Enabled switch is disabled and greyed out; a transparent enabled overlay provides its hover explanation.

All 38 tests, strict all-target Clippy, formatting, and release build pass. Native accessibility checks verified plus-button alignment, direct Edit, absent Customize/position fields, disabled primary state, GUI/CLI default-off persistence, restart/Cancel, and default/minimum-width layouts. A real pointer press/move/release changed and saved a draft monitor position. Real hover exposed screen resolution/refresh/coordinates and the disabled-primary explanation. Owned-window captures were inspected. Tests did not Apply live displays; test presets were removed, original presets and audio settings were preserved, and the actual Windows layout remained unchanged.

A native release-order issue was found: Reactor calls ReleasePointerCapture before enqueuing PointerReleased, so CaptureLost may arrive first. Capture-loss cancellation now waits 25 ms on a worker and checks a gesture token, allowing the queued release to finish and preventing stale cancellation of a later drag. PointerCanceled still restores the pre-drag draft immediately. Temporary tracing used to verify the ordering was removed. Full touch/pen/keyboard-only map interaction and deliberate mid-gesture focus/capture failures remain unverified.

### Current setup, HDR, mode lists, and picker polish — 2026-10-31

Current setup is a separate navigation page combining the shared display editor with default audio output/volume controls. Its display and audio Apply operations use the existing engines independently and never write preset documents. Pending edits survive navigation; Discard rereads actual Windows state. Native page roots are keyed separately so scrolling does not leak between pages. Audio choices are reread after display application, preserving pending audio edits.

Visible Refresh buttons were removed from preset lists; clean page entry still reads the current data. Orientation choices now have compact equal-height rows, aligned stand-only SVGs/text, a standard chevron, and dismissal after selection. The extra icon arrow was removed at the user's request.

Mode discovery no longer requires a historical adapter LUID to match a physical device path. It accepts unspecified as well as progressive DXGI scan-line ordering, while rejecting explicit interlaced/stereo modes. Native UI enumeration exposed multiple resolutions on both monitors and multiple refresh choices on AW3225QF. P27h-20 exposed one refresh choice at its native resolution, as reported by Windows.

HDR is an optional per-monitor setting, capability-gated using the HDR-specific Windows API. Missing legacy fields leave HDR unchanged. Explicit HDR settings participate in preflight availability/recovery checks, readback, persistence, and restoration. The native toggle was available on AW3225QF and disabled on P27h-20. See HDR.md for the API contract and hardware-test limits.

All 43 tests, strict all-target Clippy, formatting, and release build pass. Native checks covered refresh-button removal, expanded mode choices, compact orientation selection/dismissal, HDR draft save, Current setup pending state/navigation/discard, and default/minimum-width views. A guarded Current setup audio Apply targeted the already-active Console+Multimedia output without requesting volume, returned Already set, and preserved both defaults and volume. Preset files remained byte-identical during that live-view check; earlier owned HDR fixtures were cleaned while preserving the existing user preset. Display/HDR state was not changed by these checks.

An unrestricted whole-window UI Automation FindAll while an audio dropdown was open stalled the provider/test window. Those exact owned test processes were terminated after confirming no device operation held the operation gate. Searching only the dropdown's own items completed correctly, including selection and the no-op Apply. Test helpers now scope that query. No temporary event tracing remains in source. Full assistive-technology stress, native HDR toggles/failure injection, and wider hardware combinations remain unverified.

### HDR explanation tooltip — 2026-11-01

Rebuilt the desktop executable after Director was closed. The permanent unavailable-HDR subtitle has been replaced with a tooltip on an enabled transparent hover layer above the disabled switch. Supported HDR controls also have a short hover description. Native hover verified the reason appears only on hover; the unsupported toggle remains disabled. The unsaved test draft was cancelled, preset/audio files were preserved, and live display/HDR state did not change.

LG inactive-TV behavior remains unchanged. The attempted inactive-mode filtering was reverted before this build; remembered monitor capabilities are a proposal, and overview monitor drawers remain deferred. The P27 50 Hz* explanation is documented as a Windows cross-resolution choice, not an additional native-resolution pair.


## TVs setup/pairing — 2026-11-01

Added a native TVs navigation page and consistent +, edit, removal dialog, and fixed Cancel/Save TV footer. Manual IPv4 setup can optionally remember a Windows physical display path/name, independent of presets. Drafts survive navigation. Pairing and authenticated connection/input-list checks run on background workers through native WinHTTP, sharing the operation lock and protected storage. Pairing key/certificate continuity data uses per-user DPAPI. Default TLS 3001 and explicitly selected legacy 3000 are separate modes; no silent downgrade. Name/display-link edits retain pairing; an endpoint change clears it in this first slice. `tv list`, `tv pair`, and `tv check` work headlessly. See [TVs](TVS.md).

Validation: 52 Rust tests passed (49 library, 3 application), strict all-target Clippy, formatting, and offline release build. New tests cover TV endpoint validation/uniqueness, stale/corrupt storage preservation, metadata-versus-endpoint credential retention, correlated pairing/error responses, protected credential round-trip/tampering, native WinHTTP registration against a loopback mock, and cancellation of a silent WebSocket. Native owned-window checks passed: empty state, add validation, optional real Windows display metadata selection (no display change), draft retention across navigation, default/minimum-size editor and fixed footer, save/reopen/rename, duplicate rejection, disabled check before pairing, and removal cancellation/confirmation. Original TV storage was restored; audio/display preset bytes and live display layout stayed unchanged. Test entries were temporary offline documentation addresses; no real TV was contacted.

Not validated: physical LG TLS certificate compatibility, unsigned permission manifest acceptance, on-TV approval/denial, or actual LG input responses. Not implemented: discovery/MAC learning, verified IP migration, Wake-on-LAN, input switching/power-off, configurable retry/timing, or scene readiness. MAC/TV identity must eventually be separate from the current connection IP; the user explicitly raised this during implementation. Existing monitor capability-cache and overview-drawer proposals remain unimplemented.


User hardware feedback (2026-11-01): the user reports successfully pairing the LG TV and seeing its input list. This establishes a reported successful pairing/input-enumeration path on that TV. The connection mode was not specified, so it does not independently establish encrypted-versus-legacy compatibility. Input switching, wake, power-off, and failure/reconnect behavior remain untested.


## Guided TV setup, MAC learning, and wake — 2026-11-01

This update supersedes the earlier manual-add and MAC/wake deferrals. Add TV now has seven native steps: name, address, pair/read, PC input, wake addresses, optional Windows display, and review/save. Pairing precedes final saving; successful pairing stays in the draft when input reads fail. Refused MAC reads permit manual entry or skipping wake. Back/navigation retain choices, while changing a new draft’s endpoint invalidates its pairing/read results. Existing TV editors can read and save input/MAC metadata without recreation. Old definitions retain their protected pairing through defaulted optional fields.

Wake sends standard magic packets directly through Rust UDP IPv4 broadcast in three rounds, using each distinct Ethernet/Wi-Fi/manual MAC. Results describe dispatch, not verified power. No input/display/audio action is applied by setup. Added shared tv learn and tv wake commands; CLI pair also attempts metadata learning while retaining successful pairing if later reads fail. Scenes, input switching, power-off, discovery, and verified IP migration remain unimplemented.

Validation: 58 passing Rust tests (55 library, 3 application), strict all-target Clippy, and offline release build. Tests cover guide gating/invalidation, MAC validation, partial/denied network data, old paired-definition compatibility, exact 102-byte packet contents, and three actual UDP sends to a loopback receiver. Existing WebSocket, DPAPI, storage, display, and audio checks pass. An additional ignored test-only in-memory WinUI fixture is explicitly launched for post-pairing visual checks; production has no fixture entrypoint.

Native release checks passed for name/IP/pair screens, Back/navigation/cancel, minimum-size footer, PC/MAC/display metadata save/reopen/remove, disabled Wake without MAC, and headless missing-MAC rejection before dispatch. The fixture exercised input choice, MAC entry/validation, optional display, review, and Back/Cancel at default/minimum sizes. Captures were inspected. Original TV file bytes were restored, original audio/display preset bytes and live display layout stayed unchanged, and no real TV was contacted or woken. Physical MAC reads, the complete revised guide with on-TV approval, and actual LG wake remain user validation work; earlier pairing/input-list success was user-reported.

## Manual MAC opt-in and read-only LG status — 2026-11-01

Added the native Enter MAC address manually switch in setup/editing, default off for new TV setup. Hidden draft text is preserved but does not block progress; saving while off removes only the manual target. Existing manual values reopen enabled. User reports the TV functionality works. An explicitly run read-only hardware test returned Active and two valid TV-reported network addresses (Ethernet and Wi-Fi), without any power/input/settings write. This test is ignored by normal runs and requires exactly one paired TV, with explicit user authorization to perform the check.

The normal 58 tests and strict Clippy passed, and the release build was updated. Native checks exercise initial hidden state, showing/hiding the field, retained draft text, disabled invalid validation while hidden, re-enabled validation, and existing manual-value persistence. Settings fixtures are restored; no physical power command was sent. A basic status/power/input modal is a recommendation, not implemented in this patch. The current power icon still dispatches Wake only.


## Basic TV control modal — 2026-11-01

Implemented native TV controls with fresh status/current external input, a single state-aware Turn off/Wake button, refresh, and an explicit Switch input action. The list’s Wake-only icon is replaced by a TV-controls opener. On, screen-off, and screen-saver states select Turn off; off, standby, and unknown select Wake, disabled without a saved MAC. Fresh state is checked before acting, while preserving the action shown to the user: stale Wake never turns off an already-on TV, and stale Off never wakes a TV that became off/standby. Unreachable/unknown does not authorize an off request.

The shared tv_control engine validates fresh inputs, avoids confirmed no-ops, sends explicit SSAP commands, and attempts bounded foreground-input readback. Accepted-but-unverified and ambiguous acknowledgment failures are reported honestly. Worker jobs check TV-definition freshness under the operation lock; snapshots/pending selections do not modify settings. A bounded post-action refresh updates the dialog. Close dismisses the dialog without canceling an issued operation; root window close protection continues until completion. Added CLI status/off/input; existing wake remains explicit dispatch.

Validation: 64 regular Rust tests (61 library, 3 application), strict all-target Clippy, format, and offline release build. New tests cover stale intent races, unknown/unreachable power, wake with unavailable status, fresh input validation/no-ops, accepted-versus-confirmed readback, exact command acknowledgments, and actual native WinHTTP power/input requests against a loopback mock. Ignored native fixtures are explicitly launched for visual checks and do not send real commands.

Native checks passed for default/minimum modal size, one power control, pending selection without automatic application, Unknown → Wake, stale-input disabling, refresh/error/Close, guide/editor metadata regression checks, and preservation of original TV/audio/display bytes and live Windows layout. A separate read-only hardware check via CLI and the release modal returned On, current PC, and all four HDMI inputs from the user’s LG; refresh worked. No real power-off/input-switch request was sent. Those physical command transitions remain user validation, while wire-level commands were mock-tested.


## Action icon and TV UI cleanup — 2026-11-01

Added shared remote/delete SVG assets staged beside the executable. The TV row now contains only remote controls, edit, and red-bin remove. Audio and display preset delete actions use the same red bin. Existing action names/tooltips/deletion confirmations are retained, and Apply’s green check/callbacks are unchanged. Removed redundant TV GUI Pair/Check handlers and the editor’s manual Read TV details button; backend/CLI commands remain available.

Paired-TV editing automatically reads available inputs/MACs on a worker, leaving pending metadata unsaved until Save. Unavailable data falls back to editable saved choices. Changing a saved address/method opens the existing setup guide at Pair; final guided saving retains the newly obtained credential for that endpoint. Duplicate checks exclude the record being reconfigured. Cancel preserves the original TV definition.

Validation: 64 existing tests, strict all-target Clippy, format, and offline release build pass. Native checks verified SVG images on audio/display/TV actions, absence of removed TV buttons, unchanged Apply controls, default/minimum layouts, metadata save/reopen/delete, and changed-connection Back/Next/Cancel without an early write. TV controls and setup fixtures still pass. Original settings were restored; no preset Apply or TV power/input write was performed. Foreground-owned recapture was used for final visual inspection after an occluded-window capture produced blank client pixels.


## Responsive TV controls — 2026-11-01

Replaced the outer modal scroll panel with a compact 340-DIP column and a shared input/Switch row. Unavailable inputs use a short placeholder. Local status replaces duplicated global InfoBars; diagnostics use a collapsed disclosure with bounded scrolling. Status reads no longer hold the mutation lock or global busy guard. Dialog tokens/read revisions ignore results superseded by commands, refresh, or closing/reopening. Wake dispatch is immediate and action reports arrive before the follow-up status read; fresh turn-off validation remains intact.

Validation: 65 regular tests pass, including wake-without-query and stale-read rejection. Strict all-target Clippy and formatting pass. Native in-memory fixtures cover On, pending input selection, a delayed unavailable response, Wake during checking, expanded diagnostics, default/minimum layouts, Close, and a late response after reopening. Original settings hashes and live display layout were preserved. No command was sent to the physical TV during this work. Release build updated for the existing desktop shortcut.


## Persistent diagnostics and Options — 2026-11-01

Added local rotating JSON-line logs for GUI/CLI startup/exit, background work, audio/display apply stages/results, and TV connection/authentication/request/acknowledgment/power/input/wake stages. UTC timestamps, process and operation IDs, elapsed times, and dialog token/read revision aid correlation. The bounded asynchronous writer coordinates rotation across processes; five files retain roughly 5 MiB total. Keys, credentials and raw TV payloads are excluded by logging only selected stage/result data. Logging errors are isolated from device operations.

A native Options footer/page contains Open logs folder. It uses Windows Local AppData and native folder opening. Command errors now remain separate from follow-up status notes in the TV modal.

Validation: 67 regular tests, strict all-target Clippy and format checks pass. Rotation/concurrent-write/escaping tests are included. The native TV fixture preserved an original blocked-command diagnostic across a later unavailable-status response and confirmed correlated log entries with no TV request sent. Default/minimum Options checks and native folder-open acceptance are exercised separately. Settings hashes and live display layout remain unchanged. No real TV power/input command was issued. Explorer's folder could not be independently inspected through Shell automation in this environment; folder-open verification is limited to Windows accepting the native request.


Folder-open follow-up: the user reported that dispatch-only success did not reliably open Explorer. Replaced the call with an explicitly COM-initialized STA worker and ShellExecuteEx/SEE_MASK_NOASYNC, returning its native failure rather than assuming that an asynchronous dispatch finished. Known folder existence and user permissions were checked without changing ACLs. Direct Explorer verification remains a separate check.

The final release passed 67 regular tests, strict Clippy, formatting and offline build. Headless CLI log/flush checks passed. The desktop shortcut was verified to point to the updated release executable. Options layout and shell-return handling passed native checks; the corrected folder action still requires the user's visible-Explorer confirmation because neither Shell enumeration nor UI Automation exposes Explorer in this test environment. No access-control permissions were changed.

The native shell handoff was not confirmed by the user. The current implementation instead starts Windows explorer.exe directly with the folder argument; status wording describes the request and includes the exact runtime folder/writer state. Windows file-service metadata confirmed that the desktop app and test tools were seeing different log contents at the same nominal path, explaining the apparent absence of desktop-process events in the agent's view. Do not treat the tool-side log as the user's live log without checking this boundary. The final executable update requires Director to be closed.

Final folder verification: the release was updated after Director closed. The agent opened the updated app, navigated to Options and invoked Open logs folder through UI Automation without moving the mouse. The runtime showed the expected Windows Local AppData log location, initialized logger and written records. The user explicitly confirmed that Explorer opened Logs. The final direct-Explorer implementation is therefore visibly confirmed; earlier shell handoff failures remain historical investigation notes. Director was left open on Options for review.

## TV power transitions and selective refresh — 2026-11-01

Added typed command effects, a separate GUI power-progress state machine, bounded wake retry scheduling, and power-first wake snapshots. Acknowledged Off is explicitly assumed and never inferred from a lost connection. Confirmed input changes update in place without a full modal read. Errors and unconfirmed input results reconcile once, preserving the original diagnostic. Close/reopen invalidates both queued timers and completed reads. Command worker panic/delivery handling returns through normal completion.

Validation: 70 regular tests, strict all-target Clippy and formatting pass. Core tests cover no input queries before awake, monotonic retry/expiry, manual recovery and assumed-Off semantics. Native in-memory fixtures exercise confirmed input without status reload, Turning off, assumed Off, Waking through an unavailable response, confirmed awake, expiry, retry, minimum-size layout and stopping checks after close/reopen. Settings hashes and live display layout remain unchanged. No power/input request was sent to the physical TV. The retry window is 30 seconds; an already-running native request remains subject to its own bounded timeouts.


## Optional TV tip in new display presets — 2026-11-01

Added a restrained inline card with Add TV, session dismissal and persistent Don't show again. It waits for preferences/catalog loading and is suppressed for existing TVs and existing-preset edits. Add TV opens/resumes the guide while keeping the unsaved display draft. Shared monitor cards show linked TV names and View TV. Options contains Show TV setup tips; the preference lives in versioned per-user preferences.json with atomic/stale-safe writes.

Validation: 71 regular tests, strict all-target Clippy, formatting and offline release build pass. A native release check covers the default/minimum layouts, dismissal, persistent hide/restart, Options restoration, guide navigation, preserved draft name and linked-TV navigation. Final screenshots were recaptured with the owned window visible after an occluded capture returned black client pixels. Original TV/preferences bytes were restored; preset files and live display layout stayed unchanged. No TV was contacted and no display was enabled/disabled. Temporary capability-detection setup is not implemented in this pass.


## Inline temporary display detection — 2026-11-01

Implemented the previously deferred setup flow in display presets. Detection is explicit, uses inline explanation/progress/cancel, and optionally prepares a linked TV. A backend-independent transaction restores and verifies the original desktop after success, cancellation or failure, including partial writes/panics and TV arrival. Failed restoration retains a retryable snapshot and blocks draft Save/Cancel. Confirmed active capabilities are remembered by physical device path and become draft choices only after restoration. The target is enabled in the draft, not left enabled on the live desktop. Inactive fallback selectors are hidden until a usable inventory is available; cached offline choices retain exact refresh fractions and are revalidated on Apply.

Validation: 78 regular tests, strict all-target Clippy and formatting pass. Native simulated-display checks cover explanation, optional wake stage, reading/restoring progress, cancellation and resulting 4K/120 Hz/HDR controls at default/minimum window sizes. Unit coverage includes failed/partial activation, inspection failure/panic, cancellation before setters, failed TV preparation, restoration retry and cache corruption/precision. All original settings/cache bytes and the live Windows layout stayed unchanged. No physical TV/display was activated during these checks. Real LG/driver activation remains hardware validation. See DISPLAY_SETUP.md for cancellation/deadline/crash boundaries.


Display detection now uses CCD preferred dimensions and the highest exact reported refresh at those dimensions for the draft, rather than copying the temporary fallback desktop. Explicit detection replaces supported fallback selections while preserving position/orientation. A regression test covers virtual larger modes, fractional refresh and a supported 640x480 fallback.

Cold HDR preflight now defers unreadable inactive targets until after activation, records their live retained HDR state before writes, and restores it before disabling during rollback. Detection seeds the native pixel dimensions, and logs its actual active probe. Native plan regression covers inactive HDR vs unreadable active HDR; physical LG/driver verification remains necessary.

Scoped TV power subscriptions use the existing WinHTTP transport and pinned authentication, cancellation-aware WebSocket watchdog, and worker-to-WinUI event queue. Wake retransmissions are capped and guarded, stale events/readbacks are invalidated, and closing controls cancels listeners and queued retransmissions. CLI semantics and Screen off/on control scope are unchanged.

Live-TV validation attempt: the TV entry visible to the tool session had no wake address and its endpoint did not answer. No wake packets were sent; the cleanup turn-off request could not pass fresh-status preflight. This is not a successful hardware test. Mock transport and native event/transition fixtures passed.

Physical LG C1 follow-up passed: Windows' file provider exposed the desktop TV record with verified wake addresses, while ordinary reads in the tool context saw an older record lacking them. Temporary wake metadata was used without changing either saved record. The real native SSAP subscription reported On, acknowledged shutdown produced a reported Standby transition, and the subscription recovered to On after spaced external WOL dispatches. The final explicit Turn off command was acknowledged by the TV. Temporary protected settings copies were removed. This validates real authentication/subscription/reconnection; GUI wake-retry scheduling remains covered separately by unit/native fixtures because the tool-visible saved entry lacks wake metadata.

Preset list rows were replaced with shared native preview cards. Mirroring supports join/detach, reported common resolutions, shared orientation/position and group dragging, independent reported target refresh rates/HDR intent, group-aware primary handling, and atomic rejection of incompatible changes. Detaching/re-enabling allocates unused logical source keys so outputs cannot accidentally become clones. The existing native apply planner validates source routing and verifies the applied grouping, restoring on failure. Successful unsupported-HDR readings can satisfy an explicit HDR-off request; API errors and requested HDR-on remain strict.

Validation includes draft/group/drag/source-allocation regressions, native clone-path planning and missing-route rejection, and owned-window fixtures for gallery layout, metadata previews, mirroring, detach, primary changes and wide/narrow presentation. No real monitor topology was changed in these checks; driver-specific mirror combinations require hardware validation.


Optional audio routing is implemented in the shared GUI/CLI transaction engine: default three-role output, optional communications override, optional microphone and input volume. Legacy presets deserialize with both optional fields disabled; applying them now follows the user-approved three-role default. No existing preset file was rewritten during implementation. Tests cover missing optional-device preflight, untouched capture when disabled, six-role/two-volume rollback, and serialization compatibility. Native fixtures exercise hidden/revealed fields and cancellation without live audio writes. Automatic TV/audio association remains a proposal, not implemented by this change.

## Scene/link validation — 2026-11-03

The current build passes 103 regular tests (94 library and 9 native-state tests), strict Clippy and formatting. The owned-window scene/link fixture passed create/edit/delete, TV/display/audio selections, action reordering, draft preservation and unavailable audio-link recall at wide/narrow sizes. Inspected actual native captures. It did not write real settings, mutate audio/displays, contact the TV or dispatch apps/URIs. The release build completed and director scene list exited successfully without opening a window or leaving a process running. Physical end-to-end HDMI readiness and recovery remain unvalidated. No commit or push was performed.

## Samsung preview validation — 2026-11-03

111 regular tests pass (102 library, 9 native state), plus owned native UI and loopback TLS fixtures. Strict Clippy and formatting pass. Native checks cover brand/setup/pairing, required different-input precondition, No/Yes confirmation, failed-retest invalidation, saved PC input, redacted report content, and capability filtering in the modal and scene editor at wide/narrow sizes. Synthetic WebSocket tests cover direct frames and denial; TLS tests prove matching-pin command success and zero authorization-bearing headers reaching a mismatched-certificate server. These fixtures do not contact a real TV or mutate live Windows audio/display state/settings. Physical Samsung pairing/wake/off/HDMI compatibility awaits the friend’s report. LG protocol/storage regression tests pass; no physical LG commands were sent.

## TCL Android / Google TV preview validation — 2026-11-03

119 regular tests pass (110 library and 9 native state), strict Clippy passes, and owned native TCL/Samsung guide fixtures pass. Loopback mutual-TLS checks cover complete code pairing, DPAPI round-trip, feature negotiation, keepalive, reported power, one direct HDMI frame, changed-server-identity refusal before commands, incorrect code rejection, and partial-frame deadline enforcement. Native UI checks cover brand choice, blank/invalid code gating, Back/restart clearing the code, HDMI preconditions, Yes/No confirmation, failed-retest invalidation, saved PC input, report privacy and modal/scene capability filtering at wide/narrow sizes. Inspected native captures. No physical TV was contacted; fixtures do not change real settings or Windows audio/displays. Physical TCL compatibility, standby networking and clean-PC distribution remain for the friend to validate. No commit or push performed.

## Friend guides and report validation — 2026-11-03

Samsung/TCL guides now contain six nontechnical steps and name the single JSON file to send. Developer detail was preserved in SAMSUNG_TECHNICAL.md and ANDROID_TV_TECHNICAL.md. Compatibility format 2 adds explicit tester observations, optional model/software/network information, export time, Windows build, architecture, executable SHA-256, wake configuration flags/counts and typed per-TV action history. Pairing pages permit report export before pairing succeeds. History omits raw text, credentials and device addresses; retention/unavailable data are explicit. 121 regular tests (112 library, 9 native state) and strict Clippy pass. Owned Samsung/TCL native fixtures cover failed-pairing export, sanitized timeout/system code, input checks, manual feedback, runtime fields, saved selection and hidden unsupported controls at wide/narrow sizes. No real TV was contacted and fixture settings/audio/display state remain unchanged.

## NVIDIA/RTSS FPS validation — 2026-11-03

126 regular tests pass (117 library, 9 native state), strict Clippy and formatting pass. Pure tests cover partial-write recovery, preserved RTSS enabled state, explicit keep-on-failure, disable versus unchanged semantics, legacy scene loading, FPS-only scenes and launch ordering/recovery. Native read-only discovery finds both installed methods. An explicit 61 FPS native round trip passed on NVIDIA and RTSS; each actual configuration was read back and the exact original snapshot restored and verified: NVIDIA inherited/default zero, RTSS 115 with limiter enabled. The owned native UI fixture passed FPS-only scene create/edit, NVIDIA-first selection, disable/unchanged distinction, direct RTSS Current setup application, and wide/narrow layouts. It performs no real writes and preserves settings/layout. Physical game FPS enforcement, app-specific overrides and running-game restart behavior remain manual validation. No TV, audio or display change was performed.

Release build completed. Headless fps status and --help passed with explicit process exit checks and no window. Final release readback confirms NVIDIA 0/inherited and RTSS 115/enabled remain restored. Canonical desktop executable updated; actual game FPS validation remains for the user.
