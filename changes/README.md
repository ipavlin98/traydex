# Unreleased changes

A user-visible change adds one file here instead of editing `CHANGELOG.md` or the version in `Cargo.toml`. Every
branch writes its own file, so any number of branches merge without conflicts.

- Name: the branch name without its prefix, for example `recycle-bin.md` for `feat/recycle-bin`.
- First line: `minor` for a new page, tile, setting, subsystem or notable workflow; `patch` for a fix, a small
  feature or a refinement of existing behavior.
- Then the changelog bullets, written for users.

```markdown
minor
- Delete to the Recycle Bin (Settings, on by default): files and notes stored only in Traydex go to the Windows
  Recycle Bin when their cards are removed. Restoring a file there brings its card back.
```

Internal changes (refactoring, tests, CI, docs) need no file.

A release folds every file into a new `CHANGELOG.md` section, raises the version (minor if any file says `minor`,
otherwise patch) and deletes the files; see *Releasing* in [CONTRIBUTING.md](../CONTRIBUTING.md).
