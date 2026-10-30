# Monitor Manager

A planned Windows desktop application for capturing and applying independent display and audio profiles, optionally combined with TV control, frame limits, and final application/URI actions.

**Status: planning and documentation only.** There is no application implementation, build, or release yet. Rust and WinUI 3 are selected; SDK versions, dependencies, distribution, and licensing are not yet finalized.

## Agreed direction

- Rust application engine with native Windows interface.
- No Chromium or web frontend.
- The GUI and CLI use the same profile engine; command-line operations do not initialize the GUI.
- Display profiles and audio profiles can be created and applied independently.
- Optional combined setups compose those profiles and additional actions.
- Capture current settings first; add guided custom creation later.
- Remember audio devices and wait for unavailable endpoints, such as HDMI audio, when applying a setup.
- Native Windows notifications for operation outcomes.
- Configurable failure behavior: revert previous settings by default, or stop and keep completed changes.
- Prioritize basic functionality before system tray operation or starting with Windows.

## Documentation

| Document | Purpose |
| --- | --- |
| [Requirements](docs/REQUIREMENTS.md) | Agreed behavior, implementation order, acceptance criteria, and open scope |
| [Architecture](docs/ARCHITECTURE.md) | Proposed organization of the shared engine, GUI, CLI, and integrations |
| [Decision record](docs/DECISIONS.md) | Accepted decisions and proposals that still need confirmation |

The local pre-1.0 checklist is a private planning aid and is deliberately ignored by Git. Repository documentation records agreed requirements and clearly labels unconfirmed proposals.

## Planned usage

These commands illustrate the intended interface. They do not exist yet; the executable name and syntax remain proposals.

```text
app capture displays "Desk monitors"
app capture audio "Headphones"
app apply displays "Desk monitors"
app apply audio "Headphones"
app apply setup "Couch gaming"
```

## Development and documentation policy

- Remain in planning until implementation is explicitly authorized.
- Update requirements and the decision record when scope or behavior is agreed.
- Keep implemented behavior, proposed behavior, and hardware validation clearly distinguished.
- Add build and test instructions with the first implementation; do not present planned commands as working ones.
- Keep credentials, real device inventories, runtime state, logs, and local planning notes out of Git.
- Preserve existing scripts and tools during development unless changes are separately authorized.
- Do not commit or push without explicit authorization. Retain the existing Git signing configuration.

No application license has been selected yet. Distribution terms are still to be decided.
