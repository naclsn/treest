# TODO
## misc
* clean/update man page (and README.md)
* seems it can fail drawing when last child is link to dir (to fix in both `_enter`):
```
|   |-- some/Makefile
|   `-- some/sel
|   |   |-- > /home/sel/Desktop
|   |   |-- /home/sel/Documents
|   |   |-- /home/sel/Downloads
```

---
## fancy
### misc
* putstr -> buffering
* scrolling (^E/^Y, ^D/^U, ^F/^B) and window heigh detection (and signal?)

### flags
* (`-b` escape non graphic -- maybe)
* `-s` size
* `-h` human readable size (with -s)
