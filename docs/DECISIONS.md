# Design decision record

Recorded: 2026-10-30. All capabilities remain unimplemented. "Accepted" means agreed direction, not completed functionality.

## Accepted

| ID | Decision | Reason/context |
| --- | --- | --- |
| D001 | Rust application logic and CLI; native Windows interface | Native compiled desktop application without a Chromium frontend; prioritize speed and low resource use |
| D002 | Shared profile engine; CLI application without GUI initialization | Identical behavior from the window and automation |
| D003 | Independent display and audio profiles, optionally composed into combined setups | Users must not need to bind audio and displays at profile creation |
| D004 | Audio input, output, and volume can be controlled independently | Supports input-only, output-only, and volume-only requests |
| D005 | Capture current settings first; guided step-by-step custom creation later | Establish basic workflows before expanding creation tools |
| D006 | Remember observed audio devices while unavailable | TV HDMI audio may appear only after the display transition |
| D007 | Prefer Windows Stable ID when available, fall back to endpoint ID, allow explicit relinking | Names are editable/non-unique and device identities can change |
| D008 | A device must be enumerated at least once before selection; show identity and Windows name | Bind profiles to real observed devices, not typed names |
| D009 | Ordered optional final executable and URI actions | Support Steam Big Picture and other user-selected apps after configuration |
| D010 | Native Windows system notifications | Report background/CLI results using the system notification experience |
| D011 | Selectable failure policy; default revert, alternative stop and keep completed changes | Recover previous state by default while allowing user choice |
| D012 | Defer tray mode and starting with Windows until after basic functionality | Keep the first implementation focused |
| D013 | Exclude CRU and SpotBye | Explicitly outside the replacement application's scope |
| D014 | Preserve the local-only checklist outside Git; document agreed design in repository files | Keep the private backlog local while making the project understandable and reviewable |

## Proposed or unresolved

| Topic | Current proposal or open question |
| --- | --- |
| Combined-profile references | Reference component IDs; edits propagate to dependent setups; duplicate for independent variants |
| Rename/deletion | Editable names with stable internal IDs; prevent deletion that leaves dangling references |
| Terminology | Display profiles, Audio profiles, and combined Setups |
| CLI | Explicit type selection, for example `app apply audio "Headphones"`; syntax/name not finalized |
| Final-action gating | Skip launches if required configuration fails; determine behavior on a failure inside the final-action chain |
| Action completion | Dispatch only initially, or optionally wait for executable exit; URI readiness cannot generally be established |
| Policy overrides | Whether failure behavior can be overridden per setup or CLI invocation |
| Audio roles | Default console-only selection versus multiple Windows audio roles |
| Volume behavior | Fixed, unchanged, or restore-previous options during normal profile application; failure rollback is already agreed |
| Notifications | Failure notifications by default, optional success notifications and quiet CLI flag |
| Display confirmation | Whether/how to use a GUI countdown independently of operation-failure rollback |
| Operation ownership | GUI/CLI coordination, busy-request handling, cancellation, and interrupted operations |
| Reapplication | Reapply all steps versus skip verified unchanged state; optional toggle behavior |
| Additional scope | HDR, color depth, VRR, DDC, other TV brands, hotkeys, triggers, arbitrary scripts, import/export |
| Guided creation milestone | Required before 1.0 or later release |
| Distribution | Windows baseline, portable/installer model, update behavior, executable arrangement |
| Licensing | App license and appropriate WinUI 3 license/disclosure |
| Performance | Measured budgets for startup, idle CPU, memory, and CLI startup |

## Working rule

Update this record when a decision is accepted or changed. Update [requirements](REQUIREMENTS.md) and the [architecture proposal](ARCHITECTURE.md) where affected. Keep unresolved proposals visibly separate from accepted behavior.
