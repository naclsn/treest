## Treest

Stateful tree view of a root directory.

![./screenshot.png](./screenshot.png)

### Getting Started

This project uses `cargo`:
```console
$ cargo install --path .
$ treest --help
Visually explore a file tree.

Usage: treest [OPTIONS] [PATH]

Arguments:
  [PATH]  path to open at, defaults to current directory

Options:
  -x, --clearstate           do not load any existing state for this path
  -u, --userconf <USERCONF>  use specified config instead of any existing default ($HOME/.config/treest)
      --clean                do not use any config
  -h, --help                 Print help
  -V, --version              Print version
```
