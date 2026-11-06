# Design decision record

Planning began: 2026-10-30. Implementation authorized: 2026-10-31. "Accepted" means agreed direction, not completed functionality. See IMPLEMENTATION.md for current code and validation.

## Accepted

| ID | Decision | Reason/context |
| --- | --- | --- |
| D001 | Rust application logic and CLI; native WinUI 3 GUI through Microsoft Windows Reactor | Native compiled desktop application without a Chromium frontend; prioritize speed and low resource use |
| D002 | Shared execution engine; CLI application without GUI initialization | Identical behavior from the window and automation |
| D003 | Independent display and audio presets, optionally composed into scenes | Users must not need to bind audio and displays at preset creation |
| D004 | Audio input, output, and volume can be controlled independently | Supports input-only, output-only, and volume-only requests |
| D005 | Capture current monitor layouts first; create audio presets with dropdowns and optional volume controls; guided creation later | Audio capture is excluded from the initial workflow; establish basic workflows before expanding creation tools |
| D006 | Remember observed audio devices while unavailable | TV HDMI audio may appear only after the display transition |
| D007 | Prefer Windows Stable ID when available, fall back to endpoint ID, allow explicit relinking | Names are editable/non-unique and device identities can change |
| D008 | A device must be enumerated at least once before selection; show identity and Windows name | Bind presets to real observed devices, not typed names |
| D009 | Ordered optional final executable and URI actions | Support Steam Big Picture and other user-selected apps after configuration |
| D010 | Native Windows system notifications | Report background/CLI results using the system notification experience |
| D011 | Selectable failure policy; default revert, alternative stop and keep completed changes | Recover previous state by default while allowing user choice |
| D012 | Defer tray mode and starting with Windows until after basic functionality | Keep the first implementation focused |
| D013 | Exclude CRU and SpotBye | Explicitly outside the replacement application's scope |
| D014 | Keep the private checklist and repository-local machine files under .localonly/, excluded by the machine-wide Git rule; keep .gitignore limited to build output | Keep the private backlog local while documenting agreed design in repository files |
| D015 | Use Director as the app name and Director as the local repository folder name | User-selected working name; keep documentation and command examples consistent |
| D016 | Scene is the top-level bundle; presets hold saved settings; actions perform commands | Use display/audio presets and TV, frame-limit, executable, or URI actions consistently throughout the app |
| D017 | Fixed scene stages: Display → Audio → Apps & URIs; reorder only within a stage | Displays may introduce audio endpoints, so configure displays before selecting audio; launches come last |
| D018 | Capture current layout applies only to monitors; audio devices are selected in dropdowns with an optional Set volume percentage switch per selected device | Device/volume edits configure a preset; only Apply changes the actual device settings |

| D019 | Minimize external applications by implementing operations directly through OS APIs, device protocols, or linked libraries wherever practical | Director must replace helper executables rather than wrap them; document unavoidable dependencies separately from intentional user launch actions |

| D020 | Keep device discovery, preset/scene execution, waits, verification, and recovery off the UI thread | The native window must stay responsive; background execution reports progress/results through the UI event loop and still serializes changes |

| D021 | GPU-driver frame limiting is primary; RTSS and future external limiters are optional alternatives. Use a Set FPS limit switch, Method dropdown, and FPS value | Prefer supported driver methods, leave settings unchanged when omitted, and validate each provider before claiming support |

| D022 | Initially change Default audio only: Console and Multimedia together. Defer Communications controls and explicit changes | Present familiar user-facing behavior and keep the first audio feature focused |

| D023 | User authorized the first audio-output implementation; keep documentation updated and the README clear and user-focused | Start with output dropdown, optional volume, direct Windows Apply, verification/recovery, and a responsive native window |

| D024 | Audio presets is the main view; Add/Edit uses an editor page with Name and Save/Cancel; saved rows offer Apply/Edit/Delete | The prototype device controls belong in an editor. Saving stores choices without changing audio; only Apply executes them |

