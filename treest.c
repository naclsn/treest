#include "./treest.h"
#include "./commands.h"

#define TREEST_UPDATE() {                     \
        selected_printer->begin();            \
        node_print(&root, selected_printer);  \
        selected_printer->end();              \
}

char oups[_MAX_PATH+14];
char* prog;
char cwd[_MAX_PATH];
bool is_tty;
bool is_raw;
struct GFlags gflags;
struct Node root, * cursor;
struct Printer* selected_printer;

static int LOOPBACK_FILENO[2];
#define LB_READ 0
#define LB_WRITE 1
static int NOTIFY_FILENO;
static fd_set user_fds;

struct Node* node_alloc(struct Node* parent, char* path) {
    struct Node fill = {
        .path=path,
        .name=strrchr(path, '/')+1,
        .parent=parent,
    };

    if (lstat(path, &fill.stat) < 0) return NULL;
    fill.type = S_IFMT & fill.stat.st_mode;

    struct Node* niw; may_malloc(niw, sizeof(struct Node));
    memcpy(niw, &fill, sizeof(struct Node));

    switch (fill.type) {
        case Type_REG:
            if (fill.stat.st_mode & S_IXUSR)
                niw->type = Type_EXEC;
            break;

        case Type_LNK:
            // TODO: handle looping symlinks as broken
            lnk_resolve(niw);
            break;

        case Type_DIR:
            if (gflags.watch) {
                uint32_t m = IN_ATTRIB | IN_CREATE | IN_DELETE | IN_MOVE;
                // YYY: should probably
                //if (0 == strcmp(root.path, fill.path)) m|= IN_DELETE_SELF | IN_MOVE_SELF;
                int wd = inotify_add_watch(NOTIFY_FILENO, path, m);
                if (wd < 0) die(fill.path);
                // TODO: add assoc (wd, niw) to the list
            }
            break;

        default: ;
    }

    return niw;
}

void node_free(struct Node* node) {
    switch (node->type) {
        case Type_DIR: dir_free(node); break;
        case Type_LNK: lnk_free(node); break;
        default: ;
    }
    free(node->path);
    node->path = NULL;
    node->name = NULL;
    if (node == cursor)
        cursor = node->parent
            ? node->parent
            : &root;
}

void dir_free(struct Node* node) {
    for (size_t k = 0; k < node->count; k++) {
        node_free(node->as.dir.children[k]);
        free(node->as.dir.children[k]);
        node->as.dir.children[k] = NULL;
    }
    node->count = 0;
    free(node->as.dir.children);
    node->as.dir.children = NULL;
    node->as.dir.unfolded = false;
}

void lnk_free(struct Node* node) {
    free(node->as.link.readpath);
    if (node->as.link.to) node_free(node->as.link.to);
    free(node->as.link.to);
    node->as.link.readpath = NULL;
    node->as.link.to = NULL;
    node->as.link.tail = NULL;
}

void node_print(struct Node* node, struct Printer* pr) {
    pr->node(node);
    switch (node->type) {
        case Type_DIR: dir_print(node, pr); break;
        case Type_LNK: lnk_print(node, pr); break;
        default: ;
    }
}

void dir_print(struct Node* node, struct Printer* pr) {
    struct Dir dir = node->as.dir;
    if (dir.unfolded) {
        pr->enter(node);
        for (size_t k = 0; k < node->count; k++)
            node_print(dir.children[k], pr);
        pr->leave(node);
    }
}

void lnk_print(struct Node* node, struct Printer* pr) {
    if (node->as.link.tail && Type_DIR == node->as.link.tail->type)
        dir_print(node->as.link.tail, pr);
}

