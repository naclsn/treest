#include "./treest.h"
#include "./commands.h"

char* prog;
char cwd[_MAX_PATH];
bool is_tty;
bool is_raw;
struct GFlags gflags;
struct Node root, * cursor;
struct Printer* selected_printer;

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

    if (Type_REG == fill.type) {
        if (fill.stat.st_mode & S_IXUSR)
            niw->type = Type_EXEC;
    } else if (Type_LNK == fill.type) {
        // TODO: handle looping symlinks as broken
        lnk_resolve(niw);
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

            //if (node_ignore(niw)) {
            //    free(niw);
            //    continue;
            //}

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

    for (size_t k = 0; k < node->count; k++) node->as.dir.children[k]->index = k;

    if (0 == (parent->count = node->count)) node->as.dir.children = NULL;
    else may_realloc(node->as.dir.children, node->count * sizeof(struct Node*));
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

//static bool _path_match(char* patt, char* path) {
//    return false;
//}
//bool node_ignore(struct Node* node) {
//    return false;
//}

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

char* opts(int argc, char* argv[]) {
    selected_printer = &ascii_printer;
    bool printer_init = false;

    char* selected_path = NULL;

    for (int k = 0; k < argc; k++) {
        if (0 == memcmp("--printer=", argv[k], 10)) {
            char* arg = argv[k] + 10;
            if (printer_init) {
                selected_printer->del();
                printer_init = false;
            }
            #define DO(it) if (0 == strcmp(it.name, arg)) selected_printer = &it;
            EVERY_PRINTERS(DO, else)
            #undef DO
            else {
                printf("No such printer: '%s'\n", arg);
                if (printer_init) selected_printer->del();
                exit(EXIT_FAILURE);
            }
        } else {
            if ('-' == argv[k][0]) {
                if ('-' == argv[k][1]) {
                    if ('\0' == argv[k][2]) {
                        selected_path = argv[k+1];
                        break;
                    }
                    if (!selected_printer->command(argv[k]+2)) {
                        printf("Unknown command for '%s': '%s'\n", selected_printer->name, argv[k]+2);
                        if (printer_init) selected_printer->del();
                        exit(EXIT_FAILURE);
                    }
                }
                char* flag = argv[k];
                if (!printer_init) {
                    selected_printer->init();
                    printer_init = true;
                }
                while (*++flag) selected_printer->toggle(*flag);
            } else {
                selected_path = argv[k];
                break;
            }
        }
    }

    if (!printer_init) selected_printer->init();

    return selected_path;
}

int main(int argc, char* argv[]) {
    #ifdef TRACE_ALLOCS
    mtrace();
    #endif

    prog = argv[0];
    argv++;
    argc--;
    if (1 == argc) {
        if (0 == strcmp("--help", argv[0])) {
            printf("Usage: %s [--printer=NAME] [--LONGOPTIONS] [-FLAGS] [[--] ROOT]\n", prog);
            exit(EXIT_FAILURE);
        } else if (0 == strcmp("--version", argv[0])) {
            puts(TREEST_VERSION);
            exit(EXIT_SUCCESS);
        }
    }

    setlocale(LC_ALL, "");

    char* arg_path = opts(argc, argv);
    if (!getcwd(cwd, _MAX_PATH)) die("getcwd");
    if (!arg_path) arg_path = cwd;
    char* path;
    if (!(path = realpath(arg_path, NULL))) die(arg_path);
    struct stat sb;
    if (lstat(path, &sb) < 0) die(path);
    if (!S_ISDIR(sb.st_mode)) {
        errno = ENOTDIR;
        die(path);
    }

    root.path = path;
    root.name = strrchr(path, '/')+1;
    root.type = Type_DIR;
    dir_unfold(&root);

    cursor = &root;

    if ((is_tty = isatty(STDOUT_FILENO))) term_raw_mode();

    while (true) {
        selected_printer->begin();
        node_print(&root, selected_printer);
        selected_printer->end();

        TREEST_COMMAND();
    }
}
