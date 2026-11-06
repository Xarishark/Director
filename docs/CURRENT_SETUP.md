# Now

Now (formerly Current setup) is an alternative workspace for adjusting the active Windows configuration without creating or editing presets. It shares the display editor and backend with saved presets, rather than maintaining a second implementation of monitor controls.

## Using the view

Open **Now** in the navigation pane. Displays and FPS limit come first and wait for **Apply changes**, in the bottom-right corner of the display map. Below a divider, Sound applies as you change it:

- Arrange screens by dragging them in the box; hover for technical details.
- Enable/disable monitors, choose the primary, resolution, refresh rate, HDR, and orientation.
- Sound changes apply right away: choose the default output or microphone, move a volume slider, or press a mute button. See [Sound](AUDIO.md).
- **Apply changes** applies everything staged: displays first, then the FPS limit (only if the display change succeeded). Its tooltip says what is staged. No preset name or Save action is involved.
- **Discard** (on only while something is staged) rereads the actual Windows state.
- **Save as display preset…** opens a new display preset seeded with what Now has staged, including edits you haven’t applied. **Save as scene…** opens a new scene with its Sound step dialog open, holding the current output and microphone with their volume and mute state. Saving happens in the normal editor, and Cancel discards it.

Display and FPS controls stage changes until Apply changes is pressed, so a multi-monitor arrangement can be prepared before reconfiguring hardware. Display Apply uses verification/recovery and keeps verified changes without a Keep/Revert prompt. Sound controls don't stage: choosing an output makes it the default for all three roles, a volume slider sends its level once it rests for 150 ms, and mute toggles. Each change is verified and put back if Windows does not confirm it.

Pending live edits survive page navigation independently of preset drafts. Opening a clean Now page rereads Windows. After display Apply, audio devices are reread so HDMI endpoints can appear. Each page has an independent native control tree to prevent scroll positions leaking between pages.

## Safety and boundaries

Display Apply and each sound change run on background workers under the same cross-process operation gate, with normal window-close protection and the shared backend recovery behavior. They are independent operations, not a scene transaction combining both categories. Snapshot/readback errors remain visible.

HDR requires a supported display and an available HDR-specific Windows API. Unavailable controls are disabled. Clone-group editing, physical Identify, DPI/color-depth/VRR settings, a separate Communications output, and automatic device relinking remain outside this view.

Now never writes preset or scene files itself; its Save buttons hand a draft to the normal editor. Its temporary display draft is held only in memory; errors and actual state after an attempted application are reread. It does not include appearance/theme controls.
