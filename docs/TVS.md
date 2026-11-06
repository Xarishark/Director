# TVs

## Add a TV

Open **TVs → +**. The native guide walks through:

1. **Brand** — LG webOS or Samsung Tizen. Existing saved TVs default to LG.
2. **Name** the TV.
3. **IP address** — enter its IPv4 address from its network settings; keep it on.
4. **Pair** — accept the TV’s connection prompt. LG reads available inputs and network addresses. Samsung reads model/firmware, reported power and network information when available.
5. **PC input / compatibility** — LG selects from reported inputs. Samsung offers an optional single-command HDMI test, followed by explicit user confirmation; unconfirmed/failed inputs remain hidden. See [Samsung testing](SAMSUNG.md).
6. **Set up wake** — review learned addresses; enable the advanced manual MAC field if needed, or skip.
7. **Windows connections** — optionally link its display and audio output.
8. **Review and save**.

The guide saves after the final step. Choosing an input records its identity for future scenes. Saving does not switch inputs, wake the TV, or apply a Windows layout. The explicit Samsung compatibility test does send one direct input command; choosing a port alone does not. Back preserves completed choices; changing the IP/connection method during setup clears the draft’s pairing and learned information. Cancel writes no TV definition. Permission already granted on the TV remains until revoked in the TV’s settings. A successful pairing remains in the draft if reading inputs fails, so **Retry** can read the details again without repeating pairing.

An unavailable network-info request is a warning, not a setup failure. Missing input information can be retried; a successfully read empty input list can proceed with an explicit **No PC input** choice. Invalid names, IPs, MACs, duplicate definitions, and incomplete steps cannot be saved.

## Existing TVs

The pencil opens the compact editor. For a paired TV, available inputs and wake addresses load automatically in the background; **Save TV** remembers the pending details. If the TV cannot be reached, saved choices remain editable. Select or correct the PC input, add a manual MAC, or change the Windows display link independently. Name/connection-metadata edits retain pairing. An IP or connection-method change shows **Continue setup** and returns to guided pairing; the original definition stays saved until the guide finishes.

The TV row contains just a **remote-control SVG**, **pencil**, and **red bin**. The remote opens live TV controls; the pencil edits connection metadata; the bin asks to remove the definition. Standalone Pair again, Check connection, and Read TV details buttons are removed. Pairing and initial information reads belong to setup; metadata refresh on edit is automatic. Removing a TV deletes Director’s local definition/credential; revoke permission on the TV separately if needed. Saved pairing is not a live-status indicator. Advanced headless pair/check/learn commands remain available.

## Wake-on-LAN

Director reads `wiredInfo.macAddress` and `wifiInfo.macAddress` from the authenticated TV’s local connection-manager service. It validates individual, nonzero 48-bit addresses and stores Ethernet, Wi-Fi, and an optional manual address separately. Colon, hyphen, and compact manual forms normalize to uppercase colon-separated addresses. Invalid, zero, broadcast, and multicast addresses are rejected. A failed or partial read does not erase previously learned addresses. No ARP/gateway inference is used.

Wake sends standard 102-byte magic packets directly through Rust UDP: IPv4 local broadcast `255.255.255.255:9`, three rounds 100 ms apart, one packet per distinct saved MAC per round. A two-second socket write timeout bounds individual writes. It works without an open control connection or pairing key, including while the TV is off. **Wake request sent** means UDP dispatch succeeded; it does not prove the TV/panel has awakened. No follow-up polling, automatic input switch, display change, or audio change runs.

The TV must remain network-connected and support/enable the relevant network/mobile power-on feature. Wake behavior over Wi-Fi and deep standby is model/firmware-dependent. This first sender uses the system’s broadcast routing; multiple interfaces, routed subnets, and explicit interface/directed-broadcast selection need future work. It does not discover a changed IP.

MACs are device metadata and wake destinations, not cryptographic identity or replacements for the current connection IP. Verified rediscovery/address migration remains unimplemented. A Windows display path, TV pairing key, and TV network addresses are distinct identities.

## Optional Windows display link and scene boundary

A TV is independent of display presets and scenes. Its optional Windows link stores the physical target device path and last-known name. An unreported link remains selectable as unavailable. Director does not guess from friendly names, display numbers, IPs, or transient adapter/target numbers. A Windows path change requires manual relinking; multiple PC connections may eventually need multiple associations.

The PC input is also a connection mapping. Which TV actions to perform—wake, select the PC input, power off, timing/readiness—belongs to the scene, not pairing/setup preferences. Scene composition is still unimplemented; manual input switching and power-off are available in TV controls. TV readiness alone must not imply Windows display/HDMI-audio readiness.