| D025 | Keep machine-specific user settings under the Windows Local AppData known folder, in one Director subfolder | Audio endpoint IDs belong to this PC. Resolve the standard folder through Windows; keep settings out of the repository, Documents, Desktop, and home-folder root |

| D026 | Replace the prototype layout with a compact workspace shell and selectable preset rows; use plus/Add, green check/Apply, pencil/Edit, and red cross/Delete with hover/focus states | User requested a designed main view with compact actions instead of separate text-button blocks per preset. Preserve the worker, storage, CLI, confirmation, and audio behavior |

| D027 | Use WinUI 3 through Microsoft Windows Reactor as the only frontend; promote it to the main director executable and remove the previous frontend and parallel package | User compared the native controls and explicitly selected WinUI 3 on 2026-10-31. Keep the shared Rust engine, CLI, settings schema, and background execution |

| D028 | Implement display presets as capture/save/view/rename/delete first; independent from audio and without a display Apply operation | User authorized capturing the current layout before implementing restoration, confirmation/reversion, or advanced controls. Preserve native UI behavior and existing settings |

| D029 | Add native display Apply with validation, live snapshot, strict readback, a worker-owned 15-second Keep/Revert deadline, and automatic restoration | User authorized Apply and confirmation/reversion. Persist only verified confirmed changes; headless application uses the same engine and an explicit --yes automation option |

| D030 | Adopt native per-monitor editor cards/map, visual Identify, explicit clone controls, later DPI scaling, headless scene shortcuts, later launch/restore-on-exit, and optional descriptions/icons | User accepted the comparison-inspired additions as backlog scope; wallpaper remains optional and implementation milestones stay separate |
| D031 | Rotation choices show a rotated display SVG with a top/up marker and a plain-language orientation label; degrees are secondary | User explicitly requested visual rotation choices because degrees alone do not communicate orientation |

| D032 | Save a per-display-preset Ask to keep display changes preference, default on; CLI follows it with --yes/--confirm overrides | User wants to avoid repeated confirmation for trusted layouts. Opt-out skips only the prompt, preserving verification and failure recovery |
| D033 | Keep LG and Samsung projects as integration research references; Samsung support is deferred | User supplied lgtvcli and samsung-tv-control. No dependency, installation, cloud integration, or current Samsung implementation commitment |

| D034 | Add Create preset and Customize layout with native per-monitor controls, supported modes, visual orientations, numeric placement, and a read-only preview | User requested custom creation; Save stays separate from Apply. Clone-group editing, drag/Identify, and advanced color/scaling remain deferred |

| D035 | Use direct display Edit, a boxed draggable map with screen-detail tooltips, cohesive + toolbars/footers, and disabled primary enable control with a reason | User explicitly replaced Customize, numeric placement fields, and mismatched button positions. New GUI/CLI display presets default confirmation off; preserve existing choices. Supersedes the earlier default-on and preview-only parts of D032/D034 |

| D036 | Remove visible preset Refresh buttons; compact and align orientation choices, remove icon arrows, and fix supported mode discovery | User requested cleanup. Clean page navigation already reloads data; match device paths without stored adapter LUIDs and retain unspecified DXGI scan-line modes |
| D037 | Add optional HDR state through HDR-specific Windows device-info APIs, verification, and recovery | Capability-gate the toggle, preserve legacy leave-unchanged behavior, and reject changes when prior HDR cannot be read |
| D038 | Add Current setup as an alternative combined display/audio page without preset persistence | User clarified active setup only, excluding appearance themes. Stage controls and Apply each category independently; share the editor/backend and preserve preset drafts |

## Proposed or unresolved

