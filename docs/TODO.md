# TODO

## Done in 0.0.7

- [x] **Build releases in CI (GitHub Actions), not on this machine.** `.github/workflows/release.yml` builds the
  Windows setup (windows-latest) and the Linux tarball (ubuntu-22.04, so it runs on glibc 2.35+). Run it by hand for a
  dry run; push a `vX.Y.Z` tag to get a draft pre-release with both files and `SHA256SUMS.txt`.
- [x] **Static C runtime on Windows** (`.cargo/config.toml`): the exe no longer needs `VCRUNTIME140.dll`, which a clean
  Windows doesn't have (winget validation failed with 0xC0000135). `make-windows-setup.ps1` refuses an exe that still
  imports it.

## Next

- [ ] **Code signing via SignPath Foundation** (free for open source; signs only artifacts built in CI, which the
  workflow now does). Apply at signpath.org, then add their signing step to the `windows` job. Stops the SmartScreen
  warning on direct downloads once reputation builds.
- [ ] **Each release:** update the winget manifest after publishing (`wingetcreate update MrErrorSan.ThinkVisionControl`,
  after `winget upgrade Microsoft.WingetCreate` so it writes the current manifest schema).