## Connection and credential protection

Encrypted WebSocket on port 3001 remains the default. Director permits the local LG TLS handshake, then compares exact remembered certificate bytes **before sending a saved key**. Initial certificate trust accompanies explicit pairing and on-TV approval. Changed certificates require explicit re-pairing. LG may share certificates across TVs; continuity checking does not uniquely identify hardware.

**Use legacy connection** explicitly selects unencrypted WebSocket port 3000 for older TVs. The guide/editor explains the consequence; there is no silent downgrade. Pairing key and certificate pin are protected together with Windows per-user DPAPI under Director’s Local AppData known folder. They are not included in presets, CLI listings, application errors, or logs. Copying credentials to another Windows account/machine is not supported.

TV definitions remain a versioned `tvs.json`. New input/PC/network fields default empty when reading existing definitions, preserving existing protected pairing bytes. Storage uses the operation lock, atomic writes, and stale-window detection; corrupt or future-version files are preserved. Device/storage/network work runs off the UI thread. Initial native connection phases use 2.5-second timeouts; WebSocket pairing has a 45-second deadline and inspection/checking an 8-second deadline, with bounded cancellation. Pairing and the subsequent inspection are separate connections. Wake uses the same cross-process operation lock.

No Python, Node, ColorControl, downloaded TV helper, persistent service, or tray process is required.

## CLI

```text
director tv list
director tv pair living-room-tv
director tv check living-room-tv
director tv learn living-room-tv
director tv wake living-room-tv
director tv status living-room-tv
director tv off living-room-tv
director tv toggle living-room-tv
director tv input living-room-tv HDMI_1
```

Each TV is named by its CLI name (its name in lowercase with dashes, as `director tv list` shows) or its ID. Pair stores protected pairing and attempts metadata learning. Check authenticates and reads inputs without changing the TV or saving metadata. Learn refreshes and saves input/wake information without changing the selected PC mapping or TV state. Wake dispatches packets only; exit zero does not establish physical wake. Toggle reads the power state first: it turns the TV off when it reports on, wakes it when it reports off or standby, and sends nothing when the TV can't be reached or reports an unknown state. Add/edit/remove and PC/display mapping are configured in the window.

## Validation and remaining work

The user reported successful LG pairing/input enumeration on 2026-11-01 with the earlier slice; connection mode was unspecified. That feedback does not establish MAC-reading or wake behavior.

Current automated checks cover guide progression/gating, optional/manual MAC validation, packet bytes and three actual UDP sends to a loopback receiver, TV-reported Ethernet/Wi-Fi parsing and denial, preservation of old paired definitions, credential protection, storage failures, and mock native WebSocket registration/timeouts. Native owned-window tests cover real release guide name/IP/pair screens, Back/navigation/cancel, PC/MAC/display metadata save/reopen/remove, and the remaining guide screens through a test-only in-memory fixture. Settings and live displays are preserved; no real TV is contacted or woken by these tests.

The user reports the revised guide/MAC/wake flow works. A subsequent read-only hardware check independently verified Active state and Ethernet/Wi-Fi MAC reads. Standby/off transitions and power-off have not been exercised. Discovery/verified IP migration, scene readiness/orchestration, configurable wake timing, and per-interface broadcast selection remain later work.

