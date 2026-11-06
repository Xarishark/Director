# Audio presets

The main output receives Console, Multimedia and Communications. Separate communications output is off by default; enabling it reveals another output picker, overriding only Communications. Its volume is left unchanged unless it is also the main selected output with Set volume enabled.

Change microphone is off by default. When enabled, the microphone picker selects all three input roles. Set microphone volume is a second optional switch. Turning Change microphone off omits all input operations, including volume. Editing/saving never applies settings or records audio. The preset card previews communications and microphone intent.

The GUI and saved-preset CLI use the same request. `director audio inputs --json` lists input devices, each with its CLI name, without changing them. Legacy preset files remain readable; their absent optional selections follow the new user-approved output behavior (all three output roles) and leave the microphone unchanged. Applying an existing preset therefore also changes its communications output unless a separate one is selected. File format additions are optional and no saved files were automatically rewritten.

All selected devices must be active and uniquely resolve by endpoint ID before any write. Snapshot the current endpoint for every affected role and the actual volumes of every requested device. Record attempted mutations before calling Windows, verify results, and on failure restore all affected roles and volumes. Do not substitute another output or microphone after selection fails. The caller retains the existing cross-process operation lock throughout recovery. Windows endpoint mute state remains unchanged.

## Remembered TV audio

Linking a TV to an audio endpoint should be metadata, independent of display/audio presets. A scene can later use it to wait for the correct endpoint after enabling the TV display. Linking alone must not wake the TV or switch audio.

Prefer a verified one-to-one hardware association. Do not infer the link solely from an HDMI label, a matching name, or a shared GPU. Where Windows cannot provide a unique association, offer a one-time selection/confirmation and remember its endpoint identity and last name. Director remembers Windows input and output endpoints it has seen in a separate local device inventory.

The audio preset and TV audio pickers include previously seen offline devices. Director resolves a saved reference by endpoint ID, then by a unique Windows Stable ID in the same direction when available. A unique endpoint instance path can also match when known Stable IDs agree or are unavailable. Names help suggest replacements. If the reference cannot be resolved, use **Relink device** to choose a replacement and review affected presets and TV links before confirming.

Run `director audio volume speakers 35` to set the volume of a Windows output to 35%. Use `director audio outputs` to find its CLI name (its Windows name in lowercase with dashes, so "Speakers (Realtek(R) Audio)" is `speakers-realtek-r-audio`) or endpoint ID. If a CLI name matches multiple outputs, pass the endpoint ID. `director audio list` lists audio presets. This command preserves routing and mute state.

Windows documents [endpoint roles](https://learn.microsoft.com/en-us/windows/win32/coreaudio/device-roles), [device/container properties](https://learn.microsoft.com/en-us/windows/win32/coreaudio/device-properties), and [endpoint association/Stable ID properties](https://learn.microsoft.com/en-us/windows/win32/coreaudio/audio-endpoint-properties). The association property describes a kernel-streaming pin category; it is not itself a TV-to-monitor mapping.

Explicit TV/audio linking is now implemented in TV setup and editing. Choose an available or previously seen endpoint; Director retains its identity and last name. The TV link also appears in audio preset choices while unavailable, and scenes can use it directly. The TV-to-HDMI audio association is selected in TV setup. See [scenes](SCENES.md).
