# Sound

Director changes Windows sound in two ways:

- **Live**, from the **Now** page or `director audio`: each change applies right away and nothing is saved.
- **In a scene**, from its **Sound** step: the settings are saved with the scene and applied with its other steps, and undone if a later step fails.

## Now

The Sound section shows your default output and microphone:

- Pick an output or microphone to make it the Windows default for apps, media and calls.
- Move a volume slider; the level is sent once the slider rests, so a drag writes once.
- Use **Mute** / **Unmute**.
- Turn on **Different device for calls** and pick a calls output to send calls (Windows' communications role) somewhere else. Turning it off sends calls back to the main output.

Each change is checked, and put back if Windows doesn't confirm it. **Save as scene…** opens a new scene with its Sound step holding the current output and microphone, with their volume and mute.

## The scene Sound step

| Setting | Choices |
| --- | --- |
| Output | Leave as is, a device Director has seen, or **TV's sound** (the output linked to a saved TV) |
| Volume | Leave as is, or 0–100% |
| Mute | Leave as is, Unmute or Mute |
| Calls | **Use a different device for calls**: a separate output for calls |
| Microphone | **Change microphone**: a device, with its own volume and mute |

With the output left as is, volume and mute apply to whichever output is the default when the scene runs.

Sound runs after the Displays step, because a TV's HDMI audio often only appears once its screen is on. The scene waits for the chosen devices, up to the device wait in Options, and never switches to a different device instead. If a later step fails, Director restores the default devices, volumes and mute it changed (unless Options keeps finished changes).

## Offline devices and Relink

Director remembers every output and microphone it has seen, so you can pick a device that's unplugged right now, such as a TV's HDMI audio. If a saved device can't be found any more, for example after a driver update gave it a new ID, the step shows **Relink device…**: choose the replacement, review the scenes and TV links that will change, and confirm.

## Command line

```text
director audio status [--json]
director audio list | outputs | inputs [--json] [--all]
director audio use <device> [--role console|multimedia|communications]
director audio volume [<device>] <0..100 | +N | -N>
director audio mute | unmute | mute toggle [<device>]
director audio check
```

- Add `--mic` to target the microphone instead of the output.
- Leaving out the device means the current default, so `director audio volume +5` suits a Stream Deck button.
- A device is named by its CLI name (its Windows name in lowercase with dashes: "Speakers (Realtek(R) Audio)" is `speakers-realtek-r-audio`) or its endpoint ID. Use the ID when two devices share a name.
- `use` makes the device the default for all three Windows roles unless `--role` picks one.
- `list` hides devices Windows remembers but that aren't present, unless you add `--all`. `check` also tests the default-device interface Director uses.

## How it works

Director uses Windows Core Audio directly. A device is matched by its endpoint ID, then by its Windows Stable ID, never by its name alone. Remembered devices are kept in `audio-devices.json` in `%LOCALAPPDATA%\Director`. Linking a TV to an audio output only stores that link; it never wakes the TV or switches audio by itself. Director changes the Windows defaults and endpoint volume; apps that are set to a specific device keep using it.
