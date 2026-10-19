# Changelog

Notable changes in each release. Versions follow [Semantic Versioning](https://semver.org/).

## [0.2.0-alpha] — 2026-10-19

- Updates: Traydex checks GitHub at startup and once a day and offers a new version at the bottom of the window.
  *Update* downloads the installer or portable zip for your architecture, verifies its SHA-256, installs it and
  restarts Traydex. Alpha builds get alpha and stable releases, stable builds only stable ones.
- A button next to the version at the bottom of Settings checks for updates right away; automatic checks can be
  turned off in Settings.

## [0.1.1-alpha] — 2026-10-19

- Text no longer sits up to a pixel above the middle of buttons, fields and rows: letters land on the nearest pixel
  row to where they belong instead of the one above.

## [0.1.0-alpha] — 2026-10-18

First public release.

- Builds for x64, ARM64 and 32-bit x86, each as an installer and a portable zip.
- Portable mode: with `portable.txt` next to `Traydex.exe`, all data stays in the `data` folder beside it.

- Shelf that appears while dragging, by a hot corner, by `Ctrl+Shift+Space` or from the tray.
- Files, folders, text, links and images from any app; drag back out one by one or all at once.
- Quick action tiles: folder, ZIP, extract, JPEG/PNG, compress, PDF, OCR, QR code, to phone, rename, web search,
  Imgur and custom upload, file hash with VirusTotal, GIF, video download, copy path, Base64, run a program.
- Plugins in PowerShell, Python, `.bat` or `.exe`.
- Preview, notes, search with typo and keyboard-layout tolerance, list and grid views.
- Light and dark themes, transparency and blur, six languages.

[0.2.0-alpha]: https://github.com/ipavlin98/traydex/releases/tag/v0.2.0-alpha
[0.1.1-alpha]: https://github.com/ipavlin98/traydex/releases/tag/v0.1.1-alpha
[0.1.0-alpha]: https://github.com/ipavlin98/traydex/releases/tag/v0.1.0-alpha
