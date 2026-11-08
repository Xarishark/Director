# TVs

Director controls TVs over your home network: it pairs once, then can wake the TV, switch it to your PC's input and turn it off, from the window, a scene or the command line. Supported platforms:

| Platform | Status | Control |
| --- | --- | --- |
| LG webOS | Supported | Pairing, wake, power status, turn off, input list and switching with readback, live power updates, TV input protection |
| Samsung Tizen | Hardware preview | Pairing, wake, power status, turn off, HDMI inputs you've tested and confirmed |
| TCL Android / Google TV | Hardware preview | Code pairing, wake and sleep, power status, HDMI inputs you've tested and confirmed |

Other TCL platforms (Roku, Fire TV) and cloud services such as SmartThings aren't supported. Have a Samsung or TCL TV? [Testing it](TV_TESTING.md) and sending a report helps.

## Add a TV

Open **TVs → +**. The guide walks through:

1. **Brand.**
2. **Name** the TV.
3. **IP address**: the TV's IPv4 address from its network settings. Keep the TV on.
4. **Pair**: accept the prompt on the TV (or, for Android TV, type the code it shows). Director then reads what the TV reports: inputs, network addresses, model.
5. **PC input**: choose the input your PC is connected to. LG lists its inputs. Samsung and TCL offer a test that sends one direct HDMI command; you confirm on screen whether it switched, and only confirmed inputs become available.
6. **Wake**: review the network (MAC) addresses Director learned. If none could be read, turn on **Enter MAC address manually**, or skip.
7. **Windows connections** (optional): link the TV's Windows display and its HDMI audio output, so scenes can wait for them.
8. **Review and save.**

Nothing is saved until the last step, and saving never switches inputs, wakes the TV or changes Windows. Back keeps your choices; changing the IP address clears the pairing so you pair again. If reading details fails after pairing, **Retry** reads them again without re-pairing. Permission granted on the TV stays there until you revoke it in the TV's settings, even if you cancel.

## Edit or remove a TV

The pencil opens a compact editor. For a paired TV, inputs and wake addresses refresh in the background; if the TV can't be reached, saved choices stay editable. Changing the IP address or connection method sends you back through pairing, and the old definition stays saved until you finish.

The bin removes Director's copy of the TV and its pairing key. To revoke Director's access completely, also remove it from the TV's device-connection settings.

## TV controls

The remote-control icon on a TV card opens its controls. They appear immediately and check the TV in the background:

- **One power button.** It reads **Turn off** when the TV reports On, Screen off or Screen saver, and **Wake** when it reports Off or Standby or can't be reached (Wake needs a saved MAC address). Director rechecks the state before turning off, so a stale click never does the opposite.
- **Waking…** sends Wake-on-LAN and watches for the TV to report it's awake, resending a few times within 30 seconds. If it doesn't wake, you get **Retry wake**.
- **Turning off…** becomes **Off · assumed** once the TV accepts the command; Director doesn't ping a TV it just turned off.
- **Input**: pick an input and choose **Switch input**. LG reads the input back to confirm it; Samsung and TCL report the command as sent.
- An LG TV pushes power changes while the controls are open, so the dialog updates when someone uses the remote.

A TV that can't be reached is shown as unknown, never as Off. Controls a TV doesn't support are hidden rather than greyed out. Technical details stay collapsed unless something fails.

## TVs in scenes

A scene's **Prepare TV** step can wake the TV and switch it to the saved PC input before the display step, and optionally turn it off when you leave the scene. If the TV is linked to a Windows display and HDMI audio output, the scene waits for them after changing displays. See [Scenes](SCENES.md).

**TV input protection.** Streaming boxes and consoles on HDMI-CEC often grab the TV's input while it wakes. For TVs that report their current input (LG), Director keeps switching back to the scene's input for the first seconds after it reports it (Options, default 10 seconds, 0 turns it off) while the rest of the scene carries on. Any other TV command from the app or the CLI, a new scene or a restore stops the guard first.

