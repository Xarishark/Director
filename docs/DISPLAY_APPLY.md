# Applying display presets

## Native workflow

The green check on a display-preset row applies that saved layout. Director resolves monitor device paths against current Windows targets, validates the requested configuration, and snapshots the actual active setup before changing it. It does not reuse saved adapter/target locators as hardware identities or guess by friendly name.

Changes are temporary until readback matches the requested enabled monitors, positions, desktop dimensions, primary source, rotations, exact equivalent refresh fractions, and mirrored/extended grouping. When confirmation is enabled for the preset, a native **Keep these display settings?** dialog then gives 15 seconds to choose **Keep settings** or **Revert**. Escape/closing the dialog, a missing confirmation, or a lost confirmation channel chooses reversion. An already-matching layout reports that it is active without changing hardware or asking for confirmation.

Each preset has an **Ask to keep display changes** switch, off for newly created or captured presets. Existing saved preferences are preserved; legacy presets without the field retain their previous confirmation-on behavior. Turning it off automatically keeps verified changes. Validation, strict readback, persistence checks, and failure recovery always run. Editing or cancelling this preference does not change the captured layout or apply it. The CLI follows this saved preference unless explicitly overridden with --yes or --confirm. There is no deadline when confirmation is disabled.

The monotonic deadline belongs to the background worker, not a UI timer. A stalled UI cannot extend it. The worker owns the cross-process operation lock through application, confirmation, and any restoration. Other preset writes or device changes cannot overlap it. Normal window closure stays blocked while the transaction is pending; Revert or the timeout returns to the previous layout.

Requested changes are saved to the Windows display database only after verification and confirmation. Restoration also saves the actual pre-operation snapshot, including after an attempted persistence failure. A failed setter may already have changed the desktop, so setter errors, verification failures, persistence failures, and Rust panics after an attempted write all trigger restoration and readback. Failed restoration is reported as incomplete recovery rather than success.

## Direct Windows implementation

src/display_apply.rs contains the shared transaction, semantic verifier, and confirmation deadline. src/windows_display_apply.rs resolves current routes, assigns unique sources to independent desktops while preserving clone groups, calls SetDisplayConfig with VALIDATE before mutation, applies without SAVE_TO_DATABASE initially, and persists after confirmation. Exact current target timings are retained when only arrangement changes. CCD source dimensions are supplied as reported by Windows, while GDI desktop dimensions remain the readback/UI geometry. Cold-enabled progressive targets use a progressive scan-line value with a specified refresh fraction; an unspecified scan-line value was rejected by the validation host and is covered by a regression test. Windows best-mode completion may be used after a bad-configuration validation result, but strict readback still decides whether the requested layout was achieved.

The rollback snapshot contains native active paths and mode tables, including target timings, alongside the actual user-visible layout. Recovery reapplies that snapshot and verifies the observed layout. Known disabled monitors may be absent; required enabled monitors must be available and identifiable. Ambiguous identities, unsupported routing, and active monitors not represented in a saved preset fail before mutation. Recapture when hardware membership or driver identities change.

The GUI uses ordinary Rust channels and supported Reactor background-task messages to bridge countdown progress to the native dialog. UI objects never cross to the worker. Confirmation messages carry a transaction token so an old dialog cannot keep a later operation.

## CLI

```text
director display check "Desk monitors"
director display apply "Desk monitors"
director display apply "Desk monitors" --yes
```

check resolves and validates through Windows without applying. apply follows the saved preference. When confirmation is enabled, it asks the terminal for yes/y with the same 15-second deadline; EOF, another answer, or no answer reverts. --yes explicitly keeps a verified layout without prompting, for automation without a window. --confirm forces confirmation even for an automatic preset. Neither option skips verification or recovery. Success/already-matching exits 0; a reverted/failed application exits 1; bad arguments, missing presets, or access/initialization errors exit 2.

## Current boundaries

This supports ordinary captured desktop layouts. Virtual-mode topologies, cold-enabling interlaced modes, advanced VRR/DPI/color settings, automatic identity relinking, and driver-specific changes are outside the modeled feature. Windows may create/remove HDMI audio endpoints as displays change; Director does not explicitly change audio settings during a display transaction.

The deadline cannot rescue a process that has been forcibly terminated, a crashed/restarted process, or a synchronous driver call that never returns. There is no external watchdog or durable interruption-recovery journal yet. Restoration can also fail after unplugging hardware or a driver failure; the result includes those errors. These are limits, not claims of tested recovery on every monitor/driver combination.

## Reference

- [Microsoft SetDisplayConfig contract, validation, temporary application, and persistence](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-setdisplayconfig)
- [Target refresh and target-mode precedence](https://learn.microsoft.com/en-us/windows/win32/api/wingdi/ns-wingdi-displayconfig_path_target_info)
## Validation — 2026-10-31

All 31 tests pass, including transaction failures/partial writes, panic recovery, cancel/timeout/channel loss, incomplete recovery, clone/identity verification, source assignment, snapshot mode changes, and cold-enable timing parameters. Release build, strict all-target Clippy, and formatting pass.

Native validate-only and same-layout Apply left the desktop unchanged. A controlled live test temporarily enabled the known secondary monitor with a small offset while preserving the primary, held stdin open without confirming, and verified automatic restoration after 15 seconds. A concurrent operation was rejected while confirmation was pending. Native accessibility checks then verified the updating dialog, guarded window close, explicit Revert, Keep/persistence, and normal/narrow rendering. A verified headless Apply restored the original single-screen setup. Required-missing-monitor preflight failed without mutation. Owned fixtures were removed and the existing user display preset was preserved; audio CLI checks passed.

This is validation on the local desktop configuration, not every TV, GPU, refresh/resolution change, or hot-plug failure. Forced-termination recovery, full physical failure injection, and clean-PC distribution remain unverified.

HDR now participates in explicit preset verification and restoration when requested and readable. See [HDR application and limits](HDR.md). Legacy presets without an HDR field leave it unchanged. The same transaction also drives the independent [Current setup](CURRENT_SETUP.md) display Apply operation.
