# Sound

Director handles sound in two places:

- **Live controls** change one thing right away: the **Now** page and `director audio` commands. Picking an output or microphone makes it the Windows default, a volume slider or `audio volume` sets the level, and mute buttons or `audio mute` toggle mute. Nothing is saved.
- **A scene's Sound step** saves sound settings in the scene and applies them with the scene's other steps, as one transaction with rollback.

Audio presets are retired. Scenes that used one, and scenes that used a TV's linked audio, migrate into a Sound step when they load. See [scenes](SCENES.md).

## Now

The Sound card shows the default output and microphone. Choosing a device makes it the default for all three Windows roles (Console, Multimedia, Communications). Each volume slider sends its level once it rests for 150 ms, so a drag writes once; a slider that moves while a change is running keeps its place and is sent next. The mute buttons toggle the device's mute state. Every change is verified, and the previous value is put back if Windows does not confirm it. **Save as scene…** opens a new scene whose Sound step holds the current output and volume.

## The scene Sound step

| Setting | Choices |
| --- | --- |
| Output | Leave as is, a device, or a TV's sound (the output linked to a saved TV) |
| Volume | Leave as is, or 0–100% |
| Mute | Leave as is, unmute, or mute |
| Calls | Optionally a different device for the Communications role |
| Microphone | Optionally a device for all three input roles, with its own volume and mute |

With the output left as is, volume and mute apply to whichever output is the default when the scene runs. Sound runs after the displays, so a TV's HDMI audio is ready; the scene waits up to the device wait time in Options for the chosen devices to appear.

All selected devices must be connected and resolve to one endpoint before anything changes. Director snapshots every affected role, volume and mute state, records each change before making it, verifies the result, and on failure restores roles, volumes and mute in reverse order (unless Options keeps completed changes). The cross-process operation lock is held throughout.

## Command line

```text
director audio status [--json]
director audio list | outputs | inputs [--json] [--all]
director audio use <device> [--role console|multimedia|communications]
director audio volume [<device>] <0..100 | +N | -N>
director audio mute | unmute | mute toggle [<device>]
director audio check
```

Add `--mic` to target the microphone. Leaving out the device means the current default, so `director audio volume +5` turns the current output up, which suits a Stream Deck button. A device is named by its CLI name (its Windows name in lowercase with dashes, so "Speakers (Realtek(R) Audio)" is `speakers-realtek-r-audio`) or its endpoint ID; when two connected devices share a name, use the ID. `audio list` hides endpoints Windows remembers that are not present unless `--all` is passed.

## Remembered devices and TV audio

Linking a TV to an audio endpoint is TV metadata. A scene's Sound step can use it as "TV's sound"; linking alone never wakes the TV or switches audio.

Prefer a verified one-to-one hardware association. Do not infer the link solely from an HDMI label, a matching name, or a shared GPU. Where Windows cannot provide a unique association, offer a one-time selection and remember its endpoint identity and last name. Director remembers Windows input and output endpoints it has seen in a separate local device inventory.

The Sound step and TV audio pickers include previously seen offline devices. Director resolves a saved reference by endpoint ID, then by a unique Windows Stable ID in the same direction when available. A unique endpoint instance path can also match when known Stable IDs agree or are unavailable. If a reference cannot be resolved, use **Relink device** to choose a replacement and review the scenes and TV links it affects before confirming.

Windows documents [endpoint roles](https://learn.microsoft.com/en-us/windows/win32/coreaudio/device-roles), [device/container properties](https://learn.microsoft.com/en-us/windows/win32/coreaudio/device-properties), and [endpoint association/Stable ID properties](https://learn.microsoft.com/en-us/windows/win32/coreaudio/audio-endpoint-properties). The association property describes a kernel-streaming pin category; it is not itself a TV-to-monitor mapping.