void lnk_resolve(struct Node* node) {
    char readpath[_MAX_PATH];
    ssize_t len = readlink(node->path, readpath, _MAX_PATH-1);
    if (len < 0) {
        may_strdup(node->as.link.readpath, strerror(errno));
        node->as.link.to = node->as.link.tail = NULL;
        return;
    }
    readpath[len] = '\0';

    char* savedpath; may_strdup(savedpath, readpath);

    char fullpath[_MAX_PATH];
    char* paste;
    char* copy;
    if ('/' == readpath[0]) {
        paste = copy = strcpy(fullpath, readpath)+1;
    } else {
        char* basedir = node->path;
        size_t lendir = strlen(basedir);
        while ('/' != basedir[lendir-1]) lendir--; // YYY: strrchr
        paste = memcpy(fullpath, basedir, lendir);
        paste+= lendir;
        copy = readpath;
    }

    if ('/' != paste[-1]) *paste++ = '/';
    do {
        if ('.' == *copy) {
            if ('.' == *(copy+1)) {
                paste--;
                if (fullpath == paste) {
                    node->as.link.to = node->as.link.tail = NULL;
                    return;
                }
                while ('/' != *--paste); // YYY: strrchr
                paste++;
            } else if ('/' == *(copy+1)) copy++;
            else *paste++ = *copy;
        } else if ('/' == *copy) {
            *paste++ = '/';
            while ('/' == *(copy+1)) copy++; // YYY: strchr
        } else *paste++ = *copy;
    } while (*copy++);
    paste--;
    while ('/' == *(paste-1)) paste--; // YYY: strrchr
    *paste = '\0';

    char* path; may_strdup(path, fullpath);

    struct Node* niw = node_alloc(node->parent, path);
    node->as.link.readpath = savedpath;
    node->as.link.to = niw;
    node->as.link.tail = niw && Type_LNK == niw->type
        ? niw->as.link.tail
        : niw;
}

void dir_unfold(struct Node* node) {
    struct Node* parent = node;
    if (Type_LNK == node->type) node = node->as.link.tail;
    if (!node || Type_DIR != node->type) return;

    node->as.dir.unfolded = true;
    if (node->as.dir.children) return;

    size_t parent_path_len = strlen(node->path);

    size_t cap = 16;
    may_malloc(node->as.dir.children, cap * sizeof(struct Node*));

    DIR *dir = opendir(node->path);
    if (dir) {
        struct dirent *ent;
        while ((ent = readdir(dir))) {
            if ('.' == ent->d_name[0] && (
                '\0' == ent->d_name[1]
                || ('.' == ent->d_name[1] && '\0' == ent->d_name[2])
                || !gflags.almost_all
            )) continue;
            size_t nlen = strlen(ent->d_name);
            if (gflags.ignore_backups && '~' == ent->d_name[nlen-1]) continue;

            size_t path_len = parent_path_len+2 + nlen;
            char* path; may_malloc(path, path_len);
            strcpy(path, node->path);

            char* name = path + parent_path_len;
            if ('/' != name[-1]) *name++ = '/';
            strcpy(name, ent->d_name);

            if (cap < node->count) {
                cap*= 2;
                may_realloc(node->as.dir.children, cap * sizeof(struct Node*));
            }

            struct Node* niw = node_alloc(parent, path);

            if (gflags.ignore) {
                bool ignore = selected_printer->filter
                    ? selected_printer->filter(niw)
                    : node_ignore(niw);
                if (ignore) {
                    free(niw);
                    continue;
                }
            }

            if (niw) {
                size_t k = 0;
                for (k = node->count; 0 < k; k--) {
                    int cmp = node_compare(node->as.dir.children[k-1], niw, gflags.sort_order);
                    if (!cmp) cmp = node_compare(node->as.dir.children[k-1], niw, Sort_NAME);
                    if (cmp < 0) break;
                    node->as.dir.children[k] = node->as.dir.children[k-1];
                }
                node->as.dir.children[k] = niw;
                node->count++;
            }
        }
        closedir(dir);
    }

    for (size_t k = 0; k < node->count; k++) {
        struct Node* child = node->as.dir.children[k];
        do {
            child->index = k;
        } while (Type_LNK == child->type && (child = child->as.link.to));
    }

    if (0 == (parent->count = node->count)) {
        free(node->as.dir.children);
        node->as.dir.children = NULL;
    } else may_realloc(node->as.dir.children, node->count * sizeof(struct Node*));
}

void dir_fold(struct Node* node) {
    if (Type_LNK == node->type) node = node->as.link.tail;
    if (!node || Type_DIR != node->type) return;

    node->as.dir.unfolded = false;
}

