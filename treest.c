#include "./treest.h"
#include "./commands.h"

#pragma region node methods (free and print)

void def_free(struct Node* node) {
    free(node->path);
}

void dir_free(struct Node* node) {
    for (size_t k = 0; k < node->as.dir.children_count; k++) {
        struct Node* child = node->as.dir.children[k];
        switch (child->type) {
            case Type_DIR: dir_free(child); break;
            case Type_LNK: lnk_free(child); break;
            default:       def_free(child); break;
        }
        free(child);
    }
    node->as.dir.children_count = 0;
    free(node->as.dir.children);
    node->as.dir.children = NULL;
}

void lnk_free(struct Node* node) {
    free(node->path);
    struct Node* child = node->as.link.to;
    switch (child->type) {
        case Type_DIR: dir_free(child); break;
        case Type_LNK: lnk_free(child); break;
        default:       def_free(child); break;
    }
    free(node->as.link.to);
}

void def_print(struct Node* node, struct Printer* pr, size_t index, size_t count) {
    pr->node(pr, node, index, count);
}

void dir_print(struct Node* node, struct Printer* pr, size_t index, size_t count) {
    pr->node(pr, node, index, count);
    struct Dir dir = node->as.dir;
    if (dir.unfolded) {
        pr->enter(pr, node, index, count);
        size_t total = dir.children_count;
        for (size_t k = 0; k < total; k++) {
            struct Node* child = dir.children[k];
            switch (child->type) {
                case Type_DIR: dir_print(child, pr, k, total); break;
                case Type_LNK: lnk_print(child, pr, k, total); break;
                default:       def_print(child, pr, k, total); break;
            }

        }
        pr->leave(pr, node, index, count);
    }
}

void lnk_print(struct Node* node, struct Printer* pr, size_t index, size_t count) {
    pr->node(pr, node, index, count);
    if (Type_DIR == node->as.link.to->type) {
        struct Dir dir = node->as.link.to->as.dir;
        if (dir.unfolded) {
            pr->enter(pr, node, index, count);
            size_t total = dir.children_count;
            for (size_t k = 0; k < total; k++) {
                struct Node* child = dir.children[k];
                switch (child->type) {
                    case Type_DIR: dir_print(child, pr, k, total); break;
                    case Type_LNK: lnk_print(child, pr, k, total); break;
                    default:       def_print(child, pr, k, total); break;
                }
            }
            pr->leave(pr, node, index, count);
        }
    }
}

#pragma endregion

#pragma region directory fold/unfold

void dir_unfold(struct Node* node) {
    node->as.dir.unfolded = true;
    if (node->as.dir.children) return;

    size_t parent_path_len = strlen(node->path);

    size_t cap = 16;
    node->as.dir.children = malloc(cap * sizeof(struct Node*));

    DIR *dir = opendir(node->path);
    if (dir) {
        struct dirent *ent;
        while (ent = readdir(dir)) {
            if ('.' == ent->d_name[0] && ('\0' == ent->d_name[1] || '.' == ent->d_name[1] && '\0' == ent->d_name[2]))
                continue;

            if (cap <= node->as.dir.children_count) {
                cap*= 2;
                node->as.dir.children = realloc(node->as.dir.children, cap * sizeof(struct Node*));
            }

            char* path = malloc(parent_path_len+2 +
                #ifdef _DIRENT_HAVE_D_NAMLEN
                    ent->d_namlen
                #else
                    strlen(ent->d_name)
                #endif
            );
            strcpy(path, node->path);

            char* name = path + parent_path_len;
            if ('/' != name[-1]) *name++ = '/';
            strcpy(name, ent->d_name);

            enum Type type =
                #ifdef _DIRENT_HAVE_D_TYPE
                    ent->d_type
                #else
                    Type_UNKNOWN
                #endif
            ;

            if (Type_UNKNOWN == type || Type_REG == type) {
                struct stat sb;
                if (0 == stat(path, &sb) && sb.st_mode & S_IXUSR)
                    type = Type_EXEC;
            } else if (Type_LNK == type) {
                // TODO
            }

            struct Node* niw = malloc(sizeof(struct Node));
            struct Node fill = {
                .path=path,
                .name=name,
                .type=type,
                .parent=node,
            };
            memcpy(niw, &fill, sizeof(struct Node));
            node->as.dir.children[node->as.dir.children_count++] = niw;
        }
        closedir(dir);
    }
}

void dir_fold(struct Node* node) {
    node->as.dir.unfolded = false;
}

#pragma endregion

#pragma region cfmakeraw: 1960 magic shit

static struct termios orig_termios;

static void term_restore() {
    tcsetattr(STDOUT_FILENO, TCSAFLUSH, &orig_termios);
}

static void term_raw_mode() {
    if (!(is_tty = isatty(STDOUT_FILENO))) return;

    if (tcgetattr(STDOUT_FILENO, &orig_termios) < 0) {
        perror("tcgetattr");
        exit(errno);
    }

    atexit(term_restore);

    struct termios raw = orig_termios;
    raw.c_iflag &= ~(IGNBRK | BRKINT | PARMRK | ISTRIP | INLCR | IGNCR | ICRNL | IXON);
    raw.c_oflag &= ~(OPOST);
    raw.c_lflag &= ~(ECHO | ECHONL | ICANON | ISIG | IEXTEN);
    raw.c_cflag &= ~(CSIZE | PARENB);
    raw.c_cflag |= (CS8);

    if (tcsetattr(STDOUT_FILENO, TCSAFLUSH, &raw) < 0) {
        perror("tcsetattr");
        exit(errno);
    }
}

#pragma endregion

char* opts(int argc, char* argv[]) {
    char delay_toggle_flags[256] = {0};
    char* flag = &delay_toggle_flags[0];

    selected_printer = &ascii_printer;
    char* selected_path = NULL;

    for (int k = 0; k < argc; k++) {
        if (0 == strcmp("--version", argv[k])) {
            puts(TREEST_VERSION);
            exit(0);
        } else if (0 == memcmp("--printer=", argv[k], 10)) {
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
                    printf("Unknown argument '%s'\n", argv[k]);
                    exit(2);
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
        selected_printer->toggle(selected_printer, *flag);

    return selected_path;
}

void main(int argc, char* argv[]) {
    prog = argv[0];
    argv++;
    argc--;
    if (1 == argc && (0 == strcmp("-h", argv[0]) || 0 == strcmp("--help", argv[0]))) {
        printf("Usage: %s [--printer=name] [-flags] [[--] root]\n", prog);
        exit(2);
    }

    char* arg_path = opts(argc, argv);

    getcwd(cwd, _MAX_PATH);
    if (!arg_path) arg_path = cwd;

    char path[_MAX_PATH];
    if (NULL == realpath(arg_path, path)) {
        perror(arg_path);
        exit(errno);
    }

    root.path = path;
    root.name = basename(path);
    root.type = Type_DIR;

    dir_unfold(&root);

    term_raw_mode();
    char user;

    while (1) {
        selected_printer->begin(selected_printer);
        dir_print(&root, selected_printer, 0, 1);
        selected_printer->end(selected_printer);

        read(0, &user, 1);
        TREEST_COMMAND(user);
    }
}