| Topic | Current proposal or open question |
| --- | --- |
| Scene references | Reference component IDs; edits propagate to dependent scenes; duplicate for independent variants |
| Rename/deletion | Editable names with stable internal IDs; prevent deletion that leaves dangling references |
| CLI | Audio presets now use `director audio apply "Headphones"`; display check/apply now exists; future scene command syntax remains open |
| Final-action gating | Skip launches if required configuration fails; determine behavior on a failure inside the final-action chain |
| Frame-limit implementation | Validate direct NVIDIA/AMD/Intel controls, capabilities, and modern limiter mappings; primary driver selection and optional external providers are already agreed |
| Additional configuration | Exact placement of frame-limit actions within the configuration stages; keep them before final launches |
| Action completion | Dispatch only initially, or optionally wait for executable exit; URI readiness cannot generally be established |
| Policy overrides | Whether failure behavior can be overridden per scene or CLI invocation |
| Notifications | Failure notifications by default, optional success notifications and quiet CLI flag |
| Display confirmation | Implemented 15-second native Keep/Revert deadline and per-preset opt-out; configurable timeout remains open |
| Operation ownership | GUI/CLI coordination, busy-request handling, cancellation, and interrupted operations |
| Reapplication | Reapply all steps versus skip verified unchanged state; optional toggle behavior |
| Additional scope | HDR, color depth, VRR, DDC, other TV brands, hotkeys, triggers, arbitrary scripts, import/export |
| Guided creation milestone | Required before 1.0 or later release |
| Distribution | Windows baseline, portable/installer model, update behavior, executable arrangement |
| Licensing | App license and third-party license notices |
| Performance | Measured budgets for startup, idle CPU, memory, and CLI startup |

## Working rule

Update this record when a decision is accepted or changed. Update [requirements](REQUIREMENTS.md) and the [architecture proposal](ARCHITECTURE.md) where affected. Keep unresolved proposals visibly separate from accepted behavior.

### Proposed next feature: remembered monitor capabilities

The user clarified that the powered-off LG report was a design question, not authorization to change inactive-TV behavior. The attempted inactive-mode filtering was reverted before release. Proposed handling: learn exact mode pairs and capabilities while the target is active; retain that last verified data while it is unavailable; label remembered choices; never overwrite a complete verified inventory with an off/standby/partial list. On arrival, re-enumerate and validate the selected mode before Apply. Missing or incompatible targets should produce a bounded wait/failure and use the selected recovery policy instead of silently substituting a lower resolution. Identity, cache invalidation, and first-observation behavior need agreement before implementing this feature.

### D039 — TV setup and optional display association (2026-11-01)

The user requested a native TV view for adding a controllable TV and asked whether it should link to a Windows monitor. Implement independent remembered LG TV definitions, explicit pairing/checking, and an optional Windows physical-display-path association. The association is metadata for later coordination, not a display-preset reference or current readiness signal. User questioning IP-based identity establishes the need to distinguish remembered TV identity/MACs from current connection IP; verified rediscovery/address migration remains proposed rather than silently guessing another TV. The first conservative endpoint-change pairing reset is documented. See TVS.md for actual implemented scope and unverified physical-device behavior.

### D040 — Guided TV setup, MAC learning, and wake (2026-11-01)

The user authorized a step-by-step Add TV guide, automatic MAC learning, and wake. Implement name → IP → pair/read → PC input → wake-address review/manual fallback → optional Windows display → review/save. Pair before saving and retain a successful pairing if later reads fail. Learn only TV-reported wired/Wi-Fi MACs, not a gateway/neighbor MAC. Existing TVs can read/store metadata without recreation. Provide direct GUI/CLI Wake-on-LAN dispatch with truthful results. Store PC/display connections as metadata; action choices, order, and readiness belong to future scenes, independently of pairing.

### D041 — Advanced manual MAC entry (2026-11-01)

The user requested an opt-in switch before showing/typing a manual MAC. Implement Enter MAC address manually, off for new setup and on when editing an existing manual address. Preserve draft text while hidden; ignore disabled manual validation and clear its saved target on Save/Next when off. Automatic TV-reported learning remains the default.

Proposed next UI, not yet implemented: a small TV control modal with fresh status, Wake/Turn off, and input selection. The user asked whether the power control should turn an already-on TV off. Reported state supports this design; the existing Wake-only action is unchanged. Keep scene/CLI power actions explicit rather than toggling on each invocation.

### D042 — Basic TV control modal and one power button (2026-11-01)

