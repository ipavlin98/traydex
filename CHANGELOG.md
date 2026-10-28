# Changelog

Notable changes in each release. Versions follow [Semantic Versioning](https://semver.org/).

## [0.5.1-alpha] — 2026-10-28

- Removing a card that a note mentions asks first, since the note's link to it would break.
- The mouse wheel scrolls the shelf, the pages and folder and archive previews smoothly instead of jumping.
- Settings: *Font size* became *Scale*, a drop-down list from 90% to 150% that enlarges the whole interface. A font
  size chosen before carries over as the nearest scale.
- The version at the bottom of Settings opens the Releases page on GitHub.
- Line icons are sharp at any Windows and interface scale: their lines land on whole screen pixels instead of blurring
  across two.

## [0.5.0-alpha] — 2026-10-27

- The window opened by the hot corner takes keyboard focus even when another app is in front, so typed text no
  longer goes to the browser or editor behind it.
- Pages that load in the background (file hashes, sharing, renaming, media) no longer shift by a pixel when the
  result replaces the placeholder at larger font sizes.
- Zoom images in the preview: the mouse wheel zooms around the cursor, dragging moves the enlarged image, a
  double-click zooms in or back to fit.
- Mentions in notes: type `\` (or `@`) to link a file, picture, folder or link from the shelf. Hover a mention to peek
  at it, click to open its preview and `Esc` to come back, `Ctrl`+click to find the card on the shelf, or drag it out
  like the card itself. `Backspace` after a mention turns it back into `\` with the list open.
- Pasting or dropping files and screenshots into an open note puts them on the shelf and mentions them in the text.
- The preview of a card mentioned in notes links to them at the bottom; the mouse wheel switches between the notes.
- Notes have a little more space between lines.
- Pop-ups line up exactly with what they open from at 125% and 150% display scaling.

## [0.4.0-alpha] — 2026-10-22

- Delete to the Recycle Bin (Settings, on by default): files and notes stored only in Traydex go to the Windows
  Recycle Bin when their cards are removed, without a question. Restoring a file there brings its card back.
  With the setting off, Traydex warns before deleting them for good.

## [0.3.6-alpha] — 2026-10-22

- A tile whose action failed keeps showing its description instead of a red "Last run failed"; the error itself is
  still shown as a message.

## [0.3.5-alpha] — 2026-10-22

- Cards of files and folders show the folder they're in after the type and size, or *Stored only in Traydex* for
  pasted images and files kept by Traydex itself.
- Removing a card whose file is stored only in Traydex, or clearing the shelf with such cards, warns first, since
  the file is deleted for good.

## [0.3.4-alpha] — 2026-10-22

- A click on the message bar at the bottom of a page dismisses it, successes included, like a click on the message
  icon in the title bar.

## [0.3.3-alpha] — 2026-10-22

- The loading spinner turns around its own center instead of wobbling, and icons are no longer stretched by half a
  pixel at fractional Windows scales.

## [0.3.2-alpha] — 2026-10-21

- The hot corner works while a window run as administrator is in front.

## [0.3.1-alpha] — 2026-10-21

- Middle-clicking an item on the shelf shows it in its folder, like Ctrl+click.

## [0.3.0-alpha] — 2026-10-21

- Watched folders: add folders such as *Downloads* in Settings, and their files and subfolders appear on the shelf by
  themselves, including new ones. Cards of files deleted or moved out of the folder go away; a card you remove
  yourself doesn't come back. Removing a folder from the list also removes its unpinned cards.

## [0.2.0-alpha] — 2026-10-19

- Updates: Traydex checks GitHub at startup and once a day and offers a new version at the bottom of the window.
  *Update* downloads the installer or portable zip for your architecture, verifies its SHA-256, installs it and
  restarts Traydex. Alpha builds get alpha and stable releases, stable builds only stable ones.
- A button next to the version at the bottom of Settings checks for updates right away; automatic checks can be
  turned off in Settings.
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

[0.5.1-alpha]: https://github.com/ipavlin98/traydex/releases/tag/v0.5.1-alpha
[0.5.0-alpha]: https://github.com/ipavlin98/traydex/releases/tag/v0.5.0-alpha
[0.4.0-alpha]: https://github.com/ipavlin98/traydex/releases/tag/v0.4.0-alpha
[0.3.6-alpha]: https://github.com/ipavlin98/traydex/releases/tag/v0.3.6-alpha
[0.3.5-alpha]: https://github.com/ipavlin98/traydex/releases/tag/v0.3.5-alpha
[0.3.4-alpha]: https://github.com/ipavlin98/traydex/releases/tag/v0.3.4-alpha
[0.3.3-alpha]: https://github.com/ipavlin98/traydex/releases/tag/v0.3.3-alpha
[0.3.2-alpha]: https://github.com/ipavlin98/traydex/releases/tag/v0.3.2-alpha
[0.3.1-alpha]: https://github.com/ipavlin98/traydex/releases/tag/v0.3.1-alpha
[0.3.0-alpha]: https://github.com/ipavlin98/traydex/releases/tag/v0.3.0-alpha
[0.2.0-alpha]: https://github.com/ipavlin98/traydex/releases/tag/v0.2.0-alpha
[0.1.0-alpha]: https://github.com/ipavlin98/traydex/releases/tag/v0.1.0-alpha
