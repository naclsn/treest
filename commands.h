#ifdef TREEST_COMMAND
#error This file should not be included outside treest.c
#endif // TREEST_COMMAND
#define TREEST_COMMAND(__x) {                                      \
    unsigned char user;                                            \
    do {                                                           \
        do {                                                       \
            if (read(STDIN_FILENO, &user, 1) < 0) die("read");     \
        } while (127 < user);                                      \
        if (aliases_map[user]) {                                   \
            for (unsigned char* it = aliases_map[user]; *it; it++) \
                if (commands_map[*it].f) commands_map[*it].f();    \
            break;                                                 \
        }                                                          \
    } while (!commands_map[user].f || !commands_map[user].f());    \
}

#include "./treest.h"

#define putstr(__c) if (write(STDERR_FILENO, __c, strlen(__c)) < 0) die("write")
#define putln() putstr(is_raw ? "\r\n" : "\n")

#undef CTRL
#define CTRL(x) ( (~x&64) | (~x&64)>>1 | (x&31) )

#define TOGGLE(flag) flag = !(flag)

bool toggle_gflag(char flag) {
    switch (flag) {
        case 'A':
        case 'a':
            TOGGLE(gflags.almost_all);
            return true;
    }
    return false;
}

static char* prompt_raw(const char* c) {
    ssize_t cap = 1024;
    ssize_t len = 0;
    char* buf = malloc(cap * sizeof(char));

    char last;
    putstr(c);
    putstr(": ");
    while (true) {
        if (read(STDIN_FILENO, &last, 1) < 0) die("read");
        if (CTRL('C') == last || CTRL('D') == last || CTRL('G') == last || CTRL('[') == last) {
            putstr("- aborted");
            putln();
            return NULL;
        }
        if (CTRL('H') == last || CTRL('?') == last) {
            if (0 < len) {
                putstr("\b \b");
                len--;
            }
            continue;
        }
        if (CTRL('W') == last) {
            while (0 < len) {
                putstr("\b \b");
                len--;
                if (' ' == buf[len-1]) break;
            }
            continue;
        }
        if (CTRL('I') == last) continue;
        if (CTRL('J') == last || CTRL('M') == last) break;

        if (write(STDERR_FILENO, &last, 1) < 0) die("write")
        if (cap < len) {
            cap*= 2;
            buf = realloc(buf, cap * sizeof(char));
        }
        buf[len++] = last;
    }
    putln();
    buf[len++] = '\0';

    return realloc(buf, len * sizeof(char));
}

#ifdef FEAT_READLINE
#include <readline/readline.h>
#include <readline/history.h>
static char* prompt_rl(const char* c) {
    size_t len = strlen(c);
    char p[len+2];
    strcpy(p, c);
    strcpy(p+len, ": ");
    term_restore();
    char* r = readline(p);
    term_raw_mode();
    add_history(r);
    return r;
}

static char* prompt_sel(const char* c);
char* (* prompt)(const char* c) = prompt_sel;
static char* prompt_sel(const char* c) { return (prompt = isatty(STDIN_FILENO) ? prompt_rl : prompt_raw)(c); }
#else
#define prompt prompt_raw
#endif

static char prompt1(const char* c) {
    char r;
    putstr(c);
    putstr(": ");
    if (read(STDIN_FILENO, &r, 1) < 0) die("read");
    if (CTRL('C') == r || CTRL('D') == r || CTRL('G') == r || CTRL('J') == r || CTRL('M') == r || CTRL('[') == r) {
        putstr("- aborted");
        putln();
        return 0;
    }
    char w[2] = {r};
    putstr(w);
    putln();
    return r;
}

static struct Node* locate(const char* path) {
    if ('/' != *path) {
        putstr("! absolute path must start with a /");
        putln();
        return NULL;
    }

    ssize_t rlen = strlen(root.path);
    if (0 != memcmp(root.path, path, rlen)) {
        putstr("! unrelated root");
        putln();
        return NULL;
    }

