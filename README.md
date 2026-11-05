<p align="center"><img src="assets/director.svg" width="96" height="96" alt="Director icon"></p>
<h1 align="center">Director</h1>
<p align="center">Switch your monitors, sound, and TV with one scene.</p>
<p align="center"><strong>Windows x64 · Private preview</strong></p>

Moving from your desk to the living room? A **TV Gaming** scene can wake the TV, switch your picture and sound to HDMI, cap the frame rate, and open Steam Big Picture. Use a **Desk** scene when you come back. For a quick change, open **Current setup** and adjust the active displays, audio, or FPS limit directly.

## Presets and scenes

A **preset** saves one part of your setup. A **scene** combines presets and actions you want to run together. Save each piece once and reuse it across scenes.

| Building block | What you can save |
| --- | --- |
| **Display preset** | Which screens are enabled, how they’re arranged, which is primary, and their resolution, refresh rate, orientation, and HDR. You can extend or mirror screens. |
| **Audio preset** | A Windows output with optional volume, a separate output for calls, and an optional microphone. Director remembers devices it has seen so you can select them while they’re offline. |
| **TV connection** | A paired TV, its PC input, wake settings, and optional links to its Windows display and HDMI audio. |
| **Scene** | Any combination of TV preparation, display and audio presets, a global NVIDIA or RTSS FPS limit, and apps or links to open at the end. |

The display and audio preset cards show a preview of what you saved. Use the checkmark to apply a preset or scene. Saving gives you a chance to review it before applying.

## Install the preview

1. Download **Director-&lt;version&gt;-x64-setup.exe** from the repo’s GitHub Releases, or from the **Director-Windows-x64** artifact of a successful **Windows build** run. Both require access to the private repo.
2. Run setup and click **Install**. Director installs for everyone on the PC, so Windows asks for administrator approval. If Microsoft Windows App Runtime 2.6 is missing, setup downloads it (about 125 MB) and installs it first. Leave **Create a desktop shortcut** selected if you want one. See the [installation and runtime guide](docs/WINDOWS_GUI.md).
3. Click **Launch**, or open **Director** from Start or the desktop. Setup adds Director to PATH. Open a new terminal to use the command line.

## Set up your first TV scene

1. In **Display presets**, capture your desk layout so you can switch back. Create a separate layout for the TV. If Windows hasn’t exposed the TV’s modes yet, use **Detect options** to temporarily enable it, read its available settings, and restore your layout.
2. In **TVs**, add the TV, follow its pairing guide, and select the PC input where supported. Link its HDMI audio output, or create a separate **Audio preset** if you also want a specific volume.
3. In **Scenes**, create **TV Gaming**. Select the TV, the TV display preset, and the audio choice. Add a frame-rate cap or an app launch if you want one. Apply the scene when you’re ready.

Director runs the scene in order. **TV → Displays → Audio → FPS → Apps and links.** It waits for the selected HDMI audio device after changing displays. If a required step fails, Director stops and tries to restore the previous Windows settings by default.

LG webOS is supported through local control. Samsung Tizen and TCL Android / Google TV are hardware previews. The TV guides explain what to try on each brand and how to send a compatibility report.

## Use Director from a terminal

Use `director --version` or **About Director** in Options to check your installed version.

Run a saved scene from a shortcut, script, or terminal with the window closed. For example:

```powershell
director scene apply "TV Gaming"
director display apply "Desk monitors"
director audio apply "Headphones"
```

To change an output’s volume without switching to it, use its Windows name. Run `director audio list` for the exact name or endpoint ID.

```powershell
director audio volume "Speakers" 35
```

## Logs and backups

- **View logs** in Options shows the latest scene result from the app or CLI, followed by recent log entries. If a scene stopped, look there for the failed step and whether recovery succeeded.
- **Notify when CLI scenes fail** lets Windows tell you when an unattended run stops. Click the notification to open that run in Director.
- **Export backup** saves your scenes and presets to one local file. **Restore backup** previews what it will replace and flags device links to review. TV pairing keys stay on the original PC.

## Guides

- [Display setup and detection](docs/DISPLAY_SETUP.md) · [Audio devices and relinking](docs/AUDIO.md) · [Scenes](docs/SCENES.md)
- [TV setup and pairing](docs/TVS.md) · [Samsung test guide](docs/SAMSUNG.md) · [TCL test guide](docs/ANDROID_TV.md)
- [FPS limits](docs/FPS.md) · [Diagnostics](docs/DIAGNOSTICS.md) · [Build, test, and CLI reference](docs/DEVELOPMENT.md)

Director uses Rust and native WinUI 3. See the [rights notice](LICENSE.md).
