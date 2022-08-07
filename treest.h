#ifndef TREEST_SAD
#define TREEST_SAD

#undef _DEFAULT_SOURCE
#define _DEFAULT_SOURCE 1

#ifdef TRACE_ALLOCS
#include <mcheck.h>
#endif

#include <dirent.h>
#include <errno.h>
#include <libgen.h>
#include <locale.h>
#include <stdbool.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/stat.h>
#include <termios.h>
#include <unistd.h>

#define EVERY_PRINTERS(__do, __sep)  \
    __do(ascii_printer) __sep        \
    __do(fancy_printer)

#define die(__c) {                    \
    char oups[128];                   \
    snprintf(oups, 128, "%s:%d(%s)",  \
        __FILE__, __LINE__, __c);     \
    perror(oups);                     \
    exit(errno);                      \
}

#define may_malloc(__r, __s) {  \
    __r = malloc(__s);          \
    if (!__r) die("malloc");    \
}
#define may_realloc(__rp, __s) {  \
    __rp = realloc(__rp, __s);    \
    if (!__rp) die("realloc");    \
}
#define may_strdup(__r, __c) {  \
    __r = strdup(__c);          \
    if (!__r) die("strdup");    \
}

#ifndef PATH_MAX
#define _MAX_PATH 4096
#elif PATH_MAX < 4097
#define _MAX_PATH PATH_MAX
#else
#define _MAX_PATH 4096
#endif

#ifdef _UNUSED
#elif defined(__GNUC__)
#define _UNUSED(x) UNUSED_ ## x __attribute__((__unused__))
#elif defined(__LCLINT__)
#define _UNUSED(x) /*@unused@*/ UNUSED_ ## x
#else
#define _UNUSED(x) UNUSED_ ## x
#endif

extern char* prog;
extern char cwd[_MAX_PATH];
extern bool is_tty;
extern bool is_raw;

extern struct GFlags {
    bool almost_all;
    bool ignore_backups;
    enum Sort {
        Sort_NAME=0,
        Sort_SIZE=1,
        Sort_EXTENSION=2,
        Sort_ATIME=4,
        Sort_MTIME=6,
        Sort_CTIME=8,
        Sort_REVERSE=16,
    } sort_order;
} gflags;
extern bool toggle_gflag(char flag);

extern struct Command {
    bool (* f)(void);
    char* h;
} command_map[128];
extern char* register_map[128];
extern bool run_command(char user);
extern bool run_commands(char* user);

extern struct Node {
    char* path;
    char* name;
    enum Type {
        Type_UNKNOWN = 0,
        Type_FIFO    = S_IFIFO,  // named pipe
        Type_CHR     = S_IFCHR,  // character device
        Type_DIR     = S_IFDIR,  // directory
        Type_BLK     = S_IFBLK,  // block device
        Type_REG     = S_IFREG,  // regular file
        Type_LNK     = S_IFLNK,  // symbolic link
        Type_SOCK    = S_IFSOCK, // socket
        Type_EXEC    = 255,      // regular file with exec flag
    } type;
    union {
        struct Dir {
            bool unfolded;
            struct Node** children;
        } dir;
        struct Link {
            char* readpath;
            struct Node* to;
            struct Node* tail;
        } link;
    } as;
    struct Node* parent;
    size_t index;
    size_t count;
} root, * cursor;

extern struct Printer {
    char* name;
    void (* init)(void);
    void (* del)(void);
    bool (* toggle)(char flag);
    bool (* command)(const char* c);
    void (* begin)(void);
    void (* end)(void);
    void (* node)(struct Node* node);
    void (* enter)(struct Node* node);
    void (* leave)(struct Node* node);
}
#define DO(it) it
#define SEP ,
EVERY_PRINTERS(DO, SEP)
#undef DO
#undef SEP
, * selected_printer;

struct Node* node_alloc(struct Node* parent, char* path);
void node_free(struct Node* node);
void dir_free(struct Node* node);
void lnk_free(struct Node* node);
void node_print(struct Node* node, struct Printer* pr);
void dir_print(struct Node* node, struct Printer* pr);
void lnk_print(struct Node* node, struct Printer* pr);
void lnk_resolve(struct Node* node);
void dir_unfold(struct Node* node);
void dir_fold(struct Node* node);
void dir_reload(struct Node* node);
void term_restore(void);
void term_raw_mode(void);
//bool path_ignore(char* path);
int node_compare(struct Node* node, struct Node* mate, enum Sort order);

#endif // TREEST_SAD
