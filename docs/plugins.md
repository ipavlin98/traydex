# Writing a Traydex plugin

A plugin is your own quick action tile backed by a script: PowerShell (`.ps1`), Python (`.py`, needs `python.exe`
in `PATH`), `.bat`/`.cmd` or any `.exe`. Users drop files on the tile, Traydex runs your script and shows its
progress, result and errors.

## Package

A plugin is a `.tdx` file — a regular ZIP archive. Drag it into the Traydex window to install it: Traydex checks
the archive, unpacks it into `%LOCALAPPDATA%\Traydex\actions\custom\<id>` (a previous version is replaced
completely) and lists the plugin under *Plugins* on the actions page. The archive must not escape its folder and
must not unpack to more than 256 MB. A single new plugin gets a tile right away with its settings open; several bundles at once open the tiles page.

Pack the examples into `target\plugins`:

```powershell
.\examples\plugins\pack.ps1
```

Pack your own folder:

```powershell
.\examples\plugins\pack.ps1 -Folder <your folder> -Out <destination>
```

## Manifest

`action.json` in the root of the archive:

```json
{
  "id": "resize",
  "name": "Resize photos",
  "description": "Shrinks photos to 1920 px",
  "icon": "icon.svg",
  "entry": "main.ps1",
  "accepts": { "extensions": ["jpg", "png"], "mime": ["image/*"], "files": true, "folders": false },
  "settings": [
    { "key": "api_key", "label": "API key", "type": "secret", "required": true },
    { "key": "keep", "label": "Keep the original", "type": "toggle", "default": true },
    { "key": "target", "label": "Output folder", "type": "folder" }
  ],
  "timeout": 600
}
```

| Field | Meaning |
|---|---|
| `id` | Lowercase Latin letters, digits, `-`, `_`, `.` |
| `icon` | An SVG file inside the archive, up to 1 MB. Raster formats are rejected. The icon is drawn in a single theme color like the built-in ones, so only its shape (opaque parts) matters. A 24×24 outline SVG with a 1.75 stroke fits best |
| `accepts` | Without `extensions` and `mime` any file fits; folders only with `"folders": true`. Text from a browser arrives as `.txt`, an image as `.png`. If nothing dropped fits, the tile is dimmed with "Doesn't fit" and refuses the drop; unsuitable files from a mixed drop are not passed to the script |
| `settings` | Types `text`, `secret`, `toggle`, `file`, `folder` become fields, switches and path pickers in the tile settings. A tile with an empty `required` setting is marked "Needs setup" |
| `timeout` | Seconds, 600 by default. When it runs out, the script and all its child processes are terminated and the tile shows an error |

The manifest has one language. To translate messages, read the `language` field of the input.

## Input

The script runs without a console window; the working folder is the plugin folder. Input arrives as one line of
JSON on stdin (file paths are not passed as arguments):

```json
{"protocol": 1, "action": "resize", "files": ["C:\\Photos\\a.jpg"], "settings": {"api_key": "…", "keep": true, "target": ""}, "output": "C:\\…\\output", "language": "en"}
```

`output` is an empty folder in the Traydex storage for results. The same data is in environment variables:
`TRAYDEX_PROTOCOL`, `TRAYDEX_ACTION`, `TRAYDEX_ACTION_DIR`, `TRAYDEX_OUTPUT`, `TRAYDEX_LANGUAGE` and
`TRAYDEX_SETTING_<KEY>` (key in upper case, a toggle is `1` or `0`).

For PowerShell, stdin and stdout are switched to UTF-8; read input with `[Console]::In.ReadToEnd()`. Windows
PowerShell 5.1 reads a `.ps1` without BOM as ANSI, so save scripts with non-Latin text as UTF-8 with BOM.
For Python, `PYTHONUTF8=1` is set.

## Output

Print JSON lines to stdout, one per event; any other output is ignored:

| Line | Effect |
|---|---|
| `{"type": "progress", "value": 0.5}` | Progress from 0 to 1 on the tile and in the title bar |
| `{"type": "clipboard", "text": "…"}` | Puts text into the clipboard when finished |
| `{"type": "add", "path": "result.jpg"}` | Adds a file to the shelf; a relative path is resolved against `output` |
| `{"type": "notify", "text": "…"}` | Final notification text instead of the default one |
| `{"type": "error", "text": "…"}` | Ends the action with this error |

A non-zero exit code without an `error` line is also a failure: the last line of stderr or the exit code is shown.
Stopping the job from the title bar terminates the whole script process tree.

## Examples

- [`resize-images`](../examples/plugins/resize-images) — PowerShell, no dependencies. Shrinks JPEG, PNG, BMP, GIF
  and TIFF to a given long side, respects EXIF rotation, reports progress per file.
- [`telegram-send`](../examples/plugins/telegram-send) — Python 3 standard library only. Sends files through your
  own Telegram bot; photos up to 10 MB as photos, the rest as documents up to 50 MB.
</content>
