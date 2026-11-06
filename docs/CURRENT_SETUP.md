# Now

Now (formerly Current setup) is an alternative workspace for adjusting the active Windows configuration without creating or editing presets. It shares the display editor and backend with saved presets, rather than maintaining a second implementation of monitor controls.

## Using the view

Open **Now** in the navigation pane. Displays and Audio appear in one scrolling view:

- Arrange screens by dragging them in the box; hover for technical details.
- Enable/disable monitors, choose the primary, resolution, refresh rate, HDR, and orientation.
- Choose the default audio output and adjust its volume.
- Apply display changes or audio changes independently. No preset name or Save action is involved.
- Discard edits to reread the actual Windows state.
- **Save as display preset…** or **Save as audio preset…** to open a new Library preset seeded with what Now has staged, including edits you haven’t applied. The audio preset sets a volume only if you changed it in Now. Saving happens in the normal Library editor, and Cancel discards it.

Controls stage changes until the corresponding Apply button is pressed. This allows a multi-monitor arrangement to be prepared before reconfiguring hardware. Display Apply uses verification/recovery and keeps verified changes without a Keep/Revert prompt. Audio Apply changes Console and Multimedia only, preserving mute and leaving Communications alone. Volume is changed only if the user edits it.

Pending live edits survive page navigation independently of preset drafts. Opening a clean Now page rereads Windows. After display Apply, audio outputs are reread so HDMI endpoints can appear; pending audio selections are preserved. Each page has an independent native control tree to prevent scroll positions leaking between pages.

## Safety and boundaries

Display and audio Apply run on background workers under the same cross-process operation gate, with normal window-close protection and the shared backend recovery behavior. They are independent operations, not a scene transaction combining both categories. Snapshot/readback errors remain visible.

HDR requires a supported display and an available HDR-specific Windows API. Unavailable controls are disabled. Clone-group editing, physical Identify, DPI/color-depth/VRR settings, microphone/Communications controls, and automatic device relinking remain outside this view.

Now never writes preset files itself; Save as preset hands a draft to the Library editor. Its temporary display draft is held only in memory; errors and actual state after an attempted application are reread. It does not include appearance/theme controls.
