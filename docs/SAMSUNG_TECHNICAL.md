# Samsung Tizen preview

Director has a native local Samsung provider. No Samsung account, SmartThings cloud, external CLI or custom remote sequence is required. Physical Samsung compatibility remains unvalidated: it is a preview for hardware testing, not a promise for every model.

## Setup and compatibility test

1. Open TVs → + and select Samsung Tizen. Enter its name and static/reserved IPv4 address. Keep the TV on.
2. Pair and accept Director in the TV’s on-screen connection prompt. The token, or the fact that a tokenless TV authorized the client, is protected by Windows for this account.
3. Director reads model/firmware and network addresses when the TV reports them. Missing fields remain Not reported; failure to read optional information does not discard successful pairing.
4. Optional HDMI test: choose a connected HDMI 1–4 port, use the physical remote to start on a different input, and check that precondition. Keep Director visible on another screen while testing. Test HDMI sends exactly one KEY_HDMI<n> command. There is no SOURCE/menu-navigation fallback.
5. Answer Did the TV switch to HDMI n? Yes enables that direct command; No leaves it hidden. Sending successfully by itself never enables the feature. Each port is tested separately. The latest test for a port controls availability, and stored model/firmware changes invalidate prior confirmations. A failed retest removes the port and clears it as the PC input. Next can skip input control entirely.
6. Optionally choose a confirmed port as the PC input used by scenes. Configure wake addresses (Samsung’s network standby must be enabled), Windows links, then save. Missing wake addresses hide Wake rather than presenting an unsupported button.

Testing changes the TV input only after the explicit test action. Saving/canceling does not undo a sent command. Cancel discards the TV draft and its unsaved tests; export first if those results should be retained separately. Editing a saved Samsung TV exposes the same test/export form. Changing its connection requires pairing again before a test can send saved authorization to it. Explicit CLI re-pairing clears prior test confirmations.

## Share the report

Choose Export report in the compatibility section of setup or Edit TV. Director saves JSON in the Windows Local AppData Director/Reports folder and opens that folder. The report captures the current draft, including tests before Save. Attach that JSON to a GitHub issue to share it. Nothing is uploaded automatically.

Reports contain Director/report versions, Samsung brand, model/firmware when reported, paired status, connection mode, reported token-auth capability, timestamps, port test send outcomes and explicit confirmations. They omit pairing tokens, certificates, IP/MAC addresses, names/aliases, TV IDs, display/audio links, raw responses and arbitrary error messages. They are built from an allowlist, rather than redacting a full catalog or application log. A successful user-confirmed test applies only to that saved entry and observed model/firmware; it does not establish universal Samsung support.

Headless export: director tv report "TV name". For logs of general app operations, Options → Open logs folder remains available; the compatibility JSON is the intended shareable Samsung report.

## Implemented control and scene behavior

- Local pairing and saved authorization over Tizen’s remote WebSocket. Encrypted port 8002 is the default; legacy unencrypted 8001 is an explicit opt-in, with no automatic downgrade.
- Wake-on-LAN uses the existing bounded dispatch/retry behavior and requires a learned/manual MAC plus TV network standby.
- Power reads device.PowerState from the local /api/v2/ information endpoint. HTTP success alone never becomes On, and a timeout never becomes Off. On/Standby/Off are reported only when present; otherwise Unknown.
- Turn off requires a fresh On/awake report. The provider authenticates, checks again immediately before sending one KEY_POWER, then reports Sent / Off assumed. It cannot guarantee atomic idempotence against somebody pressing the physical remote at the same moment. It never sends a blind toggle for Unknown/unreachable state.
- Normal input controls and scene input choices exist only for confirmed direct ports. Input readback is not supported and its UI is omitted. Commands are reported as Sent, not Accepted/Confirmed by the TV.
- A modal-scoped four-second polling interval plus bounded requests updates Samsung power; LG retains its scoped subscription. Poll results are timestamped before the request so pre-command reads cannot overwrite newer state. No resident daemon is required.
- Scenes and temporary display preparation use the same provider dispatcher as the GUI/CLI. A scene waits for reported awake state, sends an optional confirmed direct input command, then uses existing display/audio verification. Samsung input readback is not invented or waited on forever. Models that do not report usable power state cannot complete automatic TV readiness yet; omit TV preparation and turn the TV on manually for those models until validated support is added.

Older non-Tizen Samsung protocols, automatic discovery, volume/remote panels, screen blanking and SmartThings are not supported. Unsupported buttons are hidden. Supported controls may still be temporarily busy or disconnected, with truthful progress/error feedback.

## Transport and validation

Native WinHTTP is shared with the existing LG integration. First-pairing TLS certificates are remembered inside DPAPI data. Samsung puts its token in the upgrade URL, so a synchronous SENDING_REQUEST callback checks the exact remembered certificate before authorization-bearing headers are sent; mismatch aborts the request. Redirects are disabled and requests/frames/responses are bounded. No token or full URL is logged. A certificate pin is not proof of a globally unique TV identity.

Validation covers legacy LG loading, brand-change invalidation, strict command generation, token/tokenless handshakes and denial, state parsing, latest-confirmation/firmware gating, redacted reports and truthful unacknowledged outcomes. Native loopback tests exercise Samsung WebSocket frames. Owned TLS fixtures verify matching-pin command success and zero HTTP headers reaching a mismatched-certificate server. Native UI Automation covers brand/setup/pairing, test preconditions, No/Yes results, saved PC input, report content and hidden modal/scene controls at wide/narrow sizes without moving the mouse or changing real devices. Physical Samsung pairing/wake/off/HDMI testing remains pending.

Protocol references: [samsung-tv-control](https://github.com/timdevlet/samsung-tv-control/blob/main/src/api/local-tv.ts), [ColorControl SamsungDevice](https://github.com/Maassoft/ColorControl/blob/master/ColorControl/Services/Samsung/SamsungDevice.cs), and [Microsoft WinHTTP callback documentation](https://learn.microsoft.com/en-us/windows/win32/api/winhttp/nc-winhttp-winhttp_status_callback). These are references, not runtime dependencies.

For the current format-2 report contents, tester result fields and sanitized action history, see [Diagnostics](DIAGNOSTICS.md). Use [SAMSUNG.md](SAMSUNG.md) for the short tester instructions.