static void _recurse_dir_reload(struct Node* old, struct Node* niw) {
    if (Type_LNK == old->type) old = old->as.link.tail;
    if (!old || Type_DIR != old->type) return;

    if (Type_LNK == niw->type) niw = niw->as.link.tail;
    if (!niw || Type_DIR != niw->type) return;

    bool found_cursor = false;
    bool moved_cursor = false;

    // YYY: search could be better (maybe) by assuming little change in order
    //   - starting from max(old->index, niw->count-1)
    //   - if niw->count < old->count, go backward; else forward
    //   - reorder the conditions inside to have less strcmp avg case
    // from nlogn (always) to n (best) and n^2 (worst -- how often?)
    //
    // should it be 'optimized'? somewhat yes, because a use
    // case could be to 'watch' (ie. reload at interval)
    // hence reload should not be an intensive operation
    for (size_t i = 0; i < old->count; i++) {
        struct Node* old_child = old->as.dir.children[i];

        if (cursor == old_child) found_cursor = true;

        for (size_t j = 0; j < niw->count; j++) {
            struct Node* niw_child = niw->as.dir.children[j];

            if (0 == strcmp(old_child->name, niw_child->name)) {
                if (cursor == old_child) {
                    cursor = niw_child;
                    moved_cursor = true;
                }

                struct Node* d = Type_LNK == old_child->type
                    ? old_child->as.link.tail
                    : old_child;
                if (d && Type_DIR == d->type && d->as.dir.unfolded) {
                    dir_unfold(niw_child);
                    _recurse_dir_reload(old_child, niw_child);
                }

                break;
            } // if found
        } // for in niw
    } // for in old

    if (found_cursor && !moved_cursor) cursor = niw;
}
void dir_reload(struct Node* node) {
    struct Node* dir = node;
    if (Type_LNK == dir->type) dir = dir->as.link.tail;
    if (!dir || Type_DIR != dir->type) return;

    bool is_unfolded = dir->as.dir.unfolded;
    bool is_root = &root == node;

    if (is_root) {
        may_malloc(node, sizeof(struct Node));
        memcpy(node, &root, sizeof(struct Node));
    }

    char* niw_path; may_strdup(niw_path, node->path);
    struct Node* niw = node_alloc(node->parent, niw_path);

    if (is_root) {
        if (!niw) die("Cannot access root anymore");
        if (Type_DIR != niw->type) {
            errno = ENOTDIR;
            die(node->path);
        }
        memcpy(&root, niw, sizeof(struct Node));
        niw = &root;
    } else {
        if (!niw) {
            node->parent->count--;
            for (size_t k = node->index; k < node->parent->count; k++) {
                node->parent->as.dir.children[k] = node->parent->as.dir.children[k+1];
                node->parent->as.dir.children[k]->index--;
            }
            node_free(node);
            return;
        } else node->parent->as.dir.children[node->index] = niw;
    }

    if (is_unfolded) dir_unfold(niw);
    if (cursor == node) cursor = niw;
    _recurse_dir_reload(node, niw);
    node_free(node);
    free(node);
}

static struct termios orig_termios;
static bool atexit_set = false;

void term_restore(void) {
    if (!is_tty) return;

    if (tcsetattr(STDOUT_FILENO, TCSAFLUSH, &orig_termios) < 0) die("tcrstattr");
    is_raw = false;
}

void term_raw_mode(void) {
    if (!is_tty) return;

    if (!atexit_set) {
        if (tcgetattr(STDOUT_FILENO, &orig_termios) < 0) die("tcgetattr");
        if (0 != atexit(term_restore)) die("atexit");
    }

    // cfmakeraw: 1960 magic shit
    struct termios raw = orig_termios;
    raw.c_iflag &= ~(IGNBRK | BRKINT | PARMRK | ISTRIP | INLCR | IGNCR | ICRNL | IXON);
    raw.c_oflag &= ~(OPOST);
    raw.c_lflag &= ~(ECHO | ECHONL | ICANON | ISIG | IEXTEN);
    raw.c_cflag &= ~(CSIZE | PARENB);
    raw.c_cflag |= (CS8);

    if (tcsetattr(STDOUT_FILENO, TCSAFLUSH, &raw) < 0) die("tcsetattr");
    is_raw = true;
}

size_t ignore_count = 0;
char** ignore_list = NULL;
bool node_ignore(struct Node* node) {
    if (!ignore_list) return false;

    size_t cwd_len = strlen(cwd);
    if ('/' != memcmp(node->path, cwd, cwd_len+1)) return false;

    char* node_rel = node->path+strlen(cwd)+1;

    for (size_t k = 0; k < ignore_count; k++) {
        char* patt = ignore_list[k];
        size_t len = strlen(patt);

        bool invert = '!' == ignore_list[k][0];
        bool beginning = '/' == patt[0];
        bool middle = memchr(patt+1, '/', len-2);
        bool end = '/' == patt[len-1];

        if (end && Type_DIR != node->type) continue;
        if (invert || '\\' == ignore_list[k][0]) patt++;
        if (beginning) patt++;

        if (end) patt[len-1] = '\0'; // XXX hack
        int no = fnmatch(patt, beginning || middle ? node_rel : node->name, FNM_PATHNAME);
        if (end) patt[len-1] = '/'; // XXX hack

        if (!no) return true;
    }
    return false;
}

