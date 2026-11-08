# Changelog

Each release's installer and full notes are on [GitHub Releases](https://github.com/Xarishark/Director/releases).

## Unreleased

- **Updates**: Director checks GitHub for a newer version when the window opens and offers **Install update**, which downloads setup, verifies its SHA-256 checksum and starts it. Turn the check off or check by hand in **Options → About**; `director update [--install]` does the same from a terminal.

## 0.13.0 (2026-11-08)

- Director is open source under the **MIT License**. The installer adds `LICENSE.txt` and the third-party notices beside the app.
- Every settings file starts at format version 1. Files from earlier previews (scenes version 2 to 4, version 2 backups) aren't read; recreate them or ask in an issue.
- `display presets` and `display current` are gone; use `director display list` and `director display screens`.
- New README with highlights, a screenshot slideshow and Stream Deck examples; the guides are merged into fewer pages.

## 0.12.0 (2026-11-07)

- Scenes can **Return** to another saved scene when you end them or run them again.
- The Active dot sits before the scene name, and card names use the space the buttons leave.

## 0.11.0 (2026-11-07)

- **Scenes overhaul**: a step-flow editor with Before, When leaving and Close app actions; the active scene and **End**; a first-run "everyday setup" scene; the TV can turn off when you leave a scene.
- **Sound**: live output, volume and mute controls on Now and in `director audio`; scenes hold their own Sound step.
- **CLI names** for every item (`director scene apply tv-gaming`), `director list`, and a `>_` button on cards that copies the command.
- **TV input protection** keeps an LG TV on the scene's input while a streaming box or console tries to take it; `director tv toggle` turns a TV on or off.
- A UI pass with a shared design system, a reworked Now page and a Windows-style display editor; preset cards preview their screens.
- Local crash reports you can file as GitHub issues; Windows 11 24H2 is now the minimum; a new app icon.

## 0.10.0 (2026-11-05)

- `Director-x64-setup.exe` installs the Windows App Runtime when it's missing, then Director for all users.
- Every version increase merged into `main` publishes a release with the setup.