    const char* cast = path + rlen;
    const char* head;
    bool istail = false;
    struct Node* curr = &root;
    do {
        head = cast+1;
        if (!(cast = strchr(head, '/'))) {
            cast = head + strlen(head);
            istail = true;
        }

        if (head == cast) continue;

        if ('.' == *head) {
            if ('/' == *(head+1) || '\0' == *(head+1)) continue;
            if ('.' == *(head+1) && ('/' == *(head+2) || '\0' == *(head+2))) {
                if (&root == curr) {
                    putstr("! '..' goes above root");
                    putln();
                    return NULL;
                }
                curr = curr->parent;
                continue;
            }
        }

        if (Type_DIR != curr->type && (Type_LNK != curr->type || !curr->as.link.tail || Type_DIR != curr->as.link.tail->type)) {
            putstr("! path element is not a directory");
            putln();
            return NULL;
        }

        if (!curr->as.dir.unfolded) {
            dir_unfold(curr);
            dir_fold(curr);
        }
        bool found = false;
        for (size_t k = 0; k < curr->count; k++) {
            struct Node* it = curr->as.dir.children[k];
            if (0 == memcmp(it->name, head, cast-head)) {
                found = true;
                curr = Type_LNK == it->type && it->as.link.tail
                    ? it->as.link.tail
                    : it;
                break;
            }
        }

        if (!found) {
            putstr("! path not found");
            putln();
            return NULL;
        }
    } while (!istail && *head);

    struct Node* up = curr;
    while (up != &root) {
        up = up->parent;
        dir_unfold(up);
    }

    return curr;
}

static char* quote(char* text) {
    size_t cap = 64;
    size_t len = 0;
    char* ab = malloc(cap * sizeof(char));

    ab[len++] = '\'';

    char* cast = text;
    while ((cast = strchr(text, '\''))) {
        size_t add = cast-text;
        if (cap < len+add+4) {
            cap*= 2;
            ab = realloc(ab, cap * sizeof(char));
        }
        memcpy(ab+len, text, add);
        len+= add;
        memcpy(ab+len, "'\\''", 4);
        len+= 4;
        text = cast+1;
    }
    size_t left = strlen(text);
    if (cap < len+left+2) ab = realloc(ab, (len+left+2) * sizeof(char));
    memcpy(ab+len, text, left);
    len+= left;

    ab[len++] = '\'';
    ab[len] = '\0';

    return ab;
}

static bool c_quit(void) {
    exit(EXIT_SUCCESS);
}

static bool c_cquit(void) {
    char c = prompt1("exit-code");
    if (c) exit(c);
    exit(EXIT_FAILURE);
}

static bool c_ignore(void) {
    free(prompt("ignore"));
    return false;
}

static bool c_toggle(void) {
    char x = prompt1("toggle");
    if (x) {
        bool r = selected_printer->toggle(x);
        if (!r) {
            putstr("! no such flag");
            putln();
        } else dir_reload(&root);
        return r;
    }
    return false;
}

static bool c_refresh(void) {
    return true;
}

static bool c_previous(void) {
    struct Node* p = cursor->parent;
    if (p) {
        if (Type_LNK == p->type) p = p->as.link.tail;
        if (p && 0 < cursor->index) {
            cursor = p->as.dir.children[cursor->index - 1];
            return true;
        }
    }
    return false;
}

static bool c_next(void) {
    struct Node* p = cursor->parent;
    if (p) {
        if (Type_LNK == p->type) p = p->as.link.tail;
        if (p && cursor->index < p->count - 1) {
            cursor = p->as.dir.children[cursor->index + 1];
            return true;
        }
    }
    return false;
}

static bool c_child(void) {
    struct Node* d = cursor;
    if (Type_LNK == d->type) d = d->as.link.tail;
    if (d && Type_DIR == d->type) {
        dir_unfold(cursor);
        if (d->count) cursor = d->as.dir.children[0];
        return true;
    }
    return false;
}

static bool c_parent(void) {
    struct Node* p = cursor->parent;
    if (p) {
        cursor = p;
        return true;
    }
    return false;
}

static bool c_firstchild(void) {
    struct Node* p = cursor->parent;
    if (p) {
        if (Type_LNK == p->type) p = p->as.link.tail;
        cursor = p->as.dir.children[0];
        return true;
    }
    return false;
}

static bool c_lastchild(void) {
    struct Node* p = cursor->parent;
    if (p) {
        if (Type_LNK == p->type) p = p->as.link.tail;
        cursor = p->as.dir.children[p->count-1];
        return true;
    }
    return false;
}

static bool c_goroot(void) {
    cursor = &root;
    return true;
}

static bool c_reloadroot(void) {
    dir_reload(&root);
    return true;
}

static bool c_reload(void) {
    struct Node* d = cursor;
    if (Type_LNK == d->type) d = d->as.link.tail;
    if (d && Type_DIR == d->type) {
        dir_reload(d);
        return true;
    }
    return false;
}

static bool c_findstartswith(void) {
    char* c = prompt("find-startswith");
    if (!c) return false;
    size_t len = strlen(c);
    struct Node* p = cursor->parent;
    if (p) {
        if (Type_LNK == p->type) p = p->as.link.tail;
        for (size_t k = cursor->index+1; k < p->count; k++)
            if (0 == memcmp(c, p->as.dir.children[k]->name, len)) {
                free(c);
                cursor = p->as.dir.children[k];
                return true;
            }
        for (size_t k = 0; k < cursor->index-1; k++)
            if (0 == memcmp(c, p->as.dir.children[k]->name, len)) {
                free(c);
                cursor = p->as.dir.children[k];
                return true;
            }
    }
    free(c);
    return false;
}

