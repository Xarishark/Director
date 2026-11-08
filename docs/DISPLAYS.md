# Displays

A display preset saves which screens are on, how they're arranged, which is primary, and each screen's resolution, refresh rate, orientation, desktop mode (extend or mirror) and HDR. Scenes use presets, and you can apply one on its own from **Displays** or the command line.

## Save a layout

- **Capture current layout** (the camera button) saves your screens exactly as they are now under a name.
- **+** opens the editor starting from your current layout, so you only change what's different.
- The pencil on a preset opens the same editor for that preset.

Saving only saves. Nothing changes on your screens until you apply a preset.

## The editor

The arrangement map works like Windows Settings › Display. Select a screen to highlight its card; drag it to move it. A screen dropped away from the others moves back to touch the nearest one, and overlaps are resolved when you let go. Hover a screen for its size, refresh rate and position.

Each display has a card:

- **On** switch and **Make primary**. The primary display can't be switched off; make another one primary first.
- **Resolution** and **Refresh rate**. Only modes Windows reports for that screen are listed, with exact rates (59.94 Hz stays 59.94). Rates that Windows marks with an asterisk belong to a different resolution and appear when you pick that resolution.
- **Desktop mode**: **Extend desktop** or mirror another enabled display. Mirrored screens share one tile on the map and one resolution.
- **Orientation**: Landscape, Portrait, Landscape flipped or Portrait flipped, each shown with a rotated monitor icon.
- **HDR**, when the screen supports it. Leaving it unset keeps whatever HDR state the screen has when the preset is applied.

**Ask to keep display changes** decides whether applying this preset shows a 15-second Keep/Revert prompt. It's off for new presets.

## Screens that are off or not detected yet

Windows only reports the modes of a screen while it's active. For a TV or monitor that's off, its card offers **Set up this display**:

1. Turn the display on and select the PC's input, then choose **Detect options**.
2. Director temporarily turns that screen on (your other screens stay as they are), reads its resolutions, refresh rates and HDR support, then restores your layout and checks it. The card shows each phase, and **Cancel detection** stops it at any point before restoring.
3. The detected options fill the card. Director remembers them, so you can build presets for that screen while it's off; they're marked **Previously detected** and checked again when applied.

If the screen belongs to a paired TV with a saved wake address and PC input, **Wake and detect** also wakes the TV and switches it to that input first. The TV stays on afterwards.

If restoring your layout fails, the card keeps what it needs and offers **Restore previous layout**. You can't save or discard that preset until it succeeds. Use **Detect again** after changing a cable, port or driver.

## Applying a preset

Choose **Apply** on a preset card, or run `director display apply <preset>`.

1. Director finds each screen by its Windows device path, checks the whole layout with Windows, and records your current setup.
2. It applies the layout, then reads it back and compares every screen: on or off, position, size, primary, orientation, exact refresh rate, mirroring and HDR.
3. With **Ask to keep display changes** on, you get **Keep these display settings?** for 15 seconds. **Revert**, closing the prompt or letting it time out restores your previous layout.
4. Only a verified (and, if asked, confirmed) layout is saved to Windows' display settings.

If anything fails along the way, Director restores your previous layout and checks it, and tells you if the restore didn't fully succeed. A preset that already matches your screens reports that and changes nothing. A preset that needs a screen that isn't connected fails before changing anything.

## Command line

```text
director display screens [--json]
director display list [--json]
director display capture "Desk monitors"
director display check desk-monitors
director display apply desk-monitors [--yes | --confirm]
```

- `screens` shows the live layout; `list` shows saved presets; `capture` saves the current layout. None of them change your screens.
- `check` validates a preset with Windows without applying it.
- `apply` follows the preset's **Ask to keep display changes** setting and asks in the terminal when it's on. `--yes` keeps a verified layout without asking; `--confirm` always asks. Neither skips verification or the restore on failure.
- Exit codes: 0 applied or already active, 1 reverted or failed, 2 bad arguments, missing preset or access error.

## How it works

Presets live in `display-presets.json` and detected options in `display-capabilities.json`, both in `%LOCALAPPDATA%\Director`. Screens are identified by their Windows device interface path, never by display number, friendly name or adapter ID, which can change between sessions. If a device path changes after a driver or hardware change, recapture or edit the preset.

Director talks to the Windows display configuration API directly (`QueryDisplayConfig`, `SetDisplayConfig`) and lists modes through DXGI. Changes are first applied without saving them to Windows, then saved only after verification. HDR uses Windows 11 24H2's HDR-specific display requests and is restored with the rest of the layout on failure; a requested HDR change is refused up front if the screen's current HDR state can't be read.

Not supported: interlaced and virtual modes, DPI scaling, colour depth, VRR settings and identifying screens with on-screen numbers. Recovery happens inside Director, so a crash or forced shutdown during a change can leave the temporary layout in place; Windows' own display settings can fix that.
