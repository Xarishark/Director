<p align="center"><img src="assets/director.svg" width="96" height="96" alt="Director icon"></p>
<h1 align="center">Director</h1>
<p align="center">Switch your monitors, sound, and TV with one scene.</p>
<p align="center"><strong>Windows x64 · Preview</strong></p>
<p align="center"><a href="https://github.com/Xarishark/Director/releases/latest/download/Director-x64-setup.exe"><img src="https://img.shields.io/badge/Download_for_Windows-x64_setup-0078D4?style=for-the-badge&amp;logo=windows&amp;logoColor=white" alt="Download Director for Windows (x64 setup)"></a></p>
<p align="center"><sub>Latest release · <a href="https://github.com/Xarishark/Director/releases">All releases</a></sub></p>

Moving from your desk to the living room? A **TV Gaming** scene can wake the TV, switch your picture and sound to HDMI, cap the frame rate, and open Steam Big Picture. A **Desk** scene puts everything back. Every change is checked, and if a step fails Director restores what it changed.

<p align="center">
  <img src="docs/images/scenes.png" width="49%" alt="The Scenes page with a sound-only scene and a TV gaming scene">
  <img src="docs/images/displays.png" width="49%" alt="The Displays page with saved monitor layouts">
</p>

## What it does

- **Scenes** switch everything together: a display layout, sound, a TV, an FPS limit, and apps or links to run before, after or when you leave. Run one from the window, a shortcut, a script or a Stream Deck.
- **Now** changes what's active right now without saving anything: arrange and switch screens, pick the output and volume, set the FPS limit.
- **Displays** saves monitor layouts: which screens are on, their arrangement, primary, resolution, refresh rate, orientation, mirroring and HDR.
- **TVs** pairs LG webOS TVs (plus Samsung Tizen and TCL Android / Google TV as previews) to wake them, switch inputs and turn them off over your network.

## Install

Director needs **Windows 11, version 24H2 or newer** (x64). Download **Director-x64-setup.exe** with the button above and run it; setup installs the Windows App Runtime if it's missing and adds `director` to PATH. See [Install](docs/INSTALL.md) for details, quiet installs and uninstalling.

## Quick start

1. Open **Scenes** and choose **Save everyday scene**. It saves your current screens, sound and FPS limit as a scene called Default, so you can always switch back in one click. Nothing on your PC changes.
2. In **TVs**, add your TV and follow its pairing guide. Link its Windows display and HDMI audio output.
3. In **Displays**, create a layout for the TV. If Windows hasn't shown the TV's modes yet, use **Set up this display** to detect them.
4. In **Scenes**, choose **+**, name it **TV Gaming**, and add the TV, the TV layout, **TV's sound** and any apps. Then **Apply** it.

## Command line

Every scene, preset, TV, screen and audio device has a CLI name: its name in lowercase with dashes, so "TV Gaming" is `tv-gaming`. The **>\_** button next to a name copies the exact command.

```powershell
director list
director scene apply tv-gaming
director scene end
director display apply desk-monitors
director audio volume +5
director tv toggle living-room-tv
```

`director --help` lists every command. Commands run without opening the window.

## Guides

- **Using Director:** [Scenes](docs/SCENES.md) · [Displays](docs/DISPLAYS.md) · [Sound](docs/SOUND.md) · [FPS limits](docs/FPS.md) · [TVs](docs/TVS.md)
- **Help:** [Install](docs/INSTALL.md) · [Troubleshooting, logs and backups](docs/TROUBLESHOOTING.md) · [Test a Samsung or TCL TV](docs/TV_TESTING.md)
- **Contributing:** [Development](docs/DEVELOPMENT.md) · [Architecture](docs/ARCHITECTURE.md) · [Interface design](docs/DESIGN.md)

## Contributing

Bug reports, TV compatibility reports and pull requests are welcome: see [CONTRIBUTING.md](CONTRIBUTING.md). Report security problems privately as described in [SECURITY.md](SECURITY.md). Release notes are in [CHANGELOG.md](CHANGELOG.md).

Director is written in Rust with a native WinUI 3 interface, and licensed under the [MIT License](LICENSE).