The user approved a basic status/power/input modal and explicitly requested merging power controls into one state-aware button. Implement a TV-controls opener in the list, fresh status/current input, a single Turn off/Wake button, and explicit input selection plus Switch. On/awake states choose Off; Off/standby/unknown choose Wake with a known MAC. Recheck fresh state while honoring the displayed intent rather than reversing a stale click. Unknown is not Off. CLI/scenes retain separate wake/off/input operations. Setup remains connection metadata, without scene action preferences. Volume and full remote controls remain deferred. Panel-only on/off is not planned; see D049.


### D043 — Remote/bin SVGs and fewer TV maintenance buttons (2026-11-01)

The user requested a remote-control SVG instead of a screen for TV controls, removal of Check connection/Pair again/Read TV details buttons, and a red bin instead of red X across the app. Apply stays unchanged pending a later decision. Share SVG action assets across all relevant views. Keep automatic setup reads; refresh paired-TV editor details in the background and retain offline editability. Changed endpoints continue through pairing setup without prematurely overwriting the stored TV. CLI maintenance commands remain available.


### D044 — Immediate, compact TV controls (2026-11-01)

The user reported an oversized, clipped modal with a scrollbar crossing its controls, duplicated timeout diagnostics, misleading success wording, and a delay when an unreachable TV was checked. Open immediately and check independently in the background. Keep Wake and Close usable during reads; retain mutual exclusion and fresh checks for mutations. Wake dispatch is idempotent and does not need a preliminary network query. Show action completion before the separate status refresh. Ignore old read responses by dialog token/revision. Use compact native controls, hide unavailable input selectors, and disclose technical errors on demand rather than duplicating them on the page. Do not equate unreachable with off or add permanent polling.


### D045 — Local operation diagnostics and Options (2026-11-01)

The user approved persistent rotating logs and requested that Open logs folder live in Options with other settings actions. Add a native Options footer/page and open the known Local AppData log folder through Windows. Share structured logging between GUI and CLI, recording UTC time, process/operation IDs, step outcomes, elapsed durations, and safe diagnostic errors. Use a bounded asynchronous queue and cross-process rotation lock; retain five 1-MiB files. Never record credentials or raw TV payloads. Preserve command diagnostics separately from a later status check so investigating a power failure does not lose its original cause. No remote upload or resident service is added.


### D046 — Command-aware TV power transitions (2026-11-01)

The user approved Waking with a bounded readiness wait, Turning off followed by an explicitly assumed Off after acknowledgment, and selective rather than unconditional refresh. Keep the modal local state distinct from reported TV power. Poll only during wake, initially reading power alone; refresh inputs when awake. Use a 30-second retry window, two-second spacing and existing bounded native request deadlines. Stop retries when closed, superseded, awake or expired; keep Close available. Use typed command effects, update confirmed input selection directly, and reconcile only failures/uncertain results or explicit refresh. Keep command errors intact, do not interpret network failure as confirmed Off, and do not change CLI wake semantics or add resident polling.


### D047 — Optional TV setup guidance (2026-11-01)

The user approved a small dismissible TV reminder during new display-preset creation, with Add TV and Don't show again, and asked for a restrained appearance. This implementation pass is limited to that guidance and draft-preserving TV navigation; temporary display activation/capability discovery remains separate future work. Use an inline neutral native card, not an interrupting dialog or warning. Show it only when preferences/catalog are loaded and no TV is already saved. Close dismisses for this session; Don't show again persists a per-user preference, reversible in Options. Add TV opens/resumes the existing guide without discarding the display draft. Linked monitor cards show the saved TV name and View TV navigation. No action in this guidance changes displays or contacts a TV.


### D048 — Inline temporary display detection (2026-11-01)

The user authorized the complete display-detection workflow after accepting inline progress instead of a loading modal. Explain the temporary change in the selected monitor card and require Detect options before any setter. Provide explicit optional Wake and detect only for a linked, paired TV with wake/PC-input metadata. Snapshot the current layout before TV preparation, preserve existing active screens/primary, temporarily enable the target without persisting the topology, discover active capabilities, then restore and verify before publishing options. Cancellation, deadline, partial setter failure, inspection errors and panics must also pass through recovery. Keep a retryable snapshot when restoration fails and block saving/discarding that draft until restored. Cache only complete results by device path and label remembered offline choices. The retry deadline is cooperative; normal close protection is not a crash/force-kill watchdog.