Protocol research: [lgtvcli](https://github.com/01dnot/lgtvcli), [PyWebOSTV](https://github.com/supersaiyanmode/PyWebOSTV), and [lgtv2](https://github.com/hobbyquaker/lgtv2) (TV-reported wired/Wi-Fi fields and local wake protocol). These are references, not dependencies; no signed application manifest was imported.



### Manual MAC opt-in and hardware status check — 2026-11-01

Manual entry now sits behind **Enter MAC address manually** in both setup and editing. New setup defaults off and hides the field. An existing saved manual address reopens with the switch on. Toggling off retains draft text during editing, ignores its validation, and clears the manual address only when saving/advancing; learned Ethernet/Wi-Fi addresses remain intact. Toggling back on restores the draft.

The user reports the TV setup/wake work is functioning. With the TV explicitly turned on by the user, a separate read-only authenticated check independently returned power state **Active** and valid TV-reported MAC addresses for both Ethernet and Wi-Fi. No power, input, or settings change was sent. At that earlier slice, the list power icon meant Wake only. The control-modal update below supersedes that UI. A proposed small control modal would use fresh reported state and explicit Wake/Turn off/input actions. Unreachable/unknown must not be interpreted as off. Standby/off transitions and Turn off have not been exercised by this check.



## Basic TV controls — current

Choose the **remote-control** SVG beside a TV. The native modal reads fresh power status, external inputs, and the current foreground input when the TV reports enough information. Use the refresh icon to reread; no background process or continuous polling is added.

There is one power button:
- **On**, **Screen off**, or **Screen saver** → **Turn off**.
- **Off** or **Standby** → **Wake**.
- **Unknown/unreachable** → **Wake**, enabled only with a saved MAC.

A failed connection is never proof of Off. Before turning off, Director checks fresh state; a stale Turn off click on a TV reporting standby/off cannot wake it. Wake dispatches idempotent Wake-on-LAN packets immediately, without waiting for an unreachable TV to answer, and never turns an already-awake TV off. Unrecognized/unreachable fresh state blocks a turn-off request. Wake can still dispatch to saved MACs without an authenticated connection.

Selecting an input changes only the pending choice. **Switch input** validates the target against a freshly read input list, sends the explicit input ID, and attempts bounded foreground-app readback. An unchanged input is a no-op. Readback confirms the TV-selected input/app, not HDMI signal or Windows/audio readiness. If acknowledgment succeeds but readback is unavailable/late, the result says accepted without claiming verification.

Control mutations run on workers under the shared operation lock and reject stale TV definitions. Status reads run independently without that mutation lock. Wake readiness and uncertain results use separate status checks; confirmed input changes and acknowledged off update directly. Closing the modal does not cancel a command already in flight; the window’s existing close protection remains active until work finishes. Errors/uncertain acknowledgments remain visible. Refresh again after wake/input settling when needed. A successful turn-off acknowledgment is not independent proof that the physical panel powered off.

The modal never edits pairing, PC-input mappings, display presets, or saved scene choices. Status/input reads do not save TV metadata. CLI and future scenes use explicit wake/off/input intents, not an alternating toggle. CLI status reads fresh state; off rechecks it; input takes the exact input ID listed by status/check. Wake remains explicit packet dispatch.

Validation: 64 regular tests pass, including explicit power intent/state races, Unknown handling, input freshness/no-op/readback, strict acknowledgments, and native WinHTTP requests against a loopback mock. Native default/minimum-size modal checks confirm one power button, pending input selection, unknown-state behavior, Close, and guide/editor regression behavior. A read-only check on the user’s LG returned On/current PC and four external inputs; the live modal showed the same values and refreshed correctly. Original settings files and live Windows layout stayed unchanged. Physical turn-off/input-switch commands were tested only against the mock, not sent to the user’s TV.




## Responsive controls and unavailable TVs

Opening the modal immediately shows Checking while a worker contacts the TV. These are authenticated TV requests, not ICMP pings. Read-only checks do not set global busy or hold the mutation lock, so Close and Wake remain usable. An ordinary refresh disables another refresh and input selection until its response arrives; power transitions additionally disable repeated commands until completion or expiry. Turn-off and input changes retain mutation exclusion and fresh validation. Wake dispatch does not require an authenticated connection, and action completion does not wait for a follow-up status read. Wake uses the bounded readiness monitor described below; manual refresh remains available after completion or expiry. There is no permanent monitor.

Each dialog has a lifetime token and read revision. Starting an action invalidates an older read; closing/reopening or refreshing ignores superseded responses. Read completion never releases a mutation guard. Network timeouts remain bounded by the transport; they no longer gate opening or closing controls.

A compact fixed-width body replaces the outer scrolling panel. Unknown status never means Off. Hide empty input controls until available. Short local messages replace duplicated page/modal errors and misleading status-updated banners. Technical diagnostics are collapsed by default, with a bounded scroll area inside that disclosure only. No user settings or TV state are changed by status reads.


## Power transitions and selective refresh — current

Wake immediately shows **Waking…**, dispatches Wake-on-LAN once, and polls authenticated power status until the TV reports an awake state. Failed/unavailable checks remain part of waking rather than flashing Unknown. Polls are spaced two seconds after each completed attempt, within a 30-second retry window starting after dispatch. A request already in flight can finish after that window, subject to existing native transport timeouts. Inputs are queried only once the TV reports awake. A responding TV still in standby is not treated as awake.

On expiry, show **Not responding yet** (or **Not awake yet** when the TV has reported a non-awake state), with **Retry wake**. A retry starts a new wake request/window. Close stays available; closing/reopening invalidates reads and queued polls through the dialog token/revision. No permanent polling continues after success, expiry or closing.

Turn off shows **Turning off…** while the command runs. A positive acknowledgment changes the modal to **Off · assumed**, with Wake available; Director does not immediately query an intentionally unreachable TV. This describes accepted intent rather than independently verified physical state. A fresh response saying already Off/Standby is shown as reported, without the assumed qualifier. An unacknowledged/failed command is never marked assumed Off.

Confirmed input changes update the current input in place and preserve the rest of the modal. No additional full status refresh follows a confirmed change or confirmed no-op. Failed/uncertain commands and accepted-but-unconfirmed input readback get one reconciliation read, retaining the original command details separately. Manual refresh remains available outside pending transitions.

The core engine returns typed effects (WakeSent, OffAccepted, AlreadyOff, InputConfirmed, InputAccepted), so the GUI never infers results from message text. CLI commands remain explicit and headless; GUI wake waiting does not change CLI wake's dispatch-only contract.



## Optional reminder in display presets

When creating a new display preset without a saved TV, Director offers a quiet Setting up a TV? card. Adding a TV is optional; ordinary displays and manually controlled TVs remain supported. Add TV opens the existing guide and keeps the display draft intact. Close hides the tip for the session; Don't show again remembers the choice. Restore it under Options → Show TV setup tips. Already-saved TVs suppress the generic reminder, and linked monitor cards show their TV name with View TV. This guidance does not enable a display, wake a TV, change its input or learn monitor capabilities.


## TV preparation during display detection

A linked TV with protected pairing, wake addresses and a saved PC input offers Wake and detect inside its monitor's setup card. This is an explicit runtime preparation action, not a stored scene preference. Director snapshots Windows first, wakes the TV, waits for an awake report, selects the saved input, then learns display options through Windows. The display layout is restored afterward; the TV stays on with that input. TV metadata is not changed. Manual Detect options remains available without adding/pairing a TV.


## Live power updates while controls are open

Opening a TV's controls starts a read-only, authenticated SSAP power subscription on a worker. It reuses the existing account-protected pairing and certificate pin; it never silently pairs. Command requests retain their existing explicit intents and shared mutation lock. Power updates change the current modal in place. An external power-on triggers one input/status reconciliation; ordinary input changes do not continuously reload the view.

A lost connection is unknown, not proof of shutdown. An acknowledged Director turn-off retains Off · assumed until a fresh reported state is available. Original command errors remain separate from subscription/connection errors. Late events are rejected across dialog/action boundaries. Reconnect attempts back off from two seconds to a maximum fifteen seconds while controls remain open; reconnecting never sends Wake-on-LAN.

An explicit GUI Wake allows up to five wake dispatches total (the initial dispatch and four retransmissions, at least two seconds apart) within the existing thirty-second readiness deadline. Each dispatch may send packets for multiple saved MAC addresses. Stop retries when the TV reports awake, the controls close, or the deadline expires. No sleeping worker holds the mutation lock. Closing the modal signals cancellation; an idle WebSocket receive is interrupted and its handles are released on the worker. An already-started HTTP handshake remains bounded by transport timeouts.

CLI Wake remains dispatch-only and does not create a listener or require Director to stay running. No tray/startup service was introduced. Screen off/on controls are not planned unless explicitly requested; existing reported Screen Off status can still be displayed.

Validation: 84 regular tests, a local mock WebSocket for pushed states and cancellation, native in-memory remote-power/disconnect/recovery/close tests, and the existing native wake/off/input/timeout fixture. Physical-TV validation is recorded separately when performed.

## Remembered HDMI audio link

TV setup’s optional Windows connection step and Edit TV now include a TV audio output dropdown. First-time choices are active Windows outputs; existing saved TV links remain selectable when unavailable. Selection records endpoint ID and last name, not a name-based guess, and never changes audio or pairing. Scenes can explicitly select the link and wait for that endpoint after displays apply. Automatic TV/display/audio correlation is not implemented. See [Scenes](SCENES.md).

## Provider capabilities

LG uses its existing direct webOS provider. Samsung uses its separate native Tizen provider, with DPAPI-protected authorization, direct Wake-on-LAN, reported power and guarded KEY_POWER turn-off. Samsung controls have no per-key acknowledgment. Unsupported features are omitted from the modal and scene editor. A manually confirmed HDMI command is exposed for that entry/model/firmware; it is reported as sent, not read back. Local status polling runs only while its modal is open. SmartThings, discovery and custom sequences are not implemented. See SAMSUNG.md.

## TCL Android / Google TV

Choose the TCL Android / Google TV platform, request a code and confirm it in Director. The remaining input-test, optional wake/display/audio links and save steps use the existing guide. Direct input controls require an explicit successful test. See [TCL setup and friend test guide](ANDROID_TV.md); other TCL platforms are not included.
