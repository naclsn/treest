#ifndef TREEST_SAD
#define TREEST_SAD

#undef _DEFAULT_SOURCE
#define _DEFAULT_SOURCE 1

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

#define EVERY_PRINTERS(__do, __sep)  \
    __do(ascii_printer) __sep        \
    __do(fancy_printer)

#define die(__c) {  \
    perror(__c);    \
    exit(errno);    \
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
} gflags;
extern bool toggle_gflag(char flag);

extern struct Command {
    bool (* f)(void);
    char* h;
} commands_map[128];
extern unsigned char* aliases_map[128];
extern bool run_command(char user);

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

struct Node* node_alloc(struct Node* parent, size_t index, char* path);
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

#endif // TREEST_SAD
