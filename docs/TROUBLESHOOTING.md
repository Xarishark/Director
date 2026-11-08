# Troubleshooting, logs and backups

## When a scene fails

**Options → View logs** shows the result of the latest scene run, from the window or the CLI, followed by recent log entries. Look for the step that failed and whether the restore of your previous settings succeeded.

Turn on **Notify when CLI scenes fail** in Options to get a Windows notification when a scene started from a shortcut, script or Stream Deck fails. Choosing the notification opens that run's result in Director.

Common causes:

- **A device didn't appear in time.** A TV's HDMI audio, or a screen that's slow to wake, can take longer than the device wait. Raise **Device wait (seconds)** in Options (up to 120 seconds).
- **A display preset no longer matches your hardware.** A new cable, port or driver can change a screen's Windows device path. Edit the preset, or capture it again.
- **A saved audio device has a new ID.** Use **Relink device…** in the scene's Sound step. See [Sound](SOUND.md).
- **The TV didn't respond.** Check that it's on the same network, has the same IP address, and has network standby or mobile power-on enabled for Wake. See [TVs](TVS.md).

## Logs

**Options → Open logs folder** opens `%LOCALAPPDATA%\Director\Logs`. The app and the CLI both write there.

- `director.log` is the current file; up to four older files (`director.1.log` to `director.4.log`) are kept, each about 1 MiB.
- Each line is a JSON record with the time (UTC), process ID, operation ID, step, outcome and details. Open them in a text editor, or parse them as JSON lines.
- Logs contain device names and IDs, TV input IDs and Windows error codes. They never contain pairing keys, tokens, certificates, MAC addresses or raw TV messages.
- Logging never holds up a device change: if the disk is busy or full, records can be dropped, and the next record says how many.

A "request sent" record only means the command left Director; look for the acknowledgment and the final result separately. For TV controls, the original command error is kept apart from any later status check, so a timeout afterwards doesn't hide why the command failed.

## Crash reports

If Director closes unexpectedly, it saves a report in `Logs\Crashes` with the error, a backtrace, the Director and Windows versions and the last 40 log records. The next time it starts, it offers the report once:

- **Report on GitHub** opens a new issue in your browser with the summary, version and backtrace filled in. Nothing is sent until you submit it. The log lines stay in the file; attach it if you're happy to share them.
- **Report a problem** in Options opens a new issue for anything else.

Native crashes that bypass Rust's error handling appear only in Windows' Event Viewer, under Windows Logs → Application.

## TV compatibility reports

For Samsung and TCL TVs, **Export report…** in the TV editor (or `director tv report <tv>`) saves an allowlisted JSON report to `%LOCALAPPDATA%\Director\Reports`. It contains no keys, addresses or raw logs. See [Test a Samsung or TCL TV](TV_TESTING.md).

## Backups

**Options → Export backup** saves your scenes and display presets to one `.director-backup` file. **Restore backup** shows what it will replace before you confirm, and flags device links to review afterwards.

TV pairing keys are encrypted for your Windows account on this PC, so they're never included: after restoring on another PC, pair each TV again. Scene app paths and links come back as they were saved; review them before applying a scene.

To back up everything by hand, copy the `%LOCALAPPDATA%\Director` folder while Director is closed. TV pairings in that copy only work for the same Windows account on the same PC.
