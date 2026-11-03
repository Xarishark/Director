# TCL Android / Google TV preview

Director uses Android TV Remote v2 directly inside the Rust app. No ADB, Python, developer mode, cloud account or companion CLI is needed. This preview targets TCL sets running Android TV or Google TV with Android TV Remote Service. TCL Roku and Fire TV models are not covered. Physical TCL validation is pending.

## Friend test guide

1. Keep the TV on, with the PC on the same trusted local network. Make sure the Google TV mobile remote works first; it uses the same service.
2. Open TVs → + → TCL — Android / Google TV. Name the TV and enter its IPv4 address (a DHCP reservation is recommended).
3. Choose Pair TV. Enter the six-character hexadecimal code shown on the television and choose Confirm code. If the code expires or is rejected, use Back and start pairing again. Codes are not saved or logged. Client identity keys are protected by Windows DPAPI for this account.
4. Director reads model and Remote Service version when available. The version is a service version, not necessarily the TV firmware. Input enumeration and selected-input readback are not provided by this protocol.
5. Optional: connect a visible source to the desired HDMI port, start the TV on a different input, select the target port, check the precondition and choose Test HDMI. Answer Yes only if the TV actually switched. Only confirmed direct ports become available to controls/scenes. No source-menu navigation or custom key sequences are used. A failed/rejected retest removes the port. New model/service-version information invalidates old confirmations.
6. Enable Screenless service / network standby under the TV’s power settings if available. This keeps network remote control reachable in standby; Director does not blank the screen. MAC entry is optional and remains behind the advanced switch: this protocol does not provide automatic MAC learning. A saved MAC enables Wake-on-LAN fallback.
7. Optionally link the Windows display and HDMI audio endpoint, then save. Power and input actions are configured separately in scenes.
8. Check status, turn off, then wake. Test each desired HDMI port. Export report in the compatibility section and attach its JSON. Export also works from Edit TV or director tv report "TV name". Nothing uploads automatically.

Use a second screen for Director during HDMI tests. Testing sends a real input command; Cancel discards unsaved test records but cannot undo that command. Reports can be exported before saving.

## Controls and boundaries

- Pairing uses mutually authenticated TLS on port 6467. Control uses port 6466. There is no unencrypted fallback.
- Each TV entry has an RSA client identity. The displayed code is checked against both certificate public keys before accepting the pairing exchange. Later control connections require the same server RSA identity; a changed identity requires explicit re-pairing. TVs using different pairing/control server keys may need additional investigation before compatibility can be established.
- Reported RemoteStart state gives On or Standby. A connected socket alone is not evidence that the panel is on. Timeout/disconnect stays Unknown.
- Wake sends the direct WAKEUP command when the authenticated remote service is reachable. If unreachable and a MAC is saved, Director sends Wake-on-LAN. Deep standby without reachable networking cannot be solved by a remote command alone.
- Turn off uses the direct SLEEP command, never a blind power toggle. Input uses exactly one numbered HDMI key. Sent is not TV acknowledgment or proof that a key had an effect.
- While controls are open, scoped status checks refresh reported state. Closing the controls stops checks/retries. CLI commands require no permanently running process.
- The shared scene and temporary display-detection providers handle TCL too. Scenes prepare the TV before displays/audio, wait for reported awake state, and skip unsupported input readback.
- Unsupported input selectors are hidden until confirmed. Generic remote buttons, volume, app launch, voice, text entry, Roku/Fire TV and custom sequences are outside this preview.

## Report privacy

The report allowlists brand, Director/report version, model/service version, pairing status, connection type, direct HDMI test send outcomes and user confirmations. It excludes codes, private keys, certificates, TV names, IP/MAC addresses, display/audio identities, raw responses and arbitrary errors. Reports live under the Windows Local AppData Director/Reports folder. General app logs remain available from Options, but the compatibility JSON is the intended shareable artifact.

## Implementation references

- [androidtvremote2](https://github.com/tronikos/androidtvremote2): pairing/protocol reference, Apache-2.0. Director implements the required wire subset independently; no Python runtime or copied generated code.
- [Google Polo pairing protocol](https://android.googlesource.com/platform/external/google-tv-pairing-protocol/): certificate/code exchange.
- [Home Assistant Android TV Remote](https://www.home-assistant.io/integrations/androidtv_remote/): documented TCL standby limitations.

Rustls, ring, rcgen, rsa, sha2 and x509-parser provide in-process cryptography and certificate handling. Framing is bounded and only the small required protobuf subset is implemented. Existing Windows App Runtime and adjacent app-icon requirements still apply when copying the preview to another PC; this change does not add an installer.

For the current format-2 report contents, tester result fields and sanitized action history, see DIAGNOSTICS.md. Use ANDROID_TV.md for the short friend instructions.
