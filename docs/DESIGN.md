# Director interface

## Design system — current

Every page is built from `src/ui.rs`, so pages line up and follow the system theme:

- **Type ramp** (Fluent): caption 12, body 14, label 14 semibold, section subtitle 20 semibold, page title 28 semibold. Secondary text uses one opacity (0.72).
- **Spacing** 8/12/16/32 and **radii** 4 (inner panels, controls) and 8 (cards). Page padding and pane widths are constants that width calculations reuse.
- **Colour** comes from theme brushes: card fill and stroke, critical red and the accent. Translucent greys mark inner panels, dashed outlines and the arrangement map, so they read on light and dark. Layout previews keep one fixed set of screen colours.
- **Page**: fixed header (title, one-line subtitle, actions at the top right), scrolling body with a scroll-bar gutter, and an optional fixed footer: a hint on the left, Cancel then the accent action on the right.
- **Section**: a subtitle with an optional quiet action beside it (Save as…), a one-line description, then the content. Forms stay within 650 DIP; monitor cards use up to three columns of about 400 DIP.
- **Settings**: Options uses Windows-Settings-style cards, each with a title and description on the left and its control on the right.
- Empty states use one helper: a subtitle, a line of explanation and at most one button.

## Navigation — current

The pane lists Scenes, Now, Displays and TVs, with Options in the pane footer. The Library group was removed once it only held Displays and TVs. Reactor 0.100 has no footer menu items, so Options is a button styled like a navigation item that shows the same selection pill and fill when open. This supersedes "Library is an expanded, non-selectable parent item" below.

## Display editor — current (comparison)

The arrangement map follows Windows Settings › Display: it spans the editor (up to 1000 DIP) and is 240 DIP tall. Pressing a screen selects it, drawn in the accent colour, and dragging still moves it. Displays that are off aren't on the map, so they're listed under it as buttons that select them.

Under the map, temporary tabs switch between two layouts so they can be compared; one will be removed:

- **One display** (Windows-style): the selected display's header (name, Primary or Make primary, the On switch), then settings rows grouped into **Layout** (Desktop mode, Resolution, Refresh rate, Orientation) and **Picture** (HDR), each with a compact control on the right.
- **All displays**: a card per display, with Resolution beside Refresh rate and Desktop mode beside Orientation.

Settings rows use a regular-weight title and a caption; only group headings are semibold.

## FPS card — current

FPS limit is one card: its title and current state on the left, the switch on the right, and its rows (Method, Limit, VRR-safe cap, Frames per second) always shown below. While the switch is off the rows are greyed out and show what turning it on would use, so switching it on never changes the page's height. A scene's FPS step uses the same rows without the card.

## Now — current

Now has three sections in this order: Displays and FPS limit, which stage edits, then a divider captioned "Sound changes below apply right away", then Sound, which applies each change as it is made. The page subtitle is "Changes here aren’t saved." The Apply bar belongs only to the staged sections: it appears once something is staged, says what, and offers Discard and **Apply changes**, which applies displays and then, if that succeeded, the FPS limit. Save as display preset… and Save as scene… sit beside their section titles; Save as scene… copies the output and microphone with volume and mute and opens the new scene on its Sound step dialog. Mute buttons read Mute or Unmute with matching icons, and the FPS section starts with what each method has set now. This supersedes "Keep separate Apply display changes and Apply audio changes actions, plus Discard edits" below.

## Information architecture (earlier)

The navigation pane has three areas. Scenes is the start page (except when opened from a failed-scene notification, which opens Options). Now (formerly Current setup) adjusts the live setup without saving. Library is an expanded, non-selectable parent item with Displays, Audio and TVs. Options stays in the pane footer. Each area has its own icon; Displays and TVs no longer share one.

Pages connect forward instead of being islands. Now offers Save as display preset… and Save as audio preset…, which open the normal Library editor seeded from the staged state. The last entry of each scene-editor dropdown (New display preset…, New audio preset…, Add TV…) opens the Library editor and, after Save, returns to the scene with the new item selected; Cancel also returns. A sidebar navigation abandons the return. An existing open Library draft is never overwritten: the user is sent to finish or cancel it. The empty Scenes page is a checklist of the building blocks, linking to Now and the Library. This supersedes "Audio presets is the main NavigationView page" below; the page names in older sections refer to Library › Audio, Library › Displays and Now.

