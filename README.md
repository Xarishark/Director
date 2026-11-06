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
- **Now** changes what’s active right now without saving anything. Sound changes apply as you make them; display and FPS edits apply when you press Apply. Use **Save as display preset…** or **Save as scene…** to keep what you’ve set up.
- **Library** holds your saved building blocks: **Displays** and **TVs**.

A **scene** is everything you want to switch together: its display preset, its sound, its TV, an FPS limit and actions. Display presets and TVs are saved once and reused across scenes; sound is set in each scene.

| Building block | What you can save |
| --- | --- |
| **Display preset** | Which screens are enabled, how they’re arranged, which is primary, and their resolution, refresh rate, orientation, and HDR. You can extend or mirror screens. |
| **TV connection** | A paired TV, its PC input, wake settings, and optional links to its Windows display and HDMI audio. |
| **Scene** | Any combination of TV preparation, a display preset, sound (an output or the TV's sound, volume, mute, a calls device and a microphone), a global NVIDIA or RTSS FPS limit, and apps, links or "close app" steps to run before, after, or when you leave the scene. Director remembers audio devices it has seen so you can pick them while they’re offline. |

Display preset cards show a preview of the layout. A scene card shows its main change: its screens, or for a sound-only scene its output and volume. Use the checkmark to apply a preset or scene. Saving gives you a chance to review it before applying.

## Install the preview

Director needs **Windows 11, version 24H2 or newer** (x64). Setup checks this and stops on older versions.

1. Use the **Download for Windows** button above to get **Director-x64-setup.exe** from the latest GitHub Release. You can also get it from the **Director-Windows-x64** artifact of a successful **Windows build** run. Both require access to the private repo.
2. Run setup and click **Install**. Director installs for everyone on the PC, so Windows asks for administrator approval. If Microsoft Windows App Runtime 2.6 is missing, setup downloads it (about 125 MB) and installs it first. Leave **Create a desktop shortcut** selected if you want one. See the [installation and runtime guide](docs/WINDOWS_GUI.md).
3. Click **Launch**, or open **Director** from Start or the desktop. Setup adds Director to PATH. Open a new terminal to use the command line.

## Set up your first TV scene

1. The first time you open **Scenes**, save your everyday setup: **Save as my Default scene** stores your current screens, sound and FPS limit as a scene, so you can always switch back in one click. It changes nothing on your PC, and it reuses a display preset you already saved when it matches. Then create a separate display layout for the TV in **Library › Displays**, or from the scene's Displays step. If Windows hasn’t exposed the TV’s modes yet, use **Detect options** to temporarily enable it, read its available settings, and restore your layout.
2. In **Library › TVs**, add the TV, follow its pairing guide, and select the PC input where supported. Link its HDMI audio output so scenes can use **TV's sound**.
3. In **Scenes**, create **TV Gaming**. Select the TV and the TV display preset, and in the Sound step choose **TV's sound** with a volume if you want one. Add a frame-rate cap or an app launch if you want one. If a display preset or TV is missing, choose **New display preset…** or **Add TV…** at the bottom of that list. Director returns to the scene with it selected after you save. Apply the scene when you’re ready.

Director runs the scene in order: **Before → TV → Displays → Sound → FPS → After.** A Before step can wait for a setup script to finish. Director waits for the selected HDMI audio device after changing displays. If a required step fails, Director stops and restores the previous Windows settings, unless you change that in Options. TV boxes and consoles often grab the TV’s input while it turns on; with TVs that report their current input, such as LG, **TV input protection** in Options keeps switching back to the scene’s input for the first seconds (10 by default) while the rest of the scene carries on.

Director remembers the scene you applied last until you restart the PC. Applying a different scene first runs the previous scene's **When leaving** steps, for example `steam://close/bigpicture` when you leave a console scene. **End** next to the **+** button (or `director scene end`) runs them without applying another scene; it appears only when the active scene has something to run. A scene can also turn its TV off when you leave it: switch it on in the scene's Prepare TV step.

LG webOS is supported through local control. Samsung Tizen and TCL Android / Google TV are hardware previews. The TV guides explain what to try on each brand and how to send a compatibility report.

## Use Director from a terminal

Use `director --version` or **About Director** in Options to check your installed version.

Run a saved scene from a shortcut, script, or terminal with the window closed. Commands use each item's **CLI name**: its name in lowercase, with dashes instead of spaces and symbols, so "TV Gaming" is `tv-gaming` and needs no quotes. Each scene and preset card has a **>\_** button next to its name that shows the exact command, by CLI name or by ID, with a Copy button, ready to paste into a Stream Deck action or a shortcut. For example:

```powershell
director scene apply tv-gaming
director display apply desk-monitors
```

`director list` shows every scene, preset, TV, screen and audio device with its CLI name. Each area also has its own list: `scene list`, `display list`, `display screens`, `audio list` (outputs and inputs), `audio outputs`, `audio inputs` and `tv list`. Add `--json` for scripts. Two items of the same kind can't share a CLI name, so Director asks for a different name when saving one that would clash.

Change sound right away, without a scene. Leaving out the device means the current default, and `--mic` targets the microphone:

```powershell
director audio status
director audio use soundbar-stage-v2
director audio volume +5
director audio mute toggle
```
## Logs and backups

- **View logs** in Options shows the latest scene result from the app or CLI, followed by recent log entries. If a scene stopped, look there for the failed step and whether recovery succeeded.
- **If Director closes unexpectedly**, it saves a crash report in the logs folder (`Logs\Crashes`) and offers it the next time it starts. **Report on GitHub** opens a new issue in your browser with the error filled in; nothing is sent until you submit it. **Report a problem** in Options does the same for anything else.
- **Notify when CLI scenes fail** lets Windows tell you when an unattended run stops. Click the notification to open that run in Director.
- **Export backup** saves your scenes and display presets to one local file. Backups from older versions still restore; their audio presets move into the scenes that used them. **Restore backup** previews what it will replace and flags device links to review. TV pairing keys stay on the original PC.

## Guides

- [Display setup and detection](docs/DISPLAY_SETUP.md) · [Audio devices and relinking](docs/AUDIO.md) · [Scenes](docs/SCENES.md)
- [TV setup and pairing](docs/TVS.md) · [Samsung test guide](docs/SAMSUNG.md) · [TCL test guide](docs/ANDROID_TV.md)
- [FPS limits](docs/FPS.md) · [Diagnostics](docs/DIAGNOSTICS.md) · [Build, test, and CLI reference](docs/DEVELOPMENT.md)

Director uses Rust and native WinUI 3. See the [rights notice](LICENSE.md).
