# TODO
## misc
* clean/update man page (and README.md)
* seems it can fail drawing when last child is link to dir:
```
|   |-- some/Makefile
|   `-- some/sel
|   |   |-- > /home/sel/Desktop
|   |   |-- /home/sel/Documents
|   |   |-- /home/sel/Downloads
```

## option
* `--ignore=PATTERN`

## gflags
* (`-I` toggle ignore? or command on `^H`?)

---
## fancy
### misc
* putstr -> buffering
* scrolling (^E/^Y, ^D/^U, ^F/^B) and window heigh detection (and signal?)

### flags
* (`-b` escape non graphic -- maybe)
* `-s` size
* `-h` human readable size (with -s)
