# TODO
## misc
* clean/update man page (and README.md)
* die better

---
## fancy
### misc
* scrolling: keep cursor in view
* (probably flag or command) dum watch (like every 1s or who cares) then better at some point
  - IN_ATTRIB
  - IN_CREATE -> dir or link is add to watch
  - IN_DELETE -> dir or link is remove from watch
  - IN_MOVE (
      IN_MOVED_TO   -> equivalent to a create
      IN_MOVED_FROM -> equivalent to a delete
    )
  - IN_DELETE_SELF/IN_MOVE_SELF -> (only on root) exit
  - IN_EXCL_UNLINK

### flags
* (`-b` escape non graphic -- maybe)
* `-s` size
* `-h` human readable size (with -s)