## Leaving a page with unsaved changes — current

Choosing another category in the navigation pane, or Options, while the current editor has unsaved changes opens a dialog: Save, Discard or Keep editing. Unsaved means the draft differs from how it was when it opened; data that loads into an opened draft (supported display modes, live TV details) doesn't count. Save appears only when the draft can be saved: not in the TV setup guide, and never for Now, whose edits are applied rather than saved, so Now offers Discard or Keep editing. A failed save keeps you on the page with its error. Links inside an editor that return to the draft (Manage presets, View TV, New … preset) don't ask. An unchanged draft stays open when you leave, as before.

## Scene editor — current

A scene is edited as a vertical flow of fixed-size step cards in run order (Before, Prepare TV, Displays, Audio, FPS limit, After) joined by small down arrows, with When leaving set apart below under its own label. A card shows an icon, the title and a one-line summary, with a pencil and a bin. The card or its pencil opens the step's controls in a dialog: Done keeps the changes and Cancel restores the step as it was when the dialog opened. Nothing in the flow expands, and the column has a fixed width (720 DIP, or less in a narrow window), so the overview never changes size. Empty steps are dashed outlines the same size as filled cards, with the step's muted icon, a one-line hint and an Add button; they open the same dialog. Every card shares one icon column, and the arrows between steps are centred on it. Leaving the page from inside a dialog (New … preset, Manage) closes it and keeps the edits. There is no side summary panel: the flow is the summary. Failure policy and device wait live in Options, not in the scene. Scene cards mark the active scene and offer End beside Apply. See [Scenes](SCENES.md).

## Scenes first run — current

With no scenes, the Scenes page shows one card, "Start with your everyday setup", instead of the old four-step checklist and its page buttons. It reads the current screens, default output and volume, and the global FPS limit, and shows each with a checkbox (all on) and a live summary. A name box defaults to "Default", and one accent button, "Save as my Default scene", saves it; a quiet "Build a scene from scratch instead" link opens the editor. Saving changes no device and reuses a saved preset when the layout, or the output and volume, match exactly. It never sends people to Now: the step editor creates presets in place. While only one scene exists, a slim tip above the cards explains making the next scene, with "Got it" stored in preferences. See [Scenes](SCENES.md).

## Status area — current

The bottom status area is hidden when nothing needs attention. Work in progress shows one muted line with an indeterminate progress bar. A finished action shows a closable Success InfoBar that hides itself after 5 seconds. Something the user must read (an open draft blocking a flow, a cancelled pairing that left permission on the TV, unavailable live TV data) shows a Warning InfoBar that stays until closed. Errors stay red until closed. Routine hints such as Ready., Draft updated or Edit TV. are recorded for diagnostics but not shown; editor footers already explain save-versus-apply. This supersedes the earlier always-open InfoBar.

## Save and shortcut commands — current

Save is enabled only when the draft can be saved and has changed: it differs from how it opened, or it's a new item that isn't saved yet (a draft seeded from Now counts). Data that loads into an opened draft doesn't count as a change. The scene editor's footer says "No changes to save." meanwhile.

Scene, display and audio cards have a borderless ">_" (CommandPrompt glyph) button right after the name. It opens a dialog with the command by CLI name (unquoted, for example `director scene apply tv-gaming`) and by ID, each in selectable text with a Copy button and a one-line subtitle, plus `director scene end` for scenes that do something when left.

## Title bar — current

The window uses WinUI 3's TitleBar, like Windows 11 Settings: the pane toggle (☰) and "Director" sit in the title bar next to the caption buttons, and the navigation pane starts directly with Scenes. NavigationView has no pane title and hides its own toggle.

## Native Windows workspace

Use genuine WinUI 3 controls through Microsoft Windows Reactor. Follow the system theme, accent, fonts, hover, focus, scrolling, and control behavior. Keep custom painting and custom interaction machinery out of core controls.

Audio presets is the main NavigationView page. Use a compact ListView with one selectable row per preset and an Add/plus control. Each row has visible native icon buttons: green check for Apply, pencil for Edit, red bin for Delete. Accessible names and native tooltips explain each action. Selection alone never applies anything. Deletion uses a native ContentDialog.

Add/Edit stays in the same window: name TextBox, output ComboBox, optional Set volume ToggleSwitch and percentage NumberBox, and Save/Cancel. Saving stores choices only; Cancel discards the draft. The endpoint ID is not shown (it was noise for most people); `director audio outputs` prints it. Failure policy is global in Options.