int node_compare(struct Node* node, struct Node* mate, enum Sort order) {
    if (Sort_REVERSE & order)
        return -node_compare(node, mate, order & ~Sort_REVERSE);

    if (Sort_DIRSFIRST & order) {
        if (Type_DIR == node->type) return -1;
        if (Type_DIR == mate->type) return +1;
        return node_compare(node, mate, order & ~Sort_DIRSFIRST);
    }

    switch (order) {
        case Sort_NAME: return strcoll(node->name, mate->name);

        case Sort_SIZE: return mate->stat.st_size - node->stat.st_size;

        case Sort_EXTENSION: {
            char* xa = strrchr(node->name, '.');
            if (!xa) return -1;
            char* xb = strrchr(mate->name, '.');
            if (!xb) return +1;
            return strcmp(xa+1, xb+1);
        }

        case Sort_ATIME: return mate->stat.st_atime - node->stat.st_atime;
        case Sort_MTIME: return mate->stat.st_mtime - node->stat.st_mtime;
        case Sort_CTIME: return mate->stat.st_ctime - node->stat.st_ctime;

        default: ;
    }
    return 0; // unreachable
}

bool user_was_stdin = false;
bool user_was_loopback = false;

static void _notify_events(void) {
    char buf[4096] __attribute__((aligned(__alignof__(struct inotify_event))));
    struct inotify_event* event;
    ssize_t len;

    while (true) {
        len = read(NOTIFY_FILENO, buf, sizeof(buf));
        if (len <= 0) return;

        char* head = buf;
        while (head < buf + len) {
            event = (struct inotify_event *) head;

            if (event->mask & IN_ATTRIB) // TODO: reload the node's stat and type
                printf("notif: attrib '%s'\r\n", event->len ? event->name : "");
            if (event->mask & (IN_CREATE | IN_MOVED_TO)) // TODO: node_new and sort-insert
                printf("notif: create '%s'\r\n", event->len ? event->name : "");
            if (event->mask & (IN_DELETE | IN_MOVED_FROM)) // TODO: node_delete and update children
                printf("notif: delete '%s'\r\n", event->len ? event->name : "");
            //if (event->mask & (IN_DELETE_SELF | IN_MOVE_SELF))
            //    exit(EXIT_FAILURE); // only root node, exit

            // to get the dirname, find the corresponding wd
            // (to event->wd) and its associated path

            // to get full path, append the name (of len event->len)
            // if len is 0, event is on the dir itself (maybe?)
            // if so would only append for the `.._SELF`s

            // can also use `event->mask & IN_ISDIR` but probably usl

            head+= sizeof(struct inotify_event) + event->len;
        }
    }
}

int user_write(void* buf, size_t len) {
    return write(LOOPBACK_FILENO[LB_WRITE], buf, len);
}

int user_read(void* buf, size_t len) {
    fd_set cpy;
try_again: // jumps here when read retured 0 (EOF, closes fd)
    cpy = user_fds;
    if (select(9, &cpy, NULL, NULL, NULL) < 0) die("select");

    for (int i = 8; -1 < i; i--)
        if (FD_ISSET(i, &cpy)) {
            if (gflags.watch && NOTIFY_FILENO == i) {
                _notify_events();
                // YYY: what if it gets a notify even when user is in prompt :/
                // should return a buf="^L"/len=1 with user_was_loopback only set;
                // printers are expected to be able to handle this
                TREEST_UPDATE();
                goto try_again; // in that case, maybe not
            }

            user_was_stdin = STDIN_FILENO == i;
            user_was_loopback = LOOPBACK_FILENO[LB_READ] == i;
            size_t r = read(i, buf, len);
            if (0 == r) {
                FD_CLR(i, &user_fds);
                close(i);
                goto try_again;
            }
            return r;
        }

    return -1; // unreachable
}

