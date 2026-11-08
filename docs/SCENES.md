# Scenes

A scene is everything you want to switch together: a display layout, sound, a TV, an FPS limit, and apps or links to open. Applying it changes everything in one go, checks that each change took effect, and puts things back if a step fails.

## Your first scene

With no scenes yet, the Scenes page shows **Start with your everyday setup**. It reads your current screens, default output and volume, and global FPS limit; untick anything you don't want, name it (Default unless you change it) and choose **Save everyday scene**. Saving doesn't change anything on your PC. If one of your saved display presets has the same layout, the scene uses it; otherwise Director saves the layout as a new preset too. **Build a scene from scratch instead** opens an empty editor.

## Steps

The editor shows a scene's steps as cards in the order they run. Choose a card (or its pencil) to edit that step in a dialog: **Done** keeps the changes and **Cancel** puts the step back as it was. The bin clears a step. Empty steps show an **Add** button.

| Step | What it does |
| --- | --- |
| **Before** | Runs apps or opens links first. An app can wait until it finishes; the scene stops if it fails or runs longer than the device wait in Options. |
| **Prepare TV** | Wakes a saved TV if needed and switches it to an input, usually your PC's. Optionally turns the TV off when you leave the scene. See [TVs](TVS.md). |
| **Displays** | Applies a saved display preset. See [Displays](DISPLAYS.md). |
| **Sound** | Sets the output, volume, mute, a separate calls device and a microphone. See [Sound](SOUND.md). |
| **FPS limit** | Sets or removes the global NVIDIA or RTSS frame rate limit. See [FPS](FPS.md). |
| **After** | Runs apps or opens links once everything else has succeeded. |
| **When leaving** | Runs when you switch to another scene or end this one, for example `steam://close/bigpicture`. |
| **Return** | Optional: the scene to go back to when you end this one. |

Every step is optional, so a scene can be sound only, displays only, or just a set of apps. Actions are **Run app**, **Open link** and **Close app**. Close app asks every running copy of the program to close, waits up to 10 seconds, and ends it only if you turned that on; a program that isn't running counts as closed. App arguments go one per line, and nothing passes through a shell.

The last entry in the display preset and TV lists (**New display preset…**, **Add TV…**) opens that editor and brings you back to the scene with the new item selected.

## Applying a scene

Choose **Apply** on a scene card, or run `director scene apply <scene>`. Steps run in a fixed order: **Before → Prepare TV → Displays → Sound → FPS limit → After.**

- Before anything changes, Director checks the scene's references and app paths and records the current Windows settings it might change.
- The TV step waits until the TV reports it's awake and on the right input. With [TV input protection](TVS.md#tvs-in-scenes) on, Director keeps it there for the first seconds while the rest of the scene carries on.
- After the displays change, the Sound step waits for the chosen devices to appear (a TV's HDMI audio often appears only now), up to the device wait in Options. It never picks a different device instead.
- A display preset that asks to keep changes shows the Keep/Revert prompt as usual.

**If a step fails** or you cancel, the remaining steps and the After actions don't run, and Director restores the displays, default devices, volumes, mute and FPS limit it changed, then checks the restore worked. To keep the steps that already finished instead, turn on **Keep changes that finished if a later step fails** in Options. TV commands and apps you launched can't be undone, so they're never part of the restore.

## The active scene, End and Return

The scene you applied last stays active until you apply another or restart the PC, in the window and the CLI alike. Its card shows a dot before the name.

- **Applying a different scene** first runs the active scene's **When leaving** steps, then turns its TV off if the scene says so. A leave step that fails is reported as a warning and never blocks the next scene.
- **End <scene name>**, next to the **+** button, appears when the active scene has something to do on exit. It runs the leave steps and then applies the **Return** scene, if one is set. `director scene end` does the same.
- **Running the active scene again** also ends it and goes to its Return scene, when it has one.

Before running any leave steps, Director checks that the Return scene still exists and is valid; if not, it keeps the current scene active and runs nothing. End doesn't restore any settings by itself; use a Return scene such as your Default for that.

## Shortcuts and the command line

The **>\_** button after a scene's name shows the commands for it, each with **Copy**: by CLI name (`director scene apply tv-gaming`, easy to read) and by ID (keeps working after a rename). Paste either into a Stream Deck action, a shortcut or a script. Setup adds `director` to PATH.

```text
director scene list
director scene check tv-gaming
director scene apply tv-gaming [--yes | --confirm]
director scene end
```

- A scene's CLI name is its name in lowercase with dashes ("TV Gaming" is `tv-gaming`). Two scenes can't share one.
- `check` validates references and app paths without changing anything.
- `apply` runs without opening the window. `--yes` keeps verified display changes without asking; `--confirm` asks in the terminal.
- The exit code is 0 only when the whole scene succeeded. With **Notify when CLI scenes fail** on in Options, a failed run shows a Windows notification that opens its result in Director.

## How it works

Scenes are stored in `scenes.json` in `%LOCALAPPDATA%\Director`. A scene refers to display presets and TVs by ID, so editing a preset changes every scene that uses it; sound settings live in the scene itself. Deleting a preset or TV that a scene still uses is refused and names those scenes. The active scene is kept in `active-scene.json` together with the time Windows started.

The window and the CLI share one runner and one cross-process lock, so two scenes never change devices at the same time. While Director waits for a TV, it resends wake at most five times and the input request at most every 1.5 seconds, within the device wait.