### D049 — Screen blanking removed from future scope (2026-11-03)

Screen off/on or panel-only blanking is not a planned or deferred feature. Reconsider it only after an explicit user request. Reading and displaying a TV-reported Screen Off power state remains valid and does not add a blanking control.


### D050 — Optional communications output and microphone

The user superseded the earlier communications deferral: output presets assign all three Windows roles to the main output by default. An optional separate communications output reveals its own picker and overrides only that role. Microphone settings remain untouched unless Change microphone is enabled; selected capture roles follow that input and its volume remains independently optional. Preserve the same semantics in GUI/CLI and full transactional preflight/recovery. TV-to-HDMI-audio association is a metadata proposal requiring verified mapping or explicit confirmation.

### D051 — Remembered TV audio and scene execution (2026-11-03)

The user authorized linking and scenes. Implement explicit endpoint selection as optional TV metadata, preserving unavailable links and leaving switching to an audio preset or scene. Do not infer identity from matching names/shared GPU. Scenes compose stable references with optional TV readiness/input, displays, audio and ordered app/URI dispatch. Share one locked GUI/CLI runner, default to actual Windows-state recovery, retain an opt-in keep-completed policy, and clearly exclude TV/app side effects from automatic recovery. Screen blanking remains outside scope.

### D052 — Samsung direct commands only (2026-11-03)

The user rejected custom key sequences. Future Samsung integration must use direct supported commands, without menu navigation sequences or silent macro fallback. ColorControl is an additional protocol reference; model-specific support remains to be validated. See INTEGRATION_REFERENCES.md.

### D053 — Hide unsupported TV controls (2026-11-03)

The user requires unsupported TV controls to be omitted from the UI, rather than displayed as unavailable/disabled buttons. Apply this to the TV control panel and scene action choices. This differs from a supported action temporarily busy or disconnected, which still needs truthful operation feedback. Samsung remains unimplemented; the proposed initial scope is direct local pairing, remembered authorization, Wake-on-LAN, reported power state and guarded turn-off. Direct input switching requires model-specific verification and no sequence fallback.

### D054 — Samsung implementation and friend compatibility testing (2026-11-03)

The user authorized Samsung implementation with the optional setup test/report flow. Pair locally by selected brand, preserve existing LG records, use direct commands only, and expose Samsung HDMI controls only after a sent test and explicit user confirmation for the current model/firmware. Failure or unconfirmed results keep controls hidden; fresh metadata changes invalidate confirmations. Export only an allowlisted redacted report, with no automatic sharing. Samsung remote commands are Sent rather than TV-acknowledged. Report actual power fields, guard KEY_POWER with fresh state and keep Unknown honest. Physical Samsung validation is required before claiming model compatibility.

### D055 — TCL Android / Google TV friend preview (2026-11-03)

The user approved TCL as the last additional brand in this round. Support the Android TV Remote v2 platform, code pairing, explicit wake/sleep, reported power, single direct HDMI tests with visual confirmation, redacted reports and existing scene/display/audio integration. No ADB, custom sequences, generic remote panel or screen blanking. Android TV service version must not be presented as confirmed TV firmware. Hardware compatibility remains pending the friend’s test; no claims for TCL Roku/Fire TV.

### D056 — Readable friend guides and self-contained test reports (2026-11-03)

The user requested short nontechnical Samsung/TCL instructions and a report with sufficient diagnostic context. Keep the friend guides focused on adding/pairing, direct HDMI, physical power checks and sending one JSON. Move implementation details to separate technical notes. Report explicit tester observations without assuming command success; collect typed sanitized per-TV history, runtime/build context and wake configuration, never raw logs or credentials. Report export must work on failed pairing as well as successful setup/Edit.

### D057 — NVIDIA/RTSS global FPS controls (2026-11-03)

