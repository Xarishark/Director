# Diagnostics

Director records local diagnostics automatically for the desktop app and CLI. In the window, choose **Options → Open logs folder**. This opens the Windows-known Local AppData location, normally `%LOCALAPPDATA%\Director\Logs`.

## Files and retention

`director.log` is the current file. Up to four older files, `director.1.log` through `director.4.log`, are retained. Each file is capped at approximately 1 MiB (records are bounded); oldest records are replaced on rotation. Each line is a JSON object with readable UTC timestamp, Unix milliseconds, process ID, operation ID, step, outcome, and diagnostic detail. Read the files in a text editor or parse them as JSON lines. Operation IDs are process-local; use timestamp and process ID with them.

A worker writes logs through a bounded 512-record queue. Ordinary UI/event logging never waits for disk access or another process. A separate file lock coordinates writers/rotation across app and CLI instances. If the queue fills, records may be dropped and the next available event reports the dropped count. Exit flush waits at most two seconds. Logging is best effort: a disk error must not stop a device operation. Options reports a known logging failure, updated when that view renders. Logs can contain device IDs, input IDs, device names in audio reports, and Windows error information; pairing keys, protected credentials, certificate bytes, MAC addresses, and raw TV messages are not deliberately recorded.

## Coverage

- Application startup, CLI exit, GUI shutdown, background jobs, and global operation failures.
- Audio apply progress, verification/recovery progress, and final report.
- Display snapshot, preparation, validation, temporary apply, verification, keep/revert, commit, restoration, and final report.
- TV connection, registration, authenticated session, request sent, reply/timeout, parsed power state, command acknowledgment, fresh turn-off/input checks, wake dispatch, and command result.
- TV dialog token/read revision connects an action to its separate follow-up check. An unavailable status snapshot records its notes even though snapshot collection itself returned successfully.

A request-sent event means bytes were handed to the transport, not that the TV accepted the command. Look for command acknowledgment and the final action result separately. A finished span records elapsed time; it does not imply success. Physical panel state, HDMI signal, and network disappearance must not be inferred from acknowledgment alone.

The modal also preserves the original command diagnostic independently of follow-up status errors. A later timeout cannot erase why an action failed.

## Development checks

Unit tests exercise rotation limits, complete concurrent records, Unicode-safe detail limits, and JSON escaping. Native fixtures verify Options/folder-open acceptance and retention/correlation of a blocked TV command followed by an unavailable status response. Fixtures do not send power/input commands to the physical TV. Windows accepted the folder-open request; inspecting Explorer's folder through Shell automation was unavailable in the test environment.


Folder opening starts Windows' own explorer.exe from the Windows installation directory and passes the log folder as a separate argument. This avoids the folder-association handoff that returned success without opening a visible window. The status reports a completed request rather than asserting that a window appeared, and includes the runtime log location and writer state.



## Samsung compatibility sharing

Samsung setup/Edit TV provides Export report, and CLI provides director tv report <name>. The report in the Local AppData Director/Reports folder is a separate allowlisted JSON document, not a copy of raw application logs. It includes observed model/firmware, paired state, tested direct ports and user confirmations; it omits credentials, certificate bytes, addresses, MACs, aliases, TV/device IDs, raw responses and arbitrary errors. Nothing is uploaded automatically. See [SAMSUNG.md](SAMSUNG.md) for the testing procedure.

## Compatibility reports (format 2)

Samsung/TCL reports now include explicit Not tested/Yes/No feedback for displayed status, physical turn-off and physical wake; optional TV model/software and Wi-Fi/Ethernet information; export timestamp, Windows version/build, architecture, executable SHA-256, and wake-address-presence/count flags. Selected-TV structured history is read from bounded rotated logs, with action, outcome, timing, reported power, coarse failure category and an optional numeric system code. No raw error strings or full log records are exported. Missing/unreadable history is reported explicitly. Only actions run with this build are captured; reproduce the failure before exporting. Pairing steps expose the same export action even before authorization succeeds. User observations are distinct from command send results.