InfoBar communicates status and errors. Disable mutations while work is active, keeping the UI responsive. Deferred features must not pretend to work. The default client size is 1000 x 700 logical pixels, minimum 760 x 600. Scroll constrained list/editor content, keep actions reachable, and make full device identity available on demand.

## Validation boundaries

Check native accessibility names/actions, dropdowns, disclosure state, optional volume drafts, Add/Edit/Cancel, duplicate errors, delete confirmation, persistence, and default/narrow layouts. Temporary test data must be removed. Record actual results in [implementation status](IMPLEMENTATION.md); do not infer physical audio success or real pointer/keyboard behavior from automation alone.

The user compared the native frontend on 2026-10-31 and explicitly selected WinUI 3 as the only interface. That selects the framework; it does not establish that every feature, accessibility path, or hardware recovery case is verified.
## Display presets

Use the same native navigation/list/editor/dialog vocabulary as audio. Use the accent + for creation and the secondary capture icon for quick snapshots. Each saved row exposes the same green Apply checkmark as audio, plus Edit and Delete. New presets keep verified changes automatically. When confirmation is enabled, a native Keep/Revert dialog has a live 15-second countdown; lack of confirmation restores the previous layout. Unchanged layouts report already active without a dialog. Capture initially shows a read-only monitor summary. The + and Edit open native per-monitor cards and the interactive arrangement map directly; Save and Cancel remain fixed below the scrolling editor. Preserve independent drafts across page switches, constrain scrolling, and test default/narrow sizes, empty/error states, cancellation, duplicate names, persistence, and audio isolation. This extension follows the ui-design product/craft guidance using the established native app; no external reference retrieval is needed.

## Per-monitor editor and remaining additions

Use the established native WinUI vocabulary for an arrangement map plus per-monitor cards. Keep resolution, refresh rate, rotation, enabled state, and primary selection discoverable; use supported-value dropdowns and refresh dependent options when resolution changes. Primary selection is exclusive across enabled desktop sources. Expose Extend/Clone grouping explicitly. Advanced DPI/HDR/color/identity controls must not overwhelm the main task or silently override omitted settings. This editor belongs to display presets; scenes select reusable presets and actions.

Rotation dropdown entries must show a display SVG rotated to the corresponding orientation, with the monitor stand as the orientation reference. Pair the graphic with Landscape, Portrait, Landscape flipped, or Portrait flipped; degrees are supporting detail. The stand distinguishes flipped orientations; do not add an arrow inside the display. Keep native keyboard/accessibility labels and preserve Save versus Apply semantics.

The extended-layout editor, draggable map, supported-mode dropdowns, primary/enabled controls, and SVG orientation choices are implemented. Coordinates are screen-tooltip information. Physical Identify, clone-group editing, and DPI/color controls remain deferred; HDR is now capability-gated.
## Cohesion and direct manipulation — current

Audio and display list pages use one shared heading/toolbar component. Keep the accent + at the top right in the same position. Do not show Refresh buttons; clean pages reload on navigation. Display capture is a secondary camera icon immediately before the display +. Remove duplicate text Add buttons from empty states. Both edit footers place Cancel then Save on the right, with the save-only explanation on the left.

The display pencil opens the complete editor directly. There is no Customize layout step. A boxed map supports pointer-captured dragging with stable scale during the gesture, edge snapping, and overlap handling on drop. Show technical position details in screen hover tooltips, rather than numeric fields. Keep the primary display's Enabled switch greyed out and put the reason on a transparent, enabled hover layer above it so it can still be hovered. New GUI and CLI presets default to confirmation off; preserve existing choices.

## Current setup alternative (now "Now")

Now puts the shared monitor editor and audio output/volume controls into one scrolling page. Keep separate Apply display changes and Apply audio changes actions, plus Discard edits. This view neither edits nor saves preset documents. Stage edits before Apply; preserve pending edits across navigation. New page roots prevent native scroll state from leaking across pages. Theme/appearance switching was explicitly excluded by the user.

Orientation choices use equally sized compact rows, left-aligned icon/text, stand-only SVGs, and a standard chevron. Re-key the picker after selection so its native flyout closes, including when choosing the current orientation.

## Proposed overview refinement (deferred)

