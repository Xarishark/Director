# Contributing to Director

Thanks for helping. Bug reports, TV compatibility reports and pull requests are all welcome.

## Reporting a problem

- **Crashes**: Director saves a crash report and offers **Report on GitHub** the next time it starts. That opens a pre-filled issue; attach the crash file from `%LOCALAPPDATA%\Director\Logs\Crashes` if you can.
- **Bugs**: open an issue with your Windows version, your Director version (`director --version` or **Options → About**), the steps that went wrong, and recent log lines from **Options → Open logs folder** if they help. Logs never contain pairing keys, but they can contain device names; trim anything you'd rather not share.
- **TV compatibility**: follow [Test a Samsung or TCL TV](docs/TV_TESTING.md) and attach the exported report.
- **Security issues**: please don't open a public issue; see [SECURITY.md](SECURITY.md).

## Building

You need Windows 11 24H2 or newer, Rust 1.95 or newer with the MSVC toolchain, and the Visual Studio C++ Build Tools. Then:

```powershell
cargo run
```

[docs/DEVELOPMENT.md](docs/DEVELOPMENT.md) covers the toolchain, the tests and preview fixtures, packaging and CI.

## Pull requests

1. Branch from `main`: `feat/<topic>` for features, `fix/<topic>` for fixes, `docs/`, `chore/` or `refactor/` otherwise.
2. Keep each pull request to one change, and each commit to one step. Commit messages follow [Conventional Commits](https://www.conventionalcommits.org) like the existing history, for example `fix(scenes): keep the Return target after a rename`.
3. Run the same checks as CI before pushing:

   ```powershell
   cargo fmt --all -- --check
   cargo clippy --locked --all-targets -- -D warnings
   cargo test --locked -- --test-threads=1
   ```

4. Update the docs in `docs/` and the README when behaviour changes, and explain design choices in the pull request description.
5. Open the pull request against `main`. The **Windows x64 build** check must pass.

Some things can't be tested in CI: applying layouts, switching audio, and talking to a real TV. Say in the pull request what you checked by hand and on which hardware. Don't change devices on someone else's machine without asking.

### Conventions

- Use the shared UI vocabulary in `src/gui/ui.rs` (type ramp, spacing, theme brushes, page, section and card builders) instead of raw sizes and colours.
- Don't put tooltips inside a `ContentDialog`: the pinned Reactor closes the window. See [docs/DEVELOPMENT.md](docs/DEVELOPMENT.md#winui-rules-worth-knowing).
- Device and settings changes take `audio::windows_audio::OperationLock`, run off the UI thread, verify by reading back, and restore what they changed on failure.
- Talk to Windows and TVs directly rather than through helper programs. New dependencies need a reason, and their licenses must be accepted in `packaging/about.toml`; regenerate `THIRD_PARTY_LICENSES.md` when they change.

By contributing, you agree that your contribution is licensed under the [MIT License](LICENSE).
