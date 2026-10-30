# TODO

## Done in 0.0.7

- [x] **Build releases in CI (GitHub Actions), not on this machine.** `.github/workflows/release.yml` builds the
  Windows setup (windows-latest) and the Linux tarball (ubuntu-22.04, so it runs on glibc 2.35+). Run it by hand for a
  dry run; push a `vX.Y.Z` tag to get a draft pre-release with both files and `SHA256SUMS.txt`.
- [x] **Static C runtime on Windows** (`.cargo/config.toml`): the exe no longer needs `VCRUNTIME140.dll`, which a clean
  Windows doesn't have (winget validation failed with 0xC0000135). `make-windows-setup.ps1` refuses an exe that still
  imports it.

## Done in 0.0.8

- [x] **Upgrade and uninstall while the app runs.** A silent setup (winget) used to exit 1 when the app was running:
  Inno's `AppMutex` prompt defaults to Cancel. Now Restart Manager closes it (0.0.8+ exits cleanly on
  `WM_ENDSESSION`, older copies are ended), the setup starts it again in the tray, and the uninstaller closes it first.
  Also gives an orderly exit at sign-out and shutdown.

## Next

- [ ] **Code signing via SignPath Foundation** (free for open source; signs only artifacts built in CI, which the
  workflow now does). Apply at signpath.org, then add their signing step to the `windows` job. Stops the SmartScreen
  warning on direct downloads once reputation builds.
- [ ] **Each release:** update the winget manifest after publishing (`wingetcreate update MrErrorSan.ThinkVisionControl`,
  after `winget upgrade Microsoft.WingetCreate` so it writes the current manifest schema).