The user approved global NVIDIA and optional RTSS providers, scene placement after audio and before launches, and direct Current setup/CLI application. Switch off means unchanged; disabling the global cap is an explicit selected action. Never fall back to another provider or alter competing limiters/game-specific overrides. Use direct installed DLL interfaces, snapshot/readback/restore and the existing scene failure policy. Driver configuration verification is distinct from measuring a game. Per-game targets remain later work.

### D058 — Scenes / Now / Library navigation (2026-11-06)

The user found the flat navigation (Scenes, Current setup, Audio presets, Display presets, TVs) disorganized and chose grouping into three areas. Scenes becomes the start page, Current setup is renamed Now, and the presets and TVs sit under an expandable Library item. Now can seed new Library presets, and the scene editor can create a missing preset or TV in place and return with it selected. Storage formats, CLI names and scene execution are unchanged.

### D059 — Step-flow scene editor, Before/Leave actions and global failure policy (2026-11-06)

The user found the scene form indistinguishable from Now, the side summary redundant, and per-scene recovery settings out of place. Scenes are now edited as a vertical flow of step blocks. Scenes gain Before actions (apps can wait for completion) and When leaving actions, plus a Close app action. Leave actions run when another scene is applied or the active scene is ended. Director remembers the active scene across the window and the CLI until the PC restarts. "Keep completed changes" and the device wait became one global Options setting shared by scenes and audio presets. The scenes file moved to version 2; version 1 still loads.

### D060 — VRR-safe FPS cap, linked TV audio volume and a Windows 11 title bar (2026-11-06)

