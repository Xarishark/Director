# Security policy

## Reporting a vulnerability

Please report security problems privately through GitHub: open the repository's **Security** tab and choose **Report a vulnerability**. Don't open a public issue, discussion or pull request for them.

Include what you found, the Director version, and steps or a proof of concept if you have one. You'll get an answer as soon as possible, and you'll be credited in the fix's release notes unless you'd rather not be.

Only the [latest release](https://github.com/Xarishark/Director/releases/latest) gets security fixes.

## What's in scope

Director runs locally and controls devices on your home network. Areas where a bug could matter:

- **TV credentials**: pairing keys, tokens, certificate pins and Android TV client keys, encrypted with Windows DPAPI under `%LOCALAPPDATA%\Director`. They must never appear in logs, backups, compatibility reports or CLI output.
- **TV connections**: the TLS connection and certificate checks for Android TV Remote v2, certificate pinning for LG and Samsung, and parsing anything a device on the network sends back.
- **Local data**: settings, backups and reports written to disk, and what crash reports and the **Report on GitHub** link include.
- **The installer**: `setup.exe` downloads the Windows App Runtime from Microsoft and checks its SHA-512 hash before running it.
- **Updates**: Director downloads setup only from this repository's GitHub releases, over HTTPS, and runs it only if its SHA-256 matches the digest GitHub reports for the release asset.
- **The `director-run:` link handler** that the installer registers.

Director's own design assumes a trusted home network. Problems that need an attacker who already controls the PC or the Windows account are out of scope.
