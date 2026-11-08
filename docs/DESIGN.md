# Director interface

Director uses genuine WinUI 3 controls through Microsoft Windows Reactor and follows the system theme, accent, fonts, focus and scrolling. Custom drawing is limited to layout previews and the display arrangement map. The default client size is 1000 × 700 DIP, minimum 760 × 600.

## Design system

Every page is built from `src/ui.rs`, so pages line up and follow the theme:

- **Type ramp** (Fluent): caption 12, body 14, label 14 semibold, section subtitle 20 semibold, page title 28 semibold. Secondary text uses one opacity (0.72).
- **Spacing** 8/12/16/32 and **radii** 4 (inner panels, controls) and 8 (cards). Page padding and pane widths are constants that width calculations reuse.
- **Colour** comes from theme brushes: card fill and stroke, critical red and the accent. Translucent greys mark inner panels, dashed outlines and the arrangement map, so they read in Light and Dark. Layout previews keep one fixed set of screen colours.
- **Page**: fixed header (title, one-line subtitle, actions at the top right), scrolling body with a scroll-bar gutter, and an optional fixed footer: a hint on the left, Cancel then the accent action on the right.
- **Section**: a subtitle with an optional quiet action beside it (Save as…), a one-line description, then the content. Forms stay within 650 DIP; monitor cards use up to three columns of about 400 DIP.
- **Settings**: Options uses Windows-Settings-style cards, each with a title and description on the left and its control on the right.
- **Empty states** use one helper: a subtitle, a line of explanation and at most one button.
- **Icons**: the green check applies, the pencil edits, the shared red bin deletes, and the remote-control SVG opens TV controls. Every icon button has an accessible name and a tooltip, except inside dialogs (see below).

## Window and navigation

The window uses WinUI 3's TitleBar, like Windows 11 Settings: the pane toggle and "Director" sit in the title bar next to the caption buttons. The navigation pane lists **Scenes** (the start page), **Now**, **Displays** and **TVs**, with **Options** in the pane footer. Reactor 0.100 has no footer menu items, so Options is a button styled like a navigation item with the same selection pill.

Pages connect forward instead of being islands. The last entry of each scene-editor dropdown (New display preset…, Add TV…) opens that editor and, after Save, returns to the scene with the new item selected; Cancel also returns. An open draft is never overwritten: you're asked to finish or cancel it first.

**Leaving with unsaved changes.** Choosing another page while the current editor has unsaved changes opens a dialog: Save, Discard or Keep editing. Unsaved means the draft differs from how it opened; data that loads into an opened draft (display modes, live TV details) doesn't count. Save appears only when the draft can be saved, so the TV setup guide and Now offer only Discard or Keep editing.

**Save** is enabled only when the draft is valid and has changed, or is a new item that isn't saved yet. The footer says "No changes to save." meanwhile. Saving never applies anything; only Apply changes devices.

**Status area.** Hidden when nothing needs attention. Work in progress shows one muted line with an indeterminate progress bar. A finished action shows a Success InfoBar that hides after 5 seconds. Warnings you need to read stay until closed, and errors stay red until closed. Routine hints are logged, not shown.

## Scenes

With no scenes, the Scenes page shows one card, "Start with your everyday setup". It reads the current screens, default output and volume, and the global FPS limit, shows each with a checkbox and a live summary, and saves them as a scene named Default. Saving changes no device and reuses a saved display preset when the layout matches. While only one scene exists, a slim tip above the cards explains making the next one.

Scene cards show the scene's main change at the top: the layout miniature when it has a display preset, otherwise a 124 px panel with a large icon and the main value (the output and a volume bar, the TV and its input, the FPS limit, or the number of actions). A fixed slot before the name holds the Active dot. The ">_" button after the name opens a dialog with the command by CLI name and by ID, each with Copy, plus `director scene end` for scenes that do something when left.

The scene editor is a vertical flow of fixed-size step cards in run order (Before, Prepare TV, Displays, Sound, FPS limit, After) joined by small arrows, with When leaving and Return set apart below. A card shows an icon, the title and a one-line summary, with a pencil and a bin. The card opens the step's controls in a dialog: Done keeps the changes and Cancel restores the step. Nothing in the flow expands, and the column has a fixed width (720 DIP, less in a narrow window), so the overview never changes size. Empty steps are dashed outlines of the same size with a hint and an Add button. Failure policy and the device wait live in Options, not in the scene. See [Scenes](SCENES.md).