The user suggested monitor drawers/expanders that open and close from the overview. Record this for a later layout pass; feature support takes priority now. No overview drawer redesign is authorized by that suggestion alone.

The HDR availability explanation is tooltip content, not a permanent subtitle. An enabled transparent layer over an unavailable HDR switch owns the reason tooltip so a greyed control can still explain itself.

## TVs

Use the same shared page heading, top-right accent +, compact native list, pencil, bin, fixed Cancel/Save footer, and deletion dialog. Pair and connection-check actions have accessible names and native tooltips. Never contact a TV just by navigating, selecting a row, saving, or editing a Windows display link. Pairing is an explicit background operation with an on-TV approval instruction and a bounded wait. Check stays disabled until a pairing is saved. Display links are optional device metadata, independent of presets; disconnected links remain visible. Keep independent TV drafts across navigation. Do not label saved pairing as currently online. Legacy unencrypted mode requires explicit selection and explanation.

## TV setup guide and wake

Add TV now uses seven native steps: name, IPv4/connection method, explicit pairing plus automatic input/network reads, PC input, wake-address/manual fallback, optional Windows display, and review/save. Use the shared heading with a step counter, a constrained scroll area, and fixed Back/Cancel/primary controls. Preserve progress across Back/navigation. Pairing runs before saving; successful pairing survives a later input-read failure. Invalid/incomplete steps stay gated. A refused MAC read is a warning with manual entry or skip available.

Existing TVs retain a compact metadata editor with Read TV details. Add a power-icon Wake action to the list, disabled without a stored MAC. Report Wake request sent rather than physical power state. Setup stores device connections and identities; scene action preferences stay separate. Finishing setup does not switch input, wake, or change displays/audio.

Manual MAC entry is an advanced opt-in ToggleSwitch, off for new setup. Reveal the field only when enabled; reopen existing manual configurations enabled. Preserve hidden draft text, gate validation only while enabled, and leave learned MACs untouched. The explicit Wake action remains unchanged; status-aware power controls belong to the proposed basic control modal.

## Basic TV control modal

The row’s remote-control SVG opens a native ContentDialog, replacing the previous Wake-only icon. Show fresh status and a refresh icon, one power button with explicit action text, current input, a target dropdown, and Switch input. Keep normal state compact; show progress/results/errors only when useful. Do not apply on selection or change stored PC/display mappings. On/Screen off/Screen saver uses Turn off; Off/Standby/Unknown uses Wake with a saved MAC. Never label an unreachable device Off. Preserve the displayed action’s intent if status changes during the fresh check.

Keep Close controls accessible at the minimum window size. Work runs in the background; while pending, disable action controls and retain window close protection. Closing the modal does not cancel an already-issued command. No permanent polling/tray process. Use native accessible action names (Turn off TV/Wake TV) and explain unavailable wake data. CLI/scenes remain explicit actions rather than alternating power toggles.



## Shared action SVGs and TV action cleanup

Use one shared red bin SVG for Delete/Remove across audio presets, display presets, and TVs; keep existing accessible names, hover labels, and confirmation dialogs. Apply remains the existing green check, explicitly unchanged by user request. The TV row has three actions: remote-control SVG, pencil, bin. Remove standalone Pair again/Check connection actions and the editor’s Read TV details button. Pairing and initial reads stay in the guide, with Retry only for a failed guide information read. Paired-TV edit refreshes available data automatically, preserving saved choices when unavailable. Connection changes route through the guide before final save.


## TV control responsiveness — current

The modal opens immediately into Checking, without making the whole application busy. Keep Close and Wake (when a MAC is saved) usable during a read. Read-only refresh is separate from mutation progress, and stale responses cannot replace a newer action or reopened dialog. Use a 340-DIP content column, compact power/status row and an input selector beside Switch. Hide empty selectors. No outer scrollbar crosses controls. Keep diagnostic errors behind Technical details; a bounded scroll area belongs only inside that disclosure. Modal progress/results remain local rather than duplicating InfoBars behind the dialog. Mutations still use the shared lock and close protection. This supersedes the earlier instruction to disable all controls for status reads.


## Options and diagnostics

Keep application options separate from device controls. The lower navigation footer exposes Options with a native gear/button; its page contains Diagnostics and Open logs folder. Use the same native typography, accent and constrained/scrollable content conventions as the other views. Show a known logging failure in this area rather than adding diagnostic controls to TV rows or modals. Preserve the original TV command diagnostic when an automatic status refresh completes or times out.


