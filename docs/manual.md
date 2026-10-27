# Traydex manual

The complete description of what Traydex does and how it works. For a short overview, see the [README](../README.md).

Traydex is a drop shelf for Windows 10/11, written in Rust on top of GPUI. The UI is rendered with DirectX; there
is no WebView inside.

## Installing and running

Every version on the [Releases](https://github.com/ipavlin98/traydex/releases/latest) page has an installer
(`*-setup.exe`) and a portable zip (`*-portable.zip`) for x64, ARM64 and 32-bit x86.

- The installer installs Traydex for the current user into `%LOCALAPPDATA%\Programs\Traydex` without admin rights,
  adds a Start menu shortcut and registers it in the list of installed apps. On uninstall it closes Traydex, removes
  autostart and the Explorer menu item, and asks whether to delete the shelf and the settings too.
- The portable zip is a folder with `Traydex.exe` and `portable.txt`. While `portable.txt` sits next to the exe, the
  shelf, settings and logs live in the `data` folder next to it instead of `%LOCALAPPDATA%\Traydex`, and autostart
  and the Explorer menu item are off by default. Turning autostart off removes only this copy's entry, so a portable
  copy never touches the autostart of an installed one.

Traydex stays in the tray after the panel hides; quit it with a right click on the tray icon → *Quit Traydex*. The
icon can be hidden in Settings (*Tray icon*); then the panel opens with the shortcut, the hot corner or by launching
Traydex again, and tray notifications aren't shown.

### Updates

With *Check for updates automatically* on (the default; off in the `--test-profile` profile), Traydex asks the
GitHub API for the repository's releases a minute after startup and then every 24 hours while it runs. The button
next to the version at the bottom of Settings checks right away and also reports when the version is current or the
check failed; automatic checks only write failures to the log.

- **What fits this copy.** The architecture is the one Traydex was built for (x64, x86 or ARM64), the form is
  portable when `portable.txt` sits next to the exe and the installer otherwise, and the channel follows the running
  version: a pre-release (`0.2.0-alpha`) is offered pre-releases and stable releases, a stable version only stable
  ones. Versions are compared by Semantic Versioning, so `1.0.0` replaces `1.0.0-alpha`, and from then on that copy
  stays on stable releases. Drafts and releases without this copy's file or `SHA256SUMS.txt` are skipped; the newest
  remaining release wins.
- **Offer.** A newer version appears at the bottom of the window: *Version X is available* with *Later* and
  *Update*. If the window is hidden when an automatic check finds it, a tray notification says so too. *Later* hides
  that version from automatic checks until Traydex restarts; a manual check always shows it.
- **Update.** The file and `SHA256SUMS.txt` go to `%TEMP%\Traydex-update` (a job with progress that can be stopped);
  a file whose SHA-256 differs from the published one is refused. The installer then runs with
  `/SILENT /SUPPRESSMSGBOXES /NORESTART /relaunch=1` in the interface language, after Traydex has saved the shelf and
  quit; `/relaunch=1` makes it start Traydex again when it's done. The portable zip is unpacked next to the exe: every
  file except `portable.txt` is replaced (the exe keeps its own name, the previous files stay as `*.update-old`), and
  the new exe starts with `--after-update`, which waits up to 15 seconds for the old copy to quit instead of handing
  over to it. The download folder and the `*.update-old` files are removed on the next start.

## Features

- The panel appears as soon as a system drag (OLE drag-and-drop) starts in any app: files and folders from Explorer,
  text, links and images from browsers, attachments from mail clients. Focus doesn't change.
- Selecting text, moving or resizing windows and other moves with a pressed button don't bring the panel up.
- *Only on shake* (*Off* by default): the panel doesn't appear right away, but when you shake the cursor
  horizontally while dragging. *Low* sensitivity needs three sharp changes of direction of at least 30 px within a
  second, *High* needs two changes of direction of at least 15 px.
- *Hot corner* (off by default): pick a screen corner in Settings, and pushing the cursor into it shows or hides the
  panel. Only outer corners of the desktop, where the cursor actually stops, work; a corner between two monitors
  doesn't. The corner fires again once the cursor has moved 60 px away from it or has been inside the Traydex
  window. Entering the corner with a mouse button held does nothing. If a full-screen window (video, game,
  presentation) is in front on the same monitor, the corner doesn't open the panel. Windows doesn't pass mouse
  movement to a non-elevated Traydex while a window run as administrator is in front, so then Traydex checks the
  cursor position every 30 ms instead.
- A panel that opened by itself hides right away when the drag ends and the cursor isn't over it. If you drop
  something on Traydex, the panel stays open as if you had opened it yourself. A panel opened manually (tray icon,
  shortcut, hotkey or hot corner) or left open after a drop hides when you click anywhere outside it, after the
  button is released, so you can press outside and immediately drag something else into it. The pin in the title
  bar turns auto-hide off in every case.
- By default the window sits in the top-left corner. Drag it by the title bar and resize it by any edge or corner.
  It snaps to screen edges and remembers its position and size between launches. Both can be reset in Settings; the
  default size is a 540 × 540 square.
- Themes: light, dark (pure black) and following the system.
- Languages: English, 简体中文, हिन्दी, Español, Français, Русский. The system language is used by default, English if
  the system language isn't in the list. Switch it in Settings.
- Start with Windows (on by default, off in Settings): Traydex starts hidden in the tray. The entry lives in
  `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`.
- Accepts files and folders, text, links, PNG and DIB images, and virtual files through the standard OLE stream.
- Drag one or several cards back out into apps and Explorer; copy them to the clipboard, open the original.
- Quick actions: while you drag, the panel shows target tiles. Whatever you drop on a tile is moved or copied to a
  folder, zipped, converted to JPEG/PNG or GIF, compressed (MozJPEG or WebP), joined into a PDF, renamed by a rule,
  copied as a data URI, hashed (SHA-256, MD5 and optionally VirusTotal), turned into text (Windows OCR) or a QR
  code, uploaded to Imgur or your own server, opened on your phone via a QR code, searched on the web (text, file
  name or image), or handed to a program or script. The *Drop here* strip above the tiles adds to the shelf; the
  gaps between tiles don't accept drops.
- Preview any card right in the panel: double-click (can be turned off in Settings, then a double-click opens the
  file), `Space` on the card under the cursor, or the eye button. Images (including animated GIF and SVG) fit the
  window; the mouse wheel zooms a raster image up to 16 times around the cursor, dragging moves the enlarged image
  without uncovering its edges, a double-click zooms 3 times at the cursor or back to fit, and the next card starts
  fitted again (SVG and images whose size can't be read don't zoom); text and code are shown with line numbers, the
  first 2 MB for large files; folders and ZIP archives as a grid of tiles with names and sizes; notes and links in
  full and editable right away. Everything else (PDF, Office documents, video, HTML and so on) opens with the same
  Windows preview handler as the Explorer preview pane. If there's no handler, you see a thumbnail or icon, the file
  type and an *Open* button.
- Notes: the plus button before the storage button, the *new note* button on an empty shelf or an empty *Notes*
  tab, or `Ctrl+N` opens an empty note in the preview. Notes are a separate kind with their own icon and *Notes*
  tab. By default (*Keep notes separate*) they aren't shown under *All* and *Text* and aren't included in *Drag
  everything*, copying the whole shelf or clearing it; clearing the *Notes* tab removes only notes. If the tabs
  don't fit a narrow window, scroll their row with the mouse wheel. Text and links on the shelf are edited right in
  the preview (word wrap, selection with the mouse and `Shift`, `Ctrl+Z`/`Ctrl+Y`) and saved automatically; an
  empty note is deleted when closed. The title is a field in the preview header that works like the search box:
  a click puts the caret in it, dragging selects text (and scrolls past the edge), a double-click selects a word,
  `F2` selects the whole title, `Tab` switches between the title and the text; edits are saved immediately. Until
  you change the title, the start of the text is shown in grey and saved as the title. A title you typed stays
  yours even if it matches the start of the text. Titles longer than 96 characters are cut at the last letter
  (without trailing commas, periods or brackets) and get "…" at the end; if a title doesn't fit a tile, the tooltip
  shows it in full. A long title fades at the edges and shows from the start when not focused. Files from disk open
  read-only.
- Mentions in notes: `\` (the key above `Enter`, unshifted in both English and Russian layouts) or `@` at the start of
  a word (not inside an address like `ivan@mail.ru` or a path like `C:\Users`) opens a list under it of every file,
  picture, folder and link on the shelf (notes and texts aren't offered), newest first, then filtered by what you type
  with the same matching as search (typos, keyboard layout). It shows seven rows and scrolls with the wheel; `↑`/`↓`
  or `Alt+K`/`Alt+J` move and keep the chosen row in view, the mouse picks the row it moves over, `Enter`, `Tab` or a
  click inserts, `Esc` closes the list and keeps the text; the same trigger doesn't ask again. A mention is a chip
  with the card's icon and current title (renaming the card renames the chip) that moves and selects as one character;
  `Backspace` right after it turns it back into `\` and opens the list again, a second `Backspace` removes the `\`,
  and `Ctrl+Z` brings the chip back. Hovering it for a moment shows a card: the picture, a file's thumbnail or the
  start of a note, with its size and place. A click opens the card's preview with a back arrow in the header; `Esc` or
  the arrow returns to the note with the caret where it was, the cross goes to the shelf. `Ctrl`+click closes the note
  and shows the card on the shelf, selected. Dragging a chip out works like dragging its card. `Ctrl+V` of files or a
  screenshot and dropping files on an open note put them on the shelf and mentions to them at the caret; text is
  pasted as text. A chip of a card removed from the shelf turns grey and says so; restoring the card from the Recycle
  Bin brings it back. The preview of a card mentioned in notes shows the newest such note at the right of its bottom
  bar (the tooltip lists all of them); with several notes the wheel over it switches to the next or previous one at
  once, with its position next to it, and a click opens the note shown at the mention. In the stored text a mention is
  `@[title](traydex:id)`; the title, search, copying and dragging a note out show it as `@title`, and copying inside
  notes keeps mentions.
- The *Send to Traydex shelf* item in the Explorer context menu for files and folders puts the selection straight
  on the shelf. On by default, can be turned off in Settings. On Windows 11 it's under *Show more options*.
- Watched folders: *Add* in the *Folders* section of Settings picks a folder, and everything at its top level, files
  and subfolders (a subfolder is one card), goes on the shelf at once, newest first; whatever appears in it later
  follows within two seconds, including while Traydex was closed. Hidden files and unfinished downloads
  (`.crdownload`, `.part`, `.partial`, `.tmp`) are skipped. Cards are links, nothing is copied. When a file is deleted
  or moved out of the folder, its card leaves the shelf, pinned or not; a renamed file gets a new card. A card you
  remove from the shelf stays off until its file leaves the folder and comes back. If the folder can't be read (a disconnected drive), its cards stay
  as they are. The cross next to a folder stops watching it and removes its unpinned cards. The list and the names
  already seen in each folder are stored in `shelf.json` under `watched` and aren't affected by *Reset all*.

## Controls

| Action | How |
|---|---|
| Show or hide the panel | `Ctrl+Shift+Space`, the tray icon or the hot corner |
| Hide the panel | `Esc` or the cross in the title bar; `Esc` clears the selection first |
| Move the window | Drag the title bar |
| Resize | Drag an edge or a corner |
| Add the clipboard contents | `Ctrl+V` or the *Ctrl V paste from clipboard* button on the shelf |
| Open the folder with saved content | The folder button in the status bar |
| Copy the selection, or everything | `Ctrl+C` or the button on the *Selected* bar |
| Select cards | A click selects or deselects, `Shift` + click selects a range; switching tabs clears the selection |
| Move between cards | `↑`/`↓` highlight a card as if hovered, and `Space`, `Enter`, `Ctrl+C`, `Delete` act on it; works from the search box too. Moving the mouse brings back normal hover |
| Select the card under the cursor | `X`, same as clicking the card |
| Pin a card to the top | The pin button on the card or `P` (acts on the card under the cursor or on the selected ones). Pinned files, notes, images and links always stay at the start of the shelf with a highlighted pin button; new cards go below them. Clearing the shelf keeps pinned cards; remove one with its cross or `Delete` |
| Move the selected cards at once | Drag any selected card |
| Move everything at once | Drag the *Drag everything* row above the search; it takes everything visible on the current tab, search included |
| Select all | `Ctrl+A` |
| Remove the selected cards | `Delete` or the button on the *Selected* bar |
| Find a card | `/` or a click on the search box at the bottom; `Esc` clears the query, a second `Esc` leaves the search box. The search box is hidden while the shelf is empty and while something is dragged over the window |
| Open a card | `Enter`, the arrow button, or a double-click when *Preview on double-click* is off |
| Preview a card | Double-click (*Preview on double-click*, on by default), or hover and press `Space` or the eye button; without hover, the first selected card |
| Flip through previews | `←`/`→` or `↑`/`↓`, the buttons in the preview header |
| Close the preview | `Space`, `Esc` or the cross; `Esc` also works from a system preview if its handler passes keys to the window. `Enter`, `Ctrl+C` and `Delete` in the preview act on the card shown |
| Drag a card out | Hold the left button and move the mouse |
| Cancel a drag and hide the panel | Right-click during a drag (left button still held, panel open): the drag is cancelled, files stay where they are, the panel hides even when pinned. The click itself never reaches Windows, so no context menu appears. Ordinary right clicks without a drag aren't affected |
| Create a note | The plus button, `Ctrl+N`, or the *Ctrl N new note* button on an empty shelf and an empty *Notes* tab |
| Rename a note or a link | Click the title in the preview or press `F2` and edit; `Enter`, `Esc` or `Tab` go back to the text. An empty title brings back the automatic one |
| Show the shelf as a list or a grid | The grid (or list) button next to the storage button, `Ctrl+G` |
| Keep the panel from hiding | The pin button |
| Theme, transparency and blur, auto-show, hot corner, autostart, context menu, preview on double-click, keep notes separate, delete to the Recycle Bin, watched folders, reset position and size, reset all settings | The settings button |
| Add or remove quick action tiles | The lightning button |
| Run an action | Drop files, an image or text on a tile |
| Add files from Explorer | Right click → *Send to Traydex shelf* |

In grid mode every card has a large preview: a photo fills the tile, a note shows the start of its text, a link its
address, and a file its system thumbnail (a video frame, a document page, an app icon), loaded in the background;
folders and files without a thumbnail get a type icon in the corner. Tiles are square, at least 148 px, and a row
holds as many as fit. When there are enough tiles for at least one full row, they stretch so the row spans the full
width up to the right edge of the buttons; with fewer tiles than a row holds, they keep the smallest size and don't
change when the window is resized. In grid mode the window can't be narrower than two columns: switching to the
grid widens a narrow window as needed. Selection, hover, the copy, preview, open and delete buttons, dragging,
double-click and right click work as in the list; `←`/`→` move between tiles, `↑`/`↓` between rows. The chosen view
is stored in `shelf.json` and reset together with the settings.

Search filters the shelf as you type and highlights matches in titles; for text and links it also looks inside the
content. Case and "ё"/"е" don't matter, and all words of the query must match. A query typed in the wrong keyboard
layout is found too (`ghbdtn` → «привет»). Without exact matches, typos are allowed: one in words of 4–7 letters,
two in longer ones. Tab counters show only what was found.

Files and folders in Traydex are links to the originals. Under the name a card shows the type, the size of a file and
the folder it's in. If an original is moved or deleted, the card shows *Original unavailable*. Text, pasted images and files from browsers and mail are saved in
`%LOCALAPPDATA%\Traydex\content`, one folder or file per card; the cards of such files and folders say *Stored only in
Traydex* instead of a folder. They're deleted together with their card or by *Clear Traydex*, after the change is
written to `shelf.json`; originals from Explorer are never touched. With *Delete to the Recycle Bin* (on by default)
they go to the Windows Recycle Bin without a question: the file itself, under its own name, not its `content`
folder. Restoring it there brings the card back as it was (kind, title, pin) the next time the window is shown or
activated, or at startup if Traydex was closed. Traydex remembers the last 100 cards sent to the Recycle Bin in
`shelf.json` (`recycled`) and checks whether their files are back only at those moments; nothing polls. A drive
without a Recycle Bin (a network share, some USB drives with the portable version) deletes for good. With the
setting off, removing a card whose file is stored only in Traydex (with the cross, `Delete` or the *Selected* bar)
first asks at the bottom of the window whether to delete it for good; `Enter` confirms and `Esc` cancels, and *Clear
Traydex* shows the same warning instead of its usual question. Notes and text are removed without asking. When such a
file is dropped on a tile, the temporary copy is deleted right after the action, and the result (a converted image,
for example) is saved separately and becomes a new card; for the run-program tile the copy stays until Traydex
starts next time. Anything in `content` that no card refers to is deleted on startup. The shelf and the settings
live in `%LOCALAPPDATA%\Traydex\shelf.json`; if the file is corrupted, a copy is kept as `shelf.broken.json`. (The
portable version uses its `data` folder instead of `%LOCALAPPDATA%\Traydex` everywhere in this manual.)

When text or a link is dragged into a browser, an editor or any other app, it arrives as plain text; into Explorer,
the desktop and windows with a file list (such as the *Save* dialog) it arrives as a `.txt` file. Traydex offers the
file only when `explorer.exe` asks for the data or the window under the cursor is a file list of the app that asks;
the initial format list that Windows builds when the drag starts does include the file, otherwise Explorer wouldn't
accept it. Images are offered as a file and in the available image formats, and the target picks the one it
supports. Unsupported formats show an error message.

Window transparency (0–40%) and blurring what's behind the window are set in *Appearance*. Transparency applies to
the whole window, text and tiles included. Blur is the system one (*Light*, Aero); Windows doesn't let apps set its
strength. With 0% transparency blur is off.

The log of work and errors, including caught crashes and a detailed drag trace (debug level; change it with the
`RUST_LOG` variable), goes to `%LOCALAPPDATA%\Traydex\logs`: the current file is `traydex_rCURRENT.log`, it rolls
over at 2 MB, and four previous files are kept. The *Logging* section of Settings has a button that opens the folder.

## Quick actions

The page with the lightning icon in the title bar lists your tiles and has the *Add a tile* buttons. First come
folder (picked in the standard dialog), program, web search and custom server, which can be added as many times as
you like; then ZIP, JPEG, PNG and the rest, one of each. An added tile is highlighted in that list: single tiles get
a check mark (clicking again removes the tile), repeatable ones show how many you've added. Imgur, search and custom
server tiles open their settings right away, with the caret in the field that needs filling; GIF downloads ffmpeg
(~100 MB, the gyan.dev build) into `%LOCALAPPDATA%\Traydex\tools` the first time it's added, unless ffmpeg is in
`PATH`.

Clicking a tile in the list opens its settings: the name, keys and addresses are entered right in Traydex and saved
immediately, and secret keys are masked. Here you also choose whether a folder tile moves or copies, and which
engine a search tile uses: Google (default for the first search tile), Yandex, Bing, DuckDuckGo, YouTube or your own
address where `%s` is replaced by the query. An engine already used by another tile can't be picked. A tile missing
required settings is marked *Needs setup*.

While an action runs, a purple progress ring fills next to the pin in the title bar (an arc spins when the percentage
is unknown); its tooltip lists all running jobs with their stage and percentage. Errors show up as a red icon in the
same place and successes (for example, "Text copied · 245 chars") as a green check: the text is in the tooltip, and a
click dismisses it. On other pages the message appears as a bar above the bottom strips instead; a click on the bar
dismisses it too. Text and images from a browser are saved
to a file before the action. Results (an archive, converted and compressed images, a GIF, a PDF) appear next to the
original and are added to the shelf. The archive path and links to uploaded files are copied to the clipboard. If the
panel is already hidden, the result is shown as a system notification. The search tile hides the panel right away
(unless it's pinned) and reports only errors.

By default the panel has these tiles: copy to the Desktop, ZIP, PNG and Google search. *Reset all settings* at the
end of Settings restores the theme, language, behavior, tiles, window position and size; the shelf keeps its items.
Tiles are stored in `%LOCALAPPDATA%\Traydex\actions.json`; changes to the file are picked up when the Traydex window
becomes active again. If the file is corrupted, a copy is kept as `actions.broken.json`.

| `type` | Fields | What it does |
|---|---|---|
| `move`, `copy` | `folder` | Moves or copies to a folder with the standard Windows file operation and its progress window; name clashes get a renamed copy |
| `zip` | — | Packs everything dropped into one archive next to the originals |
| `unpack` | — | Extracts ZIP, 7z, RAR, TAR and other archives next to them |
| `jpeg`, `png` | — | Converts images; transparency becomes white in JPEG |
| `gif` | — | Makes a GIF from a video with ffmpeg |
| `copy_path` | — | Copies the full paths of dropped files and folders to the clipboard, one per line |
| `imgur` | `client_id` | Uploads an image to Imgur |
| `phone` | — | Shows a QR code with a link to the dropped files on the local network: the phone camera opens the link, images, video, PDF and text are shown in the browser, everything else is downloaded. Several files get a list page; folders aren't served. The link is random and works while the QR screen is open (started downloads finish). The phone must be on the same network; on first use Windows asks for a firewall permission |
| `upload` | `url`, `field`, `header`, `link`, `form` | Sends the file as a `multipart/form-data` form; `link` is a JSON pointer to the link in the response, without it the whole response is the link; `form` holds extra form fields and can be set only in the file |
| `run` | `program` | Runs an `.exe`, `.bat`, `.cmd` or `.ps1` with the file paths as arguments; the working folder is the folder of the first file |
| `data_uri` | — | Copies the file as a `data:<type>;base64,…` string for HTML and CSS (SVG, PNG, fonts and so on, up to 10 MB); several files give one line each |
| `hash` | `algorithm`, `virustotal` | Computes SHA-256 and MD5, copies the chosen one (`sha256` or `md5`) and shows both on a result page with copy buttons; for several files the clipboard gets "hash  name" lines. With a VirusTotal key it checks the file by its SHA-256 only, without uploading it: "no threats 0/70", "threats found 5/70" with a link to the report, or "hasn't seen this file yet". The key is passed to `curl` through stdin and isn't visible in the process list |
| `ocr` | `language`, a language tag (`en-US`, `ru`…), optional | Recognizes text in images and scanned PDFs (up to 100 pages) with the OCR built into Windows and puts it on the clipboard. The language is picked in the tile settings among those installed in Windows; without a choice, the first Windows profile language with recognition installed is used; small images are upscaled, transparent ones get a white background |
| `compress` | `format` | Makes a lighter copy `name-min.jpg` (MozJPEG, quality 80, progressive) or `name-min.webp` (WebP 80, keeps transparency) next to the original. The long side is reduced to 2560 px and EXIF rotation is applied. If the copy isn't smaller than the original, it isn't created |
| `pdf` | — | Joins images and text files (`txt`, `md`, `log`, `csv`, `json`, `xml`, `ini`, `yaml`) into one A4 PDF in name order: an image becomes a page with 1 cm margins, JPEG goes in without re-encoding; text becomes pages with embedded Arial that can be selected and searched. One file gives `name.pdf`, several give `Document.pdf` |
| `rename` | — | Opens a page to pick a rule for dropped files and folders from disk: *Date and number* (`2026-10-11_01.jpg`, the date taken from a name like `IMG_20261011` or from the modification time), *Name and number*, *Transliteration* and *For the web* (Latin, lowercase, hyphens). A preview of the new names is at the bottom. Taken names get " (2)", cards on the shelf follow the new paths; on error, files already renamed are renamed back |
| `qr` | — | Shows the dropped text or link (also a `.txt` file or a `.url` shortcut) as a large QR code; the image can be copied or put on the shelf |
| `search` | `url` | Opens a web search in the browser. The query is the dropped text or link, or the names of files and folders (`_` and `.` become spaces). An image is searched by image: in Yandex Images or Bing for tiles of those engines, otherwise in Google Lens through a temporary page `%TEMP%\traydex-lens.html` that the browser submits to Lens itself |
| `ytdlp` | — | Downloads a video from a link with yt-dlp (needs yt-dlp, ffmpeg and Deno, about 250 MB, downloaded on first use) |

Every tile has a `name`. Uploads go through the system `curl.exe`, which ships with Windows 10 1803 and newer. An
Imgur Client-ID is issued after registering an app at api.imgur.com; the link is in the tile settings. A free
VirusTotal key is available at virustotal.com (the link is in the *File hash* tile settings); it allows 4 requests a
minute, and files over the limit are marked "request limit reached".

## Plugins

A plugin is your own tile backed by a script. How to write, pack and install one is described in
[plugins.md](plugins.md).

## Explorer context menu

The item lives in `HKCU\Software\Classes\*\shell\Traydex` and `HKCU\Software\Classes\Directory\shell\Traydex`, so no
admin rights are needed. When several files are selected, Explorer launches Traydex for each one; those copies hand
their paths to the running instance and exit at once. If Traydex wasn't running, it starts and opens the shelf.

## Building

You need Rust stable for `x86_64-pc-windows-msvc` and Visual Studio Build Tools 2022 with C++ and the Windows SDK.
From PowerShell in the project root:

```powershell
.\build.ps1 -Run
```

The script sets up MSVC, closes a running Traydex, builds the debug version into `target/debug/traydex.exe` and
starts it hidden in the tray as on autostart: no window shows up, open it with the shortcut or the tray icon.
Without `-Run` it only builds. `.\build.ps1 -Release` makes a release build in `dist/Traydex.exe` together with this
README and the list of licenses. UI resources and the icon are embedded in the file, and the CRT is linked
statically. Dependency versions are pinned in `Cargo.lock`. More on building and testing is in
[CONTRIBUTING.md](../CONTRIBUTING.md).

### Patches in `vendor/gpui`

GPUI 0.2.2 lives in `vendor/gpui` with nine patches, wired in through `[patch.crates-io]` in `Cargo.toml`.

1. **Pixel snapping.** All layout sizes are rounded to whole physical pixels, border widths to a whole number of
   pixels and at least one, and content offsets (scrolling, for example) to whole pixels. Identical elements are
   drawn identically and symmetrically at any Windows scale. The patch is `snap` in `vendor/gpui/src/taffy.rs`, and
   `snap_border` and the offset snapping in `with_element_offset` in `vendor/gpui/src/window.rs`. Anchored popups no
   longer round their offset to whole logical pixels first (`prepaint` in `vendor/gpui/src/elements/anchored.rs`),
   so at 125% or 150% a popup lands on the same physical pixel as the element it opens from; the regression test is
   `anchored_element_lands_on_the_device_pixel_it_was_given` in `src/gpui_tests.rs`.
2. **Mouse leave.** When the cursor leaves the window (`WM_MOUSELEAVE`, including when the window is hidden or
   covered), the Windows backend sends `PlatformInput::MouseExited`, as the macOS, X11 and Wayland backends do. The
   core then treats the cursor as outside the window: the hit test and tooltip checks return nothing until the next
   mouse event. Without it, hover and tooltips stayed after a quick mouse exit or after the window was hidden or
   covered. The patch is `handle_mouse_leave_msg` in `vendor/gpui/src/platform/windows/events.rs`, the
   `pointer_inside` field and `pointer_hit_test` in `vendor/gpui/src/window.rs`, and the tooltip checks in
   `vendor/gpui/src/elements/div.rs`.
3. **Grab cursors.** On Windows, the `OpenHand` and `ClosedHand` cursors are shown as the system *Move* cursor;
   without the patch they were a plain arrow, since Windows has no hand cursor of its own. The patch is
   `load_cursor` in `vendor/gpui/src/platform/windows/util.rs`.
4. **Child windows on top.** The DirectComposition layer GPUI draws into is attached below child windows
   (`CreateTargetForHwnd(hwnd, false)` in `vendor/gpui/src/platform/windows/directx_renderer.rs`); otherwise the
   window of a system preview handler ended up under the panel content and wasn't visible.
5. **Transparency and light blur.** `Window::set_opacity` sets the opacity of a DirectComposition effect group
   (`IDCompositionEffectGroup`) over the GPUI layer and reapplies it after a device loss; the
   `WindowBackgroundAppearance::LightBlurred` background turns on the system `ACCENT_ENABLE_BLURBEHIND` blur on
   Windows, while the existing `Blurred` stays Acrylic. The patch is `set_opacity` in
   `vendor/gpui/src/platform/windows/directx_renderer.rs`, `vendor/gpui/src/platform/windows/window.rs`,
   `vendor/gpui/src/platform.rs` and `vendor/gpui/src/window.rs`.
6. **Tooltips.** A tooltip hides as soon as the cursor leaves its element, including onto the tooltip itself.
   Previously visibility was checked against the rectangle remembered when the tooltip appeared, and only when the
   window was redrawn, while mouse moves redraw the window only when some element changes its hover look. So the
   tooltip stayed until something else redrew and didn't notice that its element was covered or moved. Now every
   element remembers its hit area each frame (`tooltip_source` in `InteractiveElementState`), the tooltip checks it
   against the hit test of the frame being drawn, and a change of the element under the cursor redraws the window
   while a tooltip is visible. The tooltip also appears under the cursor (22 px below its tip, aligned to the left)
   instead of right next to it at the bottom right, where the cursor covered it. The patch is
   `vendor/gpui/src/elements/div.rs`, and `draw_roots`, `prepaint_tooltip` and `dispatch_mouse_event` in
   `vendor/gpui/src/window.rs`.
7. **Line breaking.** Lines wrap by the Unicode UAX #14 algorithm (the `unicode-linebreak` crate) instead of the old
   rule "break before any character not on a short list of letters", which could break a line before a closing
   bracket, split Hindi words and start a line with a Chinese comma. On top of UAX #14 there are typographic rules:
   one-letter words (and two-letter Cyrillic ones) and numbers stay with the next word, and a dash doesn't start a
   line. The patch is `vendor/gpui/src/text_system/line_breaks.rs`, `compute_wrap_boundaries` in `line_layout.rs`
   and `wrap_line` in `line_wrapper.rs`; the regression test is `text_wraps_at_grammatical_line_breaks` in
   `src/gpui_tests.rs`.
8. **Text centering.** A line of text is placed so that its capital letters sit in the middle of the line height
   (like `TextLineBounds="TrimToCapHeight"` in WinUI or `text-box-trim` in CSS). Upstream GPUI centered the whole
   ascent-to-descent box, and since Segoe UI has a much taller ascent than descent, letters sat about a pixel low in
   buttons, key caps and fields. The patch is `baseline_offset` in `vendor/gpui/src/text_system.rs`, which
   `paint_line` in `vendor/gpui/src/text_system/line.rs` now uses; the regression test is
   `baseline_offset_leaves_equal_room_above_capitals_and_below_baseline` in `src/gpui_tests.rs`. On Windows glyphs
   have no vertical subpixel variants, and upstream dropped the fraction of the baseline, lifting text by up to a
   device pixel; `glyph_placement` in `vendor/gpui/src/window.rs` puts the baseline on the nearest pixel row
   instead, so capitals stay within half a pixel of the middle. Its test is
   `glyph_baseline_goes_to_the_nearest_pixel_row`.
9. **Icon bitmaps.** An SVG icon is rasterized at twice its size in device pixels, rounded to the nearest texel.
   Upstream rounded up, and since a whole-pixel layout size comes back from logical pixels with float noise
   (18.000002), the bitmap got an extra texel: the icon was stretched by half a pixel and drawn off its center, so the
   rotating spinner wobbled around its axis. The patch is `svg_raster_size` in `vendor/gpui/src/window.rs`; the
   regression test is `svg_bitmap_matches_whole_device_pixels_despite_float_noise` in `src/gpui_tests.rs`.

### Patch in `vendor/unrar`

The `unrar` and `unrar_sys` crates live in `vendor/` with one change: UnRAR functions and callbacks are declared
`extern "system"` instead of `extern "C"`. On x64 and ARM64 the two are the same, but on 32-bit Windows the library
exports them as `stdcall`, and without the change the x86 build doesn't link.

### Releases

Releases are built by `.github/workflows/release.yml`: `Traydex.exe` for `x86_64`, `i686` and `aarch64` (the latter on
a GitHub ARM runner), then Inno Setup installers from `packaging/traydex.iss`, portable zips with
`packaging/portable.txt`, and `SHA256SUMS.txt`. The updater relies on these asset names
(`Traydex-<version>-<arch>-setup.exe`, `-portable.zip`, `SHA256SUMS.txt`), on the `v<version>` tag and on the
installer's `/relaunch=1` parameter; change them together with `src/update.rs`. CI (`ci.yml`) checks formatting, clippy and tests on x64 and runs the
tests in the x86 and ARM64 builds.

GPUI is licensed under Apache-2.0. Dependency licenses sit next to their sources; the list for a shipped build is in
`THIRD-PARTY-NOTICES.txt`.
</content>
