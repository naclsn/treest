## Treest

Navigable tree view.

![./screenshot.png](./screenshot.png)

### Getting Started

This project uses `cargo`:
```console
$ cargo install --path .
$ treest --help
...

$ cargo r -- --help # (without installing)
...
```

### todo:

* doc/help situation (`.config/treest.lua`, `require('defaults')` and `treest --lua-meta`)
* thing too long that cause terminal line wrap (render and prompt)

### fixme:

* search can't find last line?

---

### ps

https://docs.rs/sysinfo/0.30.13/sysinfo/struct.Process.html

### subtrees

### settings, config, ..

- provider-specific
    - [ ] sorting/filtering
    - [ ] fs: chdir to root
    - [ ] fs: .ignore
    - [ ] json: render keys as quoted strings