## TV power progress — current

Use Waking… consistently for a pending wake, Turning off… for a pending off, and Off · assumed after an accepted turn-off. Keep one power control; disable repeated commands/refresh during the transition and retain Close. Replace unavailable input controls with a short explanation. Expired wake waits show Not responding yet/Not awake yet with Retry wake. Confirmed input changes update in place; do not flash Checking or clear unrelated controls after every command. Automatic readiness checks belong only to power-on transitions, and one-off recovery checks to uncertain commands. Diagnostic details stay collapsed and command/status causes stay separate.


## Optional TV setup tip

New display presets may show a compact neutral card above the name field: Setting up a TV?, a short optional explanation, Add TV, Don't show again, and an accessible close button. Match existing native typography, muted foreground, border radius and button styles. Avoid warning colors, automatic dialogs or entrance animations. The card is absent while preferences/catalog load, when a TV is already saved, when editing an existing preset, or after dismissal. Show linked TV metadata and a View TV action in the appropriate monitor card instead of another reminder.

Opening TV setup preserves the complete display draft. Close dismisses the card for this session; Don't show again persists under Windows Local AppData. Options → Show TV setup tips restores it. Existing TV drafts resume rather than being overwritten. Capability detection, temporary activation, remembered modes and automatic wake/input preparation are still proposed display-setup work; the tip does not claim those are implemented.


## Inline display detection — implemented

Each preset monitor card offers Set up this display/Detect display options. Its inline explanation describes temporary enabling, flicker and restoration before the user starts. Linked TV preparation is an explicit alternate action. Progress replaces stale settings and instructions inside the same card, showing a native indeterminate bar, current phase and Cancel detection. No loading popup is introduced. On success, the restored-layout result leads into real detected resolution/rate/HDR controls; remembered choices are labeled when inactive. Failures stay local to the card, retain recovery state and offer Restore previous layout when needed. The page-wide status must not say that screens are unchanged while temporary setup is active.


Display cards keep the enable switch in the title row; disabled cards hide mode controls, the primary action and completed-detection copy. The switch remains greyed out for the primary display with its existing explanatory hover tooltip.

Wide display editors use responsive monitor-card columns rather than a 650-DIP form across a full-screen window. Keep normal mode/HDR controls visible after discovery, hide successful setup copy, and reduce repeat detection to a subtle Detect again action.

Monitor-card headers align the display name, a Primary indicator or Make primary action, and the enable switch. The primary indicator is a label rather than a disabled action. Disabled displays omit the primary action.

## Preset preview grids

Audio and display pages use the same native GridView card vocabulary, title row and apply/edit/delete placement. Display cards show a saved-layout miniature with grouped mirror numbers plus a readable per-monitor legend (resolution, exact-rate label and HDR intent); disabled screens are listed as Display off. Details remain scrollable for larger layouts. Audio cards show the remembered/current device name and either a volume percentage or Keep current device volume. Merely selecting a card does not apply it. Normal native hover/selection/focus behavior is preserved.

The mirror selector sits in each enabled display card. Mirrored outputs share one draggable desktop tile. Primary means a source group; an individual primary-group member can be disabled while another remains. The final primary output remains protected by the disabled switch and tooltip. The scene editor is described in [Scenes](SCENES.md).

## Sound — current

Audio presets and the Library › Audio page are retired; this supersedes the Audio page, audio preset cards and "New audio preset…" above. Sound lives in two places. Now's Sound card has live controls: an output picker, a mute button and a volume slider, and the same row for the microphone; each change applies at once, with no Apply button, and **Save as scene…** opens a scene whose Sound step holds the current output and volume. A scene's **Sound** step dialog holds an Output picker (Leave as is, the devices Director knows, or "TV's sound · TV name"), a Set volume switch with a slider, a Mute choice (Leave as is, Unmute, Mute), "Use a different device for calls" and "Change microphone" checkboxes that reveal their pickers, and Relink device when a saved device is offline. Like every step dialog, it has no tooltips.

A scene card's top area shows the scene's main change. With a display preset it is the layout miniature; otherwise a 124 px panel shows a large icon and the main value: for sound, the output name with a volume bar (or "Muted") and the calls and mic devices; for a TV, its name and input; then the FPS limit, then the number of actions.