static bool c_findendswith(void) {
    char* c = prompt("find-endswith");
    if (!c) return false;
    size_t len = strlen(c);
    struct Node* p = cursor->parent;
    if (p) {
        if (Type_LNK == p->type) p = p->as.link.tail;
        for (size_t k = cursor->index+1; k < p->count; k++)
            if (0 == memcmp(c, p->as.dir.children[k]->name+strlen(p->as.dir.children[k]->name)-len, len)) {
                free(c);
                cursor = p->as.dir.children[k];
                return true;
            }
        for (size_t k = 0; k < cursor->index-1; k++)
            if (0 == memcmp(c, p->as.dir.children[k]->name+strlen(p->as.dir.children[k]->name)-len, len)) {
                free(c);
                cursor = p->as.dir.children[k];
                return true;
            }
    }
    free(c);
    return false;
}

static bool c_findcontains(void) {
    char* c = prompt("find-contains");
    if (!c) return false;
    struct Node* p = cursor->parent;
    if (p) {
        if (Type_LNK == p->type) p = p->as.link.tail;
        for (size_t k = cursor->index+1; k < p->count; k++)
            if (NULL != strstr(p->as.dir.children[k]->name, c)) {
                free(c);
                cursor = p->as.dir.children[k];
                return true;
            }
        for (size_t k = 0; k < cursor->index-1; k++)
            if (NULL != strstr(p->as.dir.children[k]->name, c)) {
                free(c);
                cursor = p->as.dir.children[k];
                return true;
            }
    }
    free(c);
    return false;
}

static bool c_unfold(void) {
    struct Node* d = cursor;
    if (Type_LNK == d->type) d = d->as.link.tail;
    if (d && Type_DIR == d->type) {
        dir_unfold(cursor);
        return true;
    }
    return false;
}

static bool c_fold(void) {
    if (Type_DIR == cursor->type || Type_LNK == cursor->type) {
        dir_fold(cursor);
        return true;
    }
    return false;
}

static void _recurse_foldrec(struct Node* curr) {
    for (size_t k = 0; k < curr->count; k++) {
        struct Node* it = curr->as.dir.children[k];
        if (Type_LNK == it->type) it = it->as.link.tail;
        if (it && Type_DIR == it->type) _recurse_foldrec(it);
    }
    dir_fold(curr);
}
static bool c_foldrec(void) {
    struct Node* d = cursor;
    if (Type_LNK == d->type) d = d->as.link.tail;
    if (d && Type_DIR == d->type) {
        _recurse_foldrec(d);
        return true;
    }
    return false;
}

static bool c_promptunfold(void) {
    char* c = prompt("unfold-path");
    if (!c) return false;
    struct Node* found = locate(c);
    free(c);
    if (found) {
        struct Node* pre = cursor;
        cursor = found;
        c_unfold();
        cursor = pre;
        return true;
    }
    return false;
}

static bool c_promptfold(void) {
    char* c = prompt("fold-path");
    if (!c) return false;
    struct Node* found = locate(c);
    free(c);
    if (found) {
        struct Node* pre = cursor;
        cursor = found;
        c_fold();
        cursor = pre;
        return true;
    }
    return false;
}

static bool c_promptgounfold(void) {
    char* c = prompt("gounfold-path");
    if (!c) return false;
    struct Node* found = locate(c);
    free(c);
    if (found) {
        cursor = found;
        c_unfold();
        return true;
    }
    return false;
}

static bool c_promptgofold(void) {
    char* c = prompt("gofold-path");
    if (!c) return false;
    struct Node* found = locate(c);
    free(c);
    if (found) {
        cursor = found;
        c_fold();
        return true;
    }
    return false;
}

// XXX: remove (also in treest.c#opts)
static bool c_command(void) {
    char* c = prompt("command");
    if (!c) return false;
    bool r = selected_printer->command(c);
    free(c);
    return r;
}