The user asked for a VRR-safe FPS cap and chose the Reflex-style formula refresh − refresh²/3600, read from the primary display when applied (after a scene's Displays step). The FPS editor shows only the selected method's current value and names its choices "Set a frame rate limit" and "Remove the global limit". A scene's linked TV audio can now set a volume. The app name and pane toggle moved into a WinUI 3 TitleBar, so the navigation pane starts with the pages.

### D061 — TV off when leaving, End beside +, shortcut commands and Save only on change (2026-11-06)

The user wanted scenes to switch the TV off, chose a switch in the Prepare TV step that shows a locked "Turn off" card under When leaving, and asked that End appear only when leaving does something, next to + as a global action. Cards gain a ">_" button with copyable commands by name and by ID (for Stream Deck and shortcuts, using `director` on PATH); display and audio CLI commands accept IDs. The audio editor no longer shows the device ID. Save is enabled only for changed, valid drafts.

### D062 — Local crash reports offered as GitHub issues (2026-11-06)

Opening a scene step with actions closed the window: the pinned Reactor rejects tooltips inside a ContentDialog, and nothing recorded the failure beyond one log line. The user asked why crashes aren't collected so people can report them. Director now saves a local crash report for panics and fatal window errors and offers it once on the next start. Report on GitHub opens a pre-filled issue in the browser, so nothing is uploaded without the user submitting it; the log lines stay in the file for the user to attach. A test keeps tooltips out of dialog content.

### D063 — Onboarding starts from the current setup (2026-11-06)

The first-run checklist sent people to Now and to each Library page before they knew what a scene was, and its buttons were mismatched page names. The user asked for a better start that explains a scene for the everyday setup. The empty Scenes page now saves the current screens, sound and FPS limit as one scene (checkboxes, default name Default), reusing matching presets and changing no device, then shows a dismissible tip for making the next scene in the step editor, which can create presets in place.

### D064 — Windows 11 24H2 is the minimum (2026-11-06)

Director declared no Windows support: no app manifest (so version APIs reported 6.2.9200), no installer check and no documented baseline. HDR switching needs display-configuration requests added in Windows 11 24H2, and only current Windows 11 is tested, so the user chose 24H2 (build 26100) as the minimum rather than a partly working Windows 10 install. The executable embeds a manifest (Windows 10/11 supportedOS, maxversiontested 10.0.26100.0, per-monitor v2 DPI), and setup.exe and the MSI refuse older builds.

### D065 — CLI names, and `list` in every area (2026-11-06)

Commands found items by name ignoring case, so any name with a space needed quotes, and the shortcut dialog copied commands such as `director scene apply "TV ""Gaming"""`. Lists were also inconsistent: `audio list` printed devices while `scene list` and `tv list` printed saved items, display presets were under `display presets`, and there was no way to see everything at once.

Every scene, preset, TV, screen and audio device now has a CLI name derived from its name: lowercase letters and digits, with each run of other characters replaced by one dash ("Desk View" is `desk-view`). It is computed rather than stored, so no file format changed and renaming updates it. Lookups accept an exact ID first, then anything that normalizes to the same CLI name, so older quoted commands keep working. Saving a new or renamed item whose CLI name is empty or already used by another item of the same kind is refused (the user chose this over auto-numbering, which would let names shift as items come and go). Existing files with a clash still load; looking up the clashing name reports both IDs.

`director list` prints every category, and each area has `list` for its saved items: `scene list`, `display list`, `audio list`, `tv list`, plus `display screens`, `audio outputs` and `audio inputs` for devices. All take `--json` with a `cli_name` field. The user accepted that `audio list` now lists audio presets instead of output devices (use `audio outputs`); `audio presets`, `display presets` and `display current` remain as aliases. D066 later retired audio presets: `audio list` prints outputs and inputs, and `audio presets` was removed.

### D066 — Live sound controls; scenes hold their sound; audio presets retired (2026-11-06)

Audio mixed two jobs. `audio apply` took either a preset or a raw endpoint ID with `--volume`, Now ran a live change through the full preset transaction behind an Apply button, `audio volume` was the only real live command, and nothing could mute. Scenes pointed at audio presets by ID and also carried a separate `tv_audio` / `tv_audio_volume` special case.

The user wants to change the active device and volume easily without a preset, and have scenes keep their sound. Sound is now split in two:

- **Live controls** (`audio_control`) change one thing right away: make a device the default (all roles, or one), set or step the volume (35, +5, -5), and mute, unmute or toggle, for the output or the microphone. No device means the current default. Each change is verified and put back when Windows does not confirm it. Now's Sound card uses them directly (no Apply button; sliders send their level once they rest), and so do `director audio status | use | volume | mute | unmute | mute toggle` with `--mic`.
- **A scene's Sound step** holds its settings inline: an output device or a TV's sound, volume, mute, an optional calls device and an optional microphone with its own volume and mute. It replaces both the audio preset reference and the linked TV audio fields, and runs through `audio::apply` as part of the scene's transaction, which now snapshots and restores mute too.

The user chose to retire audio presets rather than keep them as an optional choice: a scene with only a Sound step does everything a preset did, including Stream Deck use through `director scene apply`, and sharing one sound setup across scenes was rare. The Library Audio page, `audio apply`, `audio presets` and the scene editor's "New audio preset…" are gone; Now's save button opens a new scene instead. Breaking changes were acceptable because nobody else uses the app yet.

Scene files move to version 3 and backups to version 2. Version 1–2 scenes migrate when they load: an audio preset reference becomes the preset's output, volume, calls device and microphone (read once from `audio-presets.json` or, for a backup, from the backup's own presets), and linked TV audio becomes the TV's sound with its volume. A preset that can't be found leaves an empty Sound step that the scene card reports. Scene cards show the scene's main change at the top, so a sound-only scene shows its output and volume rather than an empty display box. Settings-file helpers moved from `presets.rs` to `storage.rs`.

### D067 — TV input protection runs beside the scene (2026-11-06)

TV boxes on HDMI-CEC take the input while a TV wakes, even when they didn't wake it. The user wanted the scene to win without waiting for the fight to end. After the TV reports the scene's input, a background guard (`tv_input_guard`) polls it for a configurable time (Options, default 10 s, 0–60, 0 off) and switches back, at most every 1.5 s, while displays, sound and apps continue. Only TVs with input readback (LG) get one: without readback a steal can't be seen, and blind resends could fight the user. One guard at a time; every other TV power or input request, a new scene and a restoring scene stop it first, so it never fights the user. The preference is written only when changed, so older builds still read the file.
