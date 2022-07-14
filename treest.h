#ifndef TREEST_VERSION
#define TREEST_VERSION "0.0.1"

#include <dirent.h>
#include <errno.h>
#include <libgen.h>
#include <stdbool.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/stat.h>
#include <termios.h>
#include <unistd.h>

#define EVERY_PRINTERS(__do, __sep) \
    __do(ascii_printer, "ascii") __sep \
    __do(fancy_printer, "fancy")

#ifndef PATH_MAX
#define _MAX_PATH 4096
#elif PATH_MAX < 4097
#define _MAX_PATH PATH_MAX
#else
#define _MAX_PATH 4096
#endif

char* prog;
char cwd[_MAX_PATH];
bool is_tty;
struct {} gflags;
void toggle_gflag(char flag);

struct Node {
    char* path;
    char* name;
    enum Type {
        Type_UNKNOWN = DT_UNKNOWN, // The type is unknown.
        Type_FIFO    = DT_FIFO,    // A named pipe, or FIFO. See FIFO Special Files.
        Type_CHR     = DT_CHR,     // A character device.
        Type_DIR     = DT_DIR,     // A directory.
        Type_BLK     = DT_BLK,     // A block device.
        Type_REG     = DT_REG,     // A regular file.
        Type_LNK     = DT_LNK,     // A symbolic link.
        Type_SOCK    = DT_SOCK,    // A local-domain socket.
        //Type_WHT     = DT_WHT,     // An unionfs "whiteout".
        Type_EXEC    = 255,
        Type_COUNT   = 9, //10,
    } type;
    union {
        struct Dir {
            bool unfolded;
            struct Node** children;
            int children_count;
        } dir;
        struct Link {
            struct Node* to;
        } link;
    } as;
    struct Node* parent;
} root, * cursor;

#define DO(ident, name) ident
#define SEP ,
struct Printer {
    void (* toggle)(struct Printer* self, char flag);
    void (* begin)(struct Printer* self);
    void (* end)(struct Printer* self);
    void (* node)(struct Printer* self, struct Node* node, size_t index, size_t count);
    void (* enter)(struct Printer* self, struct Node* node, size_t index, size_t count);
    void (* leave)(struct Printer* self, struct Node* node, size_t index, size_t count);
} EVERY_PRINTERS(DO, SEP), * selected_printer;
#undef DO
#undef SEP

void def_free(struct Node* node);
void dir_free(struct Node* node);
void lnk_free(struct Node* node);
void def_print(struct Node* node, struct Printer* pr, size_t index, size_t count);
void dir_print(struct Node* node, struct Printer* pr, size_t index, size_t count);
void lnk_print(struct Node* node, struct Printer* pr, size_t index, size_t count);
void dir_unfold(struct Node* node);
void dir_fold(struct Node* node);

#endif // TREEST_VERSION
