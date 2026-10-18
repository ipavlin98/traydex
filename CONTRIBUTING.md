# Contributing to Traydex

Bug reports, ideas, translations and code are welcome.

## Before you start

- **Small fix?** Just open a pull request.
- **New feature or a bigger change?** Open an issue first and describe the problem it solves. Traydex tries to stay
  small and obvious, so it's better to agree on the idea before you spend time on code.

## Setting up

You need Windows 10/11 x64 and:

- [Rust](https://rustup.rs/) stable (`x86_64-pc-windows-msvc`)
- [Visual Studio Build Tools 2022](https://visualstudio.microsoft.com/downloads/#build-tools-for-visual-studio-2022)
  with *Desktop development with C++* (MSVC and Windows SDK)

`build.ps1` finds MSVC and cargo for you, so run everything from PowerShell in the project root:

| Command | What it does |
|---|---|
| `.\build.ps1` | Debug build into `target\debug\traydex.exe` |
| `.\build.ps1 -Run` | Closes running Traydex, builds and starts the debug build hidden in the tray |
| `.\build.ps1 -Lint` | `cargo fmt` and `cargo clippy -D warnings` — must pass |
| `.\build.ps1 -Test [filter]` | Unit, integration and headless UI tests — must pass |
| `.\build.ps1 -Release` | Release build into `dist\` with the license list |

> [!WARNING]
> `.\build.ps1 -Run` closes **every** running Traydex, including the one you use every day.

### End-to-end tests

`.\build.ps1 -E2E run` starts a test copy of Traydex on your real desktop and drives it with the mouse and keyboard.
While it runs, **your mouse and keyboard are blocked** and the clipboard is overwritten. Stop it any time with
`Ctrl+Alt+Shift+Q`. You don't need it for ordinary changes; CI runs only lint and tests.

## Code

The architecture and code rules live in [CLAUDE.md](CLAUDE.md) (it doubles as the guide for AI coding assistants).
The short version:

- Layers go one way: `view → shelf → domain (actions, store, …) → system ← platform`.
  Win32/COM code lives only in `platform`, `os.rs` and `main.rs`.
- No `unwrap`, `expect` or `panic!` outside tests; errors go to the user through `i18n` strings.
- Every UI string exists in all six languages in `src/i18n` (a test checks it).
- Visual sizes come from `src/design.rs`, colors from `Theme`.
- Write a test when it can catch a real bug; a bug fix starts with a failing test.

GPUI is vendored in `vendor/gpui` with a few patches; they are described in the
[manual](docs/manual.md#patches-in-vendorgpui). A change there needs a regression test in `src/gpui_tests.rs`.

## Pull requests

- One topic per pull request; keep unrelated refactoring out.
- `-Lint` and `-Test` pass locally; CI checks the same.
- Visible change → add a screenshot or a short video.
- Describe *what* and *why*. Commit messages can be in English or Russian.

## Translations

Strings live in `src/i18n/<language>.rs`. To fix a translation, edit the file and open a pull request. To add a
language, copy `en.rs`, translate it and register it next to the others.

## Releasing (maintainers)

1. Bump `version` in `Cargo.toml` and add a section to [CHANGELOG.md](CHANGELOG.md).
2. Commit to `main`, then either push a tag with the same version (`git tag v0.2.0 && git push origin v0.2.0`)
   or open *Actions → Release → Run workflow*.
3. The [Release workflow](.github/workflows/release.yml) builds Traydex for x64, x86 and ARM64, packs an installer
   ([`packaging/traydex.iss`](packaging/traydex.iss), Inno Setup) and a portable zip for each, writes
   `SHA256SUMS.txt`, creates the `v<version>` tag and publishes a GitHub release. Versions with a suffix
   (`0.2.0-beta`) become pre-releases.

Every change has to build on all three architectures; CI runs the tests on x86 and ARM64 too. Watch out for
pointer-sized Win32 values (`SetWindowLongPtrW` takes `i32` on x86) and packed structs such as `NOTIFYICONDATAW`
(assign whole fields, don't take references to them).
</content>
