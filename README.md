## Treest

Navigable tree view.

![./screenshot.png](./screenshot.png)

### Getting Started

This project uses `cargo`:
```console
$ cargo install --path .
$ treest --help
...
```

---

## (wip and such)

thing too long that cause terminal line wrap

### subtrees

### settings, config, ..

- [x] enable/disable mouse support
- [x] enable/disable alt screen
- [x] enable/disable pretty (ascii)

- [ ] enable/disable single nested child

- provider-specific
    - [ ] sorting/filtering
    - [ ] fs: chdir to root
    - [ ] fs: .ignore
    - [ ] json: render keys as quoted strings

### `ProviderMut`

mv: edit a fragment
mk: add and edit a fragment -> add
rm: remove a fragment
cp

### better redraw logic than "redraw all"
