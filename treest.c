#include "./treest.h"
#include "./commands.h"

char* prog;
char cwd[_MAX_PATH];
bool is_tty;
bool is_raw;
struct GFlags gflags;
struct Node root, * cursor;
struct Printer* selected_printer;

struct Node* node_alloc(struct Node* parent, size_t index, char* path) {
    char* name = strrchr(path, '/')+1;

    struct stat sb;
    enum Type type = stat(path, &sb) < 0
        ? Type_UNKNOWN
        : S_IFMT & sb.st_mode;

    struct Node* niw = malloc(sizeof(struct Node));
    struct Node fill = {
        .path=path,
        .name=name,
        .type=type,
        .parent=parent,
        .index=index
    };
    memcpy(niw, &fill, sizeof(struct Node));

    if (Type_REG == type) {
        if (sb.st_mode & S_IXUSR)
            niw->type = Type_EXEC;
    } else if (Type_LNK == type) {
        // TODO: handle ELOOP as broken symlinks
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
    node_free(node->as.link.to);
    free(node->as.link.to);
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
    if (Type_LNK == node->type) node = node->as.link.tail;
    if (node && Type_DIR == node->as.link.to->type)
        dir_print(node->as.link.to, pr);
}

void lnk_resolve(struct Node* node) {
    char relpath[_MAX_PATH];
    relpath[readlink(node->path, relpath, _MAX_PATH-1)+1] = '\0';

    char fullpath[_MAX_PATH];
    char* paste;
    char* copy;
    if ('/' == relpath[0]) {
        paste = copy = strcpy(fullpath, relpath)+1;
    } else {
        paste = strcpy(fullpath, node->path) + strlen(node->path);
        copy = relpath;
    }

    if ('/' != paste[-1]) *paste++ = '/';
    do {
        if ('.' == *copy) {
            if ('.' == *(copy+1)) {
                paste--;
                if (fullpath == paste) {
                    // TODO: handle beyond root as broken symlink
                    errno = ENOTDIR;
                    die(relpath);
                }
                while ('/' != *--paste);
                paste++;
            } else if ('/' == *(copy+1)) copy++;
        } else if ('/' == *copy) {
            *paste++ = '/';
            while ('/' == *(copy+1)) copy++;
        } else *paste++ = *copy;
    } while (*copy++);
    paste--;
    while ('/' == *(paste-1)) paste--;
    *paste = '\0';

    char* path = strdup(fullpath);

    struct Node* niw = node_alloc(node->parent, node->index, path);
    node->as.link.to = niw;
    node->as.link.tail = Type_LNK == niw->type
        ? niw->as.link.tail
        : niw;
}

void dir_unfold(struct Node* node) {
    if (Type_LNK == node->type) node = node->as.link.tail;
    if (!node || Type_DIR != node->type) return;

    node->as.dir.unfolded = true;
    if (node->as.dir.children) return;

    size_t parent_path_len = strlen(node->path);

    size_t cap = 16;
    node->as.dir.children = malloc(cap * sizeof(struct Node*));

    DIR *dir = opendir(node->path);
    if (dir) {
        struct dirent *ent;
        while ((ent = readdir(dir))) {
            if ('.' == ent->d_name[0] && ('\0' == ent->d_name[1]
            || ('.' == ent->d_name[1] && '\0' == ent->d_name[2])))
                continue;

            if (cap <= node->count) {
                cap*= 2;
                node->as.dir.children = realloc(node->as.dir.children, cap * sizeof(struct Node*));
            }

            size_t path_len = parent_path_len+2 + strlen(ent->d_name);
            char* path = malloc(path_len);
            strcpy(path, node->path);

            char* name = path + parent_path_len;
            if ('/' != name[-1]) *name++ = '/';
            strcpy(name, ent->d_name);

            struct Node* niw = node_alloc(node, node->count, path);
            node->as.dir.children[node->count++] = niw;
        }
        closedir(dir);
    }
}

void dir_fold(struct Node* node) {
    if (Type_LNK == node->type) node = node->as.link.tail;
    if (!node || Type_DIR != node->type) return;

    node->as.dir.unfolded = false;
}

void dir_reload(struct Node* node) {
    if (Type_LNK == node->type) node = node->as.link.tail;
    if (!node || Type_DIR != node->type) return;

    bool unfolded = node->as.dir.unfolded;
    dir_free(node);
    if (unfolded) dir_unfold(node);
}

static struct termios orig_termios;
static bool atexit_set = false;

void term_restore(void) {
    if (!is_tty) return;

    if (tcsetattr(STDOUT_FILENO, TCSAFLUSH, &orig_termios) < 0) die("tcresattr");
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

char* opts(unsigned argc, char* argv[]) {
    char delay_toggle_flags[256] = {0};
    char* flag = &delay_toggle_flags[0];

    selected_printer = &ascii_printer;
    char* selected_path = NULL;

    for (unsigned k = 0; k < argc; k++) {
        if (0 == memcmp("--printer=", argv[k], 10)) {
            char* arg = argv[k] + 10;
            #define DO(ident, name) if (0 == strcmp(name, arg)) selected_printer = &(ident);
            #define SEP else
            EVERY_PRINTERS(DO, SEP)
            #undef DO
            #undef SEP
            else {
                printf("No such printer: '%s'\n", arg);
                exit(3);
            }
        } else {
            if ('-' == argv[k][0]) {
                if ('-' == argv[k][1]) {
                    if ('\0' == argv[k][2]) {
                        selected_path = argv[k+1];
                        break;
                    }
                    if (!selected_printer->command(argv[k]+2)) {
                        printf("Unknown argument '%s'\n", argv[k]);
                        exit(2);
                    }
                }
                strcpy(flag, argv[k]+1);
                flag+= strlen(argv[k]+1);
            } else {
                selected_path = argv[k];
                break;
            }
        }
    }

    while (delay_toggle_flags != flag--)
        selected_printer->toggle(*flag);

    return selected_path;
}

int main(int argc, char* argv[]) {
    prog = argv[0];
    argv++;
    argc--;
    if (1 == argc) {
        if (0 == strcmp("--help", argv[0])) {
            printf("Usage: %s [--printer=NAME] [--LONGOPTIONS] [-FLAGS] [[--] ROOT]\n", prog);
            exit(2);
        } else if (0 == strcmp("--version", argv[0])) {
            puts(TREEST_VERSION);
            exit(0);
        }
    }

    char* arg_path = opts(argc, argv);
    if (!getcwd(cwd, _MAX_PATH)) die("getcwd");
    if (!arg_path) arg_path = cwd;
    char path[_MAX_PATH];
    if (NULL == realpath(arg_path, path)) die(arg_path);

    root.path = path;
    root.name = basename(path);
    root.type = Type_DIR;
    dir_unfold(&root);

    cursor = &root;

    if ((is_tty = isatty(STDOUT_FILENO))) term_raw_mode();

    selected_printer->init();

    while (1) {
        selected_printer->begin();
        node_print(&root, selected_printer);
        selected_printer->end();

        TREEST_COMMAND(user);
    }
}