**No tooltips in dialogs.** The pinned Reactor rejects tooltips inside a ContentDialog and closes the window. Controls in dialogs get an automation name instead, and `scene_editor_ui::tests::dialog_content_has_no_tooltips` guards the step dialogs and the shared helpers they use.

## Now

Now changes the live setup without saving anything. It has no page title. It starts with Displays and the FPS card, which stage edits until **Apply changes**; then a divider captioned "Sound changes below apply right away"; then Sound.

- **Apply changes** and **Discard** sit in the arrangement map's bottom-right corner, where Windows keeps its display actions. Apply changes applies the displays and then, only if that succeeded, the FPS limit. Discard rereads Windows.
- **Sound** has an output picker, a Mute/Unmute button and a volume slider, and the same row for the microphone. Each change applies at once. "Different device for calls" enables a calls-output picker beside it (greyed out while off, so nothing moves) that sets only Windows' communications role.
- **Save as display preset…** asks for a name and saves the layout shown, including unapplied edits. **Save as scene…** opens a new scene on its Sound step with the current output and microphone.

## Display editor

The arrangement map follows Windows Settings › Display: centred, up to 1000 DIP wide and 280 DIP tall. Pressing a screen selects it in the accent colour and outlines its card; dragging moves it with pointer capture and a stable scale. A screen dropped away from the others moves back to share part of an edge with the nearest one. Edits stay in the draft until Save or Apply; no gesture calls a Windows setter.

Every display has a card: its name with Primary or Make primary and the On switch, then Resolution beside Refresh rate, Desktop mode (extend or mirror) beside Orientation, and HDR when the display supports it. Mirrored outputs share one map tile. The primary display's On switch is greyed out, with the reason on a hover layer above it. Orientation choices show a monitor SVG rotated to that orientation (the stand marks which way is down) beside Landscape, Portrait, Landscape flipped or Portrait flipped.

Each card offers **Detect options** when Windows hasn't exposed a display's modes yet. The card explains the temporary change first, then shows progress, the current phase and Cancel inline, with no popup. Failures stay in the card and offer Restore previous layout when needed. See [Display setup](DISPLAYS.md#screens-that-are-off-or-not-detected-yet).

Display preset cards show a miniature of the saved layout with grouped mirror numbers and a legend (resolution, refresh rate, HDR); disabled screens are listed as Display off. Selecting a card never applies it.

## FPS card

FPS limit is one card at form width: title and current state on the left, the switch on the right, then Method beside Limit and Frames per second beside VRR-safe cap, always shown. While the switch is off the controls are greyed out and show what turning it on would use, so the page never changes height. A scene's FPS step uses the same controls without the card. See [FPS](FPS.md).

## TVs

The TVs page uses the same cards, + button, pencil and bin. Navigating, selecting, saving or editing never contacts a TV; pairing and control are explicit background actions.

**Add TV** is a step-by-step guide: brand, name, IP address, pairing (with an on-TV approval instruction and a bounded wait) plus automatic input and network reads, PC input, the wake address (with an opt-in manual MAC field), an optional Windows display and HDMI audio link, and review. Pairing happens before saving and survives a later read failure. Existing TVs open a compact editor that refreshes details in the background and stays editable offline.

**TV controls** open from the remote-control icon in a dialog that appears immediately in Checking and stays usable while it reads. It shows the status, one power button whose text says what it will do (Turn off when on, Wake when off or unknown and a MAC is saved), the current input and a picker with Switch. Pending states read Waking… and Turning off…; an accepted turn-off shows Off · assumed. An unreachable TV is never labelled Off. Unsupported controls are hidden rather than shown disabled. Technical details stay collapsed.

New display presets can show a small neutral "Setting up a TV?" card with Add TV and Don't show again (restorable in Options). It's hidden once a TV is saved.

## Options

Options holds app-wide settings as setting cards. **Applying scenes**: keep finished changes when a later step fails, the device wait and TV input protection. Then notifications for failed CLI scenes, TV setup tips, **Backup** (export and restore), **Diagnostics** (logs and Report a problem) and **About**. See [Diagnostics](DIAGNOSTICS.md).