static bool c_shell(void) {
    if (0 == system(NULL)) {
        putstr("! no shell available");
        return false;
    }

    char* c = prompt("shell-command");
    if (!c) return false;
    ssize_t clen = strlen(c);

    char* quoted = quote(cursor->path);
    size_t nlen = strlen(quoted);

    char* com = malloc(clen * sizeof(char));
    char* into = com;

    char* head = c;
    char* tail;
    while ((tail = strstr(head, "{}"))) {
        memcpy(into, head, tail-head);
        into+= tail-head;

        clen+= nlen;
        char* pcom = com;
        com = realloc(com, clen * sizeof(char));
        into+= com - pcom;

        strcpy(into, quoted);
        into+= nlen;

        head = tail+2;
    }
    strcpy(into, head);
    free(c);

    term_restore();
    int _usl = system(com); // YYY
    free(com);
    term_raw_mode();

    putstr("! done");
    if (read(STDIN_FILENO, &_usl, 1) < 0) die("read");

    dir_reload(&root);
    return true;
}

static bool c_pipe(void) {
    if (0 == system(NULL)) {
        putstr("! no shell available");
        return false;
    }

    char* c = prompt("pipe-command");
    if (!c) return false;
    ssize_t clen = strlen(c);

    char* quoted = quote(cursor->path);
    size_t nlen = strlen(quoted);

    clen+= nlen+1;

    char* com = malloc(clen * sizeof(char));
    char* into = com;

    char* head = c;
    char* tail;
    while ((tail = strstr(head, "{}"))) {
        memcpy(into, head, tail-head);
        into+= tail-head;

        clen+= nlen;
        char* pcom = com;
        com = realloc(com, clen * sizeof(char));
        into+= com - pcom;

        strcpy(into, quoted);
        into+= nlen;

        head = tail+2;
    }
    strcpy(into, head);

    into+= strlen(head);
    free(c);

    *into++ = '<';
    strcpy(into, quoted);
    *(into+nlen) = '\0';

    term_restore();
    int _usl = system(com); // YYY
    free(com);
    term_raw_mode();

    putstr("! done");
    if (read(STDIN_FILENO, &_usl, 1) < 0) die("read");

    dir_reload(&root);
    return true;
}

static bool c_alias(void) {
    unsigned char a = prompt1("alias-name");
    if (!a) return false;
    if (127 < a) {
        putstr("! not a valid alias name");
        putln();
        return false;
    }
    free(aliases_map[a]);
    char* c = prompt("alias-commands");
    aliases_map[a] = (unsigned char*)c;
    return !!c;
}

static bool c_help(void) {
    unsigned char c = prompt1("help-command");
    if (c < 128 && commands_map[c].h) {
        putstr(commands_map[c].h);
    } else putstr("! not a command");
    putln();
    return false;
}

// REM: `LC_ALL=C sort`
struct Command commands_map[128] = {
    [CTRL('C')]={c_quit,           "quit"},
    [CTRL('D')]={c_quit,           "quit"},
    [CTRL('R')]={c_reload,         "reload the directory at the cursor"},
    [CTRL('L')]={c_refresh,        "refresh the view"},
    ['!']      ={c_shell,          "execute a shell command"},
    ['"']      ={c_alias,          "create an alias from a name (single character) and a chain of commands"},
    ['#']      ={c_ignore,         "(comment) ignore input until the end of line"},
    ['$']      ={c_findendswith,   "find the next file which name ends with"},
    ['-']      ={c_toggle,         "toggle a flag"},
    ['/']      ={c_findcontains,   "find the next file which name contains"},
    [':']      ={c_command,        "execute a printer command"},
    ['=']      ={c_foldrec,        "fold recursively at the cursor"},
    ['?']      ={c_help,           "enter ? then a key combination to find out about the associated command"},
    ['C']      ={c_promptfold,     "fold at the given path"},
    ['H']      ={c_fold,           "fold at the cursor"},
    ['L']      ={c_unfold,         "unfold at the cursor"},
    ['O']      ={c_promptunfold,   "unfold at the given path"},
    ['Q']      ={c_cquit,          "quit with an exit code (by default indicating failure)"},
    ['[']      ={c_firstchild,     "go to the parent's first child"},
    [']']      ={c_lastchild,      "go to the parent's last child"},
    ['^']      ={c_findstartswith, "find the next file which name starts with"},
    ['`']      ={c_goroot,         "go to the root"},
    ['c']      ={c_promptgofold,   "go to and fold at the given path"}, // XXX: ?
    ['h']      ={c_parent,         "go to the parent directory"},
    ['j']      ={c_next,           "go to the next file"},
    ['k']      ={c_previous,       "go to the previous file"},
    ['l']      ={c_child,          "go to the directory's first child (unfold if needed)"},
    ['o']      ={c_promptgounfold, "go to and unfold at the given path"},
    ['q']      ={c_quit,           "quit"},
    ['|']      ={c_pipe,           "pipe content into a shell command"},
    ['~']      ={c_reloadroot,     "reload at the root (read the whole tree from file system)"},
};
unsigned char* aliases_map[128] = {0};