**Display detection.** A linked, paired TV with a saved PC input offers **Wake and detect** in its monitor card, so Director can wake it, select the input and read its display modes. See [Display setup](DISPLAYS.md#screens-that-are-off-or-not-detected-yet).

## Command line

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
director tv report living-room-tv
```

TVs are named by CLI name (the name in lowercase with dashes) or ID.

- `check` authenticates and reads inputs without changing anything; `learn` refreshes and saves inputs and wake addresses.
- `wake` only sends the packets: success means they were sent, not that the TV woke.
- `toggle` reads the power state first: it turns the TV off when it reports on, and wakes it when it reports off or standby or can't be reached.
- `input` takes an input ID exactly as `status` or `check` lists it.
- `report` exports a compatibility report for Samsung and TCL; see [Test a Samsung or TCL TV](TV_TESTING.md).

## How it works

**Wake-on-LAN.** Director stores the Ethernet and Wi-Fi MAC addresses the TV reports, plus an optional manual one; invalid, zero, broadcast and multicast addresses are rejected, and a failed read never erases known addresses. Wake sends standard magic packets to the local broadcast address (`255.255.255.255:9`), three rounds 100 ms apart, one per saved address. It needs no pairing or open connection. The TV must stay on the network and have its network or mobile power-on setting enabled; Wi-Fi and deep-standby behaviour depends on the model. Multiple network interfaces, routed subnets and finding a TV whose IP changed aren't handled yet.

**LG connection.** Director uses the encrypted WebSocket on port 3001 by default and pins the TV's certificate at pairing: the exact certificate is compared before a saved key is sent, and a changed certificate requires pairing again. **Use legacy connection** selects the unencrypted port 3000 for older TVs, only when you choose it.

**Samsung.** Director uses the TV's local remote-control WebSocket on the encrypted port 8002; **Use legacy connection** selects the unencrypted port 8001 only when you choose it. The TV's certificate is pinned at pairing and checked before the authorization token is sent. Power status comes from the TV's local information endpoint, and only a reported On, Standby or Off counts; a timeout stays unknown. **Turn off** needs a fresh On report and sends one power key, never a blind toggle. HDMI inputs use one direct `KEY_HDMI<n>` command each, never menu navigation or key sequences, and each port is offered only after you've tested it and confirmed that the TV switched. A new model or firmware reading clears earlier confirmations. Samsung doesn't acknowledge individual keys, so commands are reported as sent. While the controls are open, Director checks the power state every four seconds.

**Android TV.** Director implements Google's Android TV Remote v2 protocol in Rust, the same service the Google TV phone remote uses: pairing on port 6467 and control on port 6466, both over mutually authenticated TLS with no unencrypted fallback. Pairing creates an RSA client certificate for that TV; the code shown on screen is checked against both certificates, and later connections require the same TV certificate (a changed one needs pairing again). The TV reports On or Standby; a connection alone doesn't count as On. **Wake** sends the protocol's wake command, falling back to Wake-on-LAN with a saved MAC address when the TV can't be reached; **Turn off** sends its sleep command. This protocol can't list inputs or read the current one, so HDMI inputs work like Samsung's: one direct key each, offered only after a confirmed test. Android TV reports a Remote Service version rather than the TV's firmware version.

**Credentials.** Pairing keys, tokens, certificate pins and client keys are encrypted with Windows DPAPI for the current user under `%LOCALAPPDATA%\Director`. They never appear in CLI output, errors, logs, backups or reports, and can't be moved to another PC or Windows account.

**Identity.** A TV's Windows display link stores the monitor device path and last name, never a guess from names or display numbers. Its audio link stores the endpoint ID. If Windows changes the path or ID, relink it in the editor.

**Storage.** TV definitions live in versioned `tvs.json`, written atomically under the shared operation lock like other settings. Connections use bounded timeouts (2.5 s per connection phase, 45 s for pairing, 8 s for checks), and all TV I/O runs off the UI thread. No helper program, service or tray process is involved.