static char** printer_argv = NULL;
static int printer_argc = 0;
static char* rcfile = NULL;
char* opts(int argc, char* argv[]) {
    selected_printer = &ascii_printer;
    char* selected_path = NULL;

    for (int k = 0; k < argc; k++) {
        if (0 == strcmp("--help", argv[k])) {
            printf("Usage: %s [--printer=NAME] [--LONGOPTIONS] [-FLAGS] [[--] ROOT]\n", prog);
            free(ignore_list);
            free(printer_argv);
            exit(EXIT_FAILURE);
        } else if (0 == strcmp("--version", argv[k])) {
            puts(
                TREEST_VERSION
                #ifdef FEAT_READLINE
                "\n+ readline"
                #endif
                #ifdef FEAT_GIT2
                "\n+ git2"
                #endif
            );
            free(ignore_list);
            free(printer_argv);
            exit(EXIT_SUCCESS);
        } else if (0 == memcmp("--printer=", argv[k], 10)) {
            char* arg = argv[k] + 10;
            #define DO(it) if (0 == strcmp(it.name, arg)) selected_printer = &it;
            EVERY_PRINTERS(DO, else)
            #undef DO
            else {
                printf("No such printer: '%s'\n", arg);
                free(ignore_list);
                free(printer_argv);
                exit(EXIT_FAILURE);
            }
        } else if (0 == memcmp("--ignore=", argv[k], 9)) {
            char* arg = argv[k] + 9;
            ignore_count++;
            if (!ignore_list) {
                may_malloc(ignore_list, ignore_count * sizeof(char*));
            } else {
                may_realloc(ignore_list, ignore_count * sizeof(char*));
            }
            ignore_list[ignore_count-1] = arg;
        } else if (0 == memcmp("--rcfile=", argv[k], 9)) {
            rcfile = argv[k] + 9;
        } else {
            if ('-' == argv[k][0]) {
                if ('-' == argv[k][1] && '\0' == argv[k][2]) {
                    selected_path = argv[k+1];
                    break;
                }
                printer_argc++;
                if (!printer_argv) {
                    may_malloc(printer_argv, printer_argc * sizeof(char*));
                } else {
                    may_realloc(printer_argv, printer_argc * sizeof(char*));
                }
                printer_argv[printer_argc-1] = argv[k];
            } else {
                selected_path = argv[k];
                break;
            }
        } // else (argv not long option)
    } // foreach argv

    return selected_path;
}

int main(int argc, char* argv[]) {
    #ifdef TRACE_ALLOCS
    mtrace();
    #endif
    if (!getcwd(cwd, _MAX_PATH)) die("getcwd");
    setlocale(LC_ALL, "");

    prog = argv[0];
    argv++;
    argc--;

    char* arg_path = opts(argc, argv);
    if (!arg_path) arg_path = cwd;
    char* path;
    struct stat sb;
    if (!(path = realpath(arg_path, NULL))) {
        if (selected_printer->del) selected_printer->del();
        free(ignore_list);
        free(printer_argv);
        die(arg_path);
    }
    if (lstat(path, &sb) < 0) {
        if (selected_printer->del) selected_printer->del();
        free(ignore_list);
        free(printer_argv);
        die(path);
    }
    if (!S_ISDIR(sb.st_mode)) {
        if (selected_printer->del) selected_printer->del();
        free(ignore_list);
        free(printer_argv);
        errno = ENOTDIR;
        die(path);
    }

    root.path = path;
    root.name = strrchr(path, '/')+1;
    root.type = Type_DIR;
    dir_unfold(&root);

    cursor = &root;

    if (pipe(LOOPBACK_FILENO) < 0) die("pipe(loopback)");
    // YYY: should probably O_NONBLOCK, but the real hack
    // is to fork exec a while (read && write); in the
    // mean time, no writing more than PIPE_BUF without
    // causing a deadlock - not that it matter 'cause this
    // 'feature' should not be used anyway...
    FD_ZERO(&user_fds);
    FD_SET(STDIN_FILENO, &user_fds);
    FD_SET(LOOPBACK_FILENO[LB_READ], &user_fds);

    if (selected_printer->init) selected_printer->init();
    for (int k = 0; k < printer_argc; k++) {
        if ('-' == printer_argv[k][1]) {
            if (!selected_printer->command || !selected_printer->command(printer_argv[k]+2)) {
                printf("Unknown command for '%s': '%s'\n", selected_printer->name, printer_argv[k]+2);
                if (selected_printer->del) selected_printer->del();
                free(ignore_list);
                free(printer_argv);
                exit(EXIT_FAILURE);
            }
            continue;
        }
        if (!selected_printer->toggle) continue;
        char* flag = printer_argv[k];
        while (*++flag) selected_printer->toggle(*flag);
    }
    free(printer_argv);

    if (gflags.watch) {
        NOTIFY_FILENO = inotify_init1(IN_NONBLOCK);
        FD_SET(NOTIFY_FILENO, &user_fds);
        dir_reload(&root);
    }

    if (rcfile) {
        int rcfd = open(rcfile, O_RDONLY);
        if (rcfd < 0) die(rcfile);
        FD_SET(rcfd, &user_fds);
    }

    if ((is_tty = isatty(STDOUT_FILENO))) term_raw_mode();

    while (true) {
        TREEST_UPDATE();
        TREEST_COMMAND();
    }
}
