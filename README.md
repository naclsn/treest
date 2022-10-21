## Treest

Stateful tree view of a root directory.

![./screenshot.png](./screenshot.png)

> Example of `--printer=fancy -FCj` with [trapd00r/LS_COLORS](https://github.com/trapd00r/LS_COLORS/).

### Getting Started

With `PREFIX` or `DESTDIR`;

```console
$ PREFIX=~/.local make install
$ treest --help
Usage: treest [--printer=NAME] [--LONGOPTIONS] [-FLAGS] [[--] ROOT]
```

> Note that the man page is probably more helpful.

Somewhat sane alias:
```bash
alias treest='treest --printer=fancy --rcfile='"'$HOME/.treestrc'"' -CdFIjwX'
```

---

### TODO

- cd to where cursor is (so shell commands are relative to cursor)
- prompt replaces (last 4 from `history(3)`):
  - {} is absolute path of cursor
  - {~} is root
  - {h} ("head") Remove a trailing file name component, leaving only the head.
  - {t} ("tail") Remove all leading file name components, leaving the tail.
  - {r} ("remove"?) Remove a trailing suffix of the form .xxx, leaving the basename.
  - {e} ("extension") Remove all but the trailing suffix.
- mapping every user command to `\\` can be boring... (or at least does not enable realy customising)
- `^W` window commands (would be cool tho)
  - for this, need proper mini curses of sort... uh
- state when invocked twice in a same directory
