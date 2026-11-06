<p align="center"><img src="assets/director.svg" width="96" height="96" alt="Director icon"></p>
<h1 align="center">Director</h1>
<p align="center">Switch your monitors, sound, and TV with one scene.</p>
<p align="center"><strong>Windows x64 · Private preview</strong></p>
<p align="center"><a href="https://github.com/Xarishark/Director/releases/latest/download/Director-x64-setup.exe"><img src="https://img.shields.io/badge/Download_for_Windows-x64_setup-0078D4?style=for-the-badge&amp;logo=windows&amp;logoColor=white" alt="Download Director for Windows (x64 setup)"></a></p>
<p align="center"><sub>Latest release · Requires access to this private repo · <a href="https://github.com/Xarishark/Director/releases">All releases</a></sub></p>

Moving from your desk to the living room? A **TV Gaming** scene can wake the TV, switch your picture and sound to HDMI, cap the frame rate, and open Steam Big Picture. Use a **Desk** scene when you come back. For a quick change, open **Now** and adjust the active displays, audio, or FPS limit directly.

## Presets and scenes

Director has three areas in its sidebar:

- **Scenes** is where Director opens. Run a scene, or create one.
- **Now** changes what’s active right now without saving anything. Use **Save as display preset…** or **Save as audio preset…** to keep what you’ve set up.
- **Library** holds your saved building blocks: **Displays**, **Audio**, and **TVs**.

A **preset** saves one part of your setup. A **scene** combines presets and actions you want to run together. Save each piece once and reuse it across scenes.

| Building block | What you can save |
| --- | --- |
| **Display preset** | Which screens are enabled, how they’re arranged, which is primary, and their resolution, refresh rate, orientation, and HDR. You can extend or mirror screens. |
| **Audio preset** | A Windows output with optional volume, a separate output for calls, and an optional microphone. Director remembers devices it has seen so you can select them while they’re offline. |
| **TV connection** | A paired TV, its PC input, wake settings, and optional links to its Windows display and HDMI audio. |
| **Scene** | Any combination of TV preparation, display and audio presets, a global NVIDIA or RTSS FPS limit, and apps, links or "close app" steps to run before, after, or when you leave the scene. |

The display and audio preset cards show a preview of what you saved. Use the checkmark to apply a preset or scene. Saving gives you a chance to review it before applying.

## Install the preview

1. Use the **Download for Windows** button above to get **Director-x64-setup.exe** from the latest GitHub Release. You can also get it from the **Director-Windows-x64** artifact of a successful **Windows build** run. Both require access to the private repo.
2. Run setup and click **Install**. Director installs for everyone on the PC, so Windows asks for administrator approval. If Microsoft Windows App Runtime 2.6 is missing, setup downloads it (about 125 MB) and installs it first. Leave **Create a desktop shortcut** selected if you want one. See the [installation and runtime guide](docs/WINDOWS_GUI.md).
3. Click **Launch**, or open **Director** from Start or the desktop. Setup adds Director to PATH. Open a new terminal to use the command line.

## Set up your first TV scene

1. In **Library › Displays**, capture your desk layout so you can switch back. Create a separate layout for the TV. If Windows hasn’t exposed the TV’s modes yet, use **Detect options** to temporarily enable it, read its available settings, and restore your layout.
2. In **Library › TVs**, add the TV, follow its pairing guide, and select the PC input where supported. Link its HDMI audio output, or create a separate **Audio preset** if you also want a specific volume.
3. In **Scenes**, create **TV Gaming**. Select the TV, the TV display preset, and the audio choice. Add a frame-rate cap or an app launch if you want one. If a preset or TV is missing, choose **New display preset…**, **New audio preset…**, or **Add TV…** at the bottom of that list. Director returns to the scene with it selected after you save. Apply the scene when you’re ready.

Director runs the scene in order: **Before → TV → Displays → Audio → FPS → After.** A Before step can wait for a setup script to finish. Director waits for the selected HDMI audio device after changing displays. If a required step fails, Director stops and restores the previous Windows settings, unless you change that in Options.

Director remembers the scene you applied last until you restart the PC. Applying a different scene first runs the previous scene's **When leaving** steps, for example `steam://close/bigpicture` when you leave a console scene. **End** next to the **+** button (or `director scene end`) runs them without applying another scene; it appears only when the active scene has something to run. A scene can also turn its TV off when you leave it: switch it on in the scene's Prepare TV step.

LG webOS is supported through local control. Samsung Tizen and TCL Android / Google TV are hardware previews. The TV guides explain what to try on each brand and how to send a compatibility report.

## Use Director from a terminal

Use `director --version` or **About Director** in Options to check your installed version.

Run a saved scene from a shortcut, script, or terminal with the window closed. Each scene and preset card has a **>\_** button next to its name that shows the exact command, by name or by ID, with a Copy button, ready to paste into a Stream Deck action or a shortcut. For example:

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
