# TODO
## misc
* malloc -> may_malloc (or with MAYP/MAYN)
* putstr -> buffering
* const when should
* recursive reload
* clean/update man page (and README.md)
* pipe-command shows no output?
* `'?'` and a static help for each command

## commands
* moving to root without folding
* reloading (not refreshing, proper reload) of dir at cursor
* (and why not reloading of root without moving cursor)

## gflags
* `-I` ignore=PATTERN
* `-S` size sort
* `-X` extension sort
* `-c` ctime sort
* `-r` reverse sort
* `-t` mtime sort
* `-u` atime sort

---
## fancy
### misc
* scrolling (^E/^Y, ^N/^P, ^D/^U, ^F/^B) and window heigh detection (and signal?)
* see https://stackoverflow.com/questions/51909557/mouse-events-in-terminal-emulator

### flags
* (`-b` escape non graphic -- maybe)
* `-s` size
* `-h` human readable size (with -s)
