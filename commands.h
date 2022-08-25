#ifdef TREEST_COMMAND
#error This file should not be included outside treest.c
#endif // TREEST_COMMAND
#define TREEST_COMMAND

#include "./treest.h"

static void _free_before_normal_exit();

#define putstr(__c) if (write(STDERR_FILENO, __c, strlen(__c)) < 0) die("write")
#define putln() putstr(is_raw ? "\r\n" : "\n")

#undef CTRL
#define CTRL(x) ( (~x&64) | (~x&64)>>1 | (x&31) )

#define TOGGLE(flag) flag = !flag
#define TOGGLE_BIT(array, flag) array^= flag
#define TOGGLE_SRT(array, flag) array = (array^flag) & (flag|Sort_REVERSE|Sort_DIRSFIRST)

bool toggle_gflag(char flag) {
    switch (flag) {
        case 'A': case 'a': TOGGLE(gflags.almost_all);           return true;
        case 'B': TOGGLE(gflags.ignore_backups);                 return true;
        case 'I': TOGGLE(gflags.ignore);                         return true;
        case 'S': TOGGLE_SRT(gflags.sort_order, Sort_SIZE);      return true;
        case 'X': TOGGLE_SRT(gflags.sort_order, Sort_EXTENSION); return true;
        case 'c': TOGGLE_SRT(gflags.sort_order, Sort_CTIME);     return true;
        case 'd': TOGGLE_BIT(gflags.sort_order, Sort_DIRSFIRST); return true;
        case 'r': TOGGLE_BIT(gflags.sort_order, Sort_REVERSE);   return true;
        case 't': TOGGLE_SRT(gflags.sort_order, Sort_MTIME);     return true;
        case 'u': TOGGLE_SRT(gflags.sort_order, Sort_ATIME);     return true;
        case 'w': TOGGLE(gflags.watch);                          return true;
    }
    return false;
}

bool run_command(char user) {
    unsigned char c = user;
    return c < 128 && command_map[c].f && command_map[c].f();
}

void run_commands(char* user) {
    user_write(user, strlen(user));
}

static char* prompt_raw(const char* c) {
    ssize_t cap = 1024;
    ssize_t len = 0;
    char* buf; may_malloc(buf, cap * sizeof(char));

    char last;
    putstr(c);
    putstr(": ");
    while (true) {
        if (user_read(&last, 1) < 0) die("read");
        if (CTRL('L') == last) {
            if (command_map[CTRL('L')].f)
                command_map[CTRL('L')].f();
            putstr(c);
            putstr(": ");
            buf[len] = '\0';
            putstr(buf);
            continue;
        }
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
        if (CTRL('I') == last && user_was_loopback) break;
        if (CTRL('J') == last || CTRL('M') == last) break;

        if (write(STDERR_FILENO, &last, 1) < 0) die("write")
        if (cap < len) {
            cap*= 2;
            may_realloc(buf, cap * sizeof(char));
        }
        buf[len++] = last;
    }
    putln();
    buf[len++] = '\0';

    may_realloc(buf, len * sizeof(char));
    return buf;
}

#ifdef FEAT_READLINE
#include <readline/readline.h>
#include <readline/history.h>
static char* prompt_rl(const char* c) {
    size_t len = strlen(c);
    char p[len+7];
    strcpy(p, c);
    strcpy(p+len, " (rl): ");
    term_restore();
    char* r = readline(p); // YYY: will not refresh on eg notify
    term_raw_mode();
    add_history(r);
    return r;
}
static char* prompt_impl(const char* c) { return (user_was_stdin && isatty(STDIN_FILENO) ? prompt_rl : prompt_raw)(c); }
#else
#define prompt_impl prompt_raw
#endif

static char* prompt(const char* c) {
    char* r = prompt_impl(c);
    size_t add = strlen(r);
    size_t len = strlen(register_map['.']);
    may_realloc(register_map['.'], (len+add+2) * sizeof(char));
    strcpy(register_map['.']+len, r);
    register_map['.'][len+add] = '\t';
    register_map['.'][len+add+1] = '\0';
    return r;
}

static char prompt1(const char* c) {
    char r;
try_again:
    putstr(c);
    putstr(": ");
    if (user_read(&r, 1) < 0) die("read");
    if (CTRL('L') == r) {
        if (command_map[CTRL('L')].f)
            command_map[CTRL('L')].f();
        goto try_again;
    }
    if (CTRL('C') == r || CTRL('D') == r || CTRL('G') == r || CTRL('J') == r || CTRL('M') == r || CTRL('[') == r) {
        putstr("- aborted");
        putln();
        return 0;
    }
    char w[2] = {r};
    putstr(w);
    putln();

    size_t len = strlen(register_map['.']);
    may_realloc(register_map['.'], (len+2) * sizeof(char));
    register_map['.'][len] = r;
    register_map['.'][len+1] = '\0';
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
    char* ab; may_malloc(ab, cap * sizeof(char));

    ab[len++] = '\'';

    char* cast = text;
    while ((cast = strchr(text, '\''))) {
        size_t add = cast-text;
        if (cap < len+add+4) {
            cap*= 2;
            may_realloc(ab, cap * sizeof(char));
        }
        memcpy(ab+len, text, add);
        len+= add;
        memcpy(ab+len, "'\\''", 4);
        len+= 4;
        text = cast+1;
    }
    size_t left = strlen(text);
    if (cap < len+left+2) may_realloc(ab, (len+left+2) * sizeof(char));
    memcpy(ab+len, text, left);
    len+= left;

    ab[len++] = '\'';
    ab[len] = '\0';

    return ab;
}

static bool c_quit(void) {
    _free_before_normal_exit();
    exit(EXIT_SUCCESS);
}

static bool c_cquit(void) {
    char c = prompt1("exit-code");
    _free_before_normal_exit();
    if (c) exit(c);
    exit(EXIT_FAILURE);
}

static bool c_suspend(void) {
    term_restore();
    raise(SIGSTOP); // YYY: or SIGTSTP?
    term_raw_mode();
    return true;
}

static bool c_ignore(void) {
    free(prompt("ignore"));
    return false;
}

static bool c_reloadroot(void) {
    dir_reload(&root);
    return true;
}

static bool c_toggle(void) {
    char x = prompt1("toggle");
    if (x) {
        bool r = selected_printer->toggle && selected_printer->toggle(x);
        if (!r) {
            putstr("! no such flag");
            putln();
        } else c_reloadroot();
        return r;
    }
    return false;
}

static bool c_refresh(void) {
    selected_printer->begin();
    node_print(&root, selected_printer);
    selected_printer->end();
    return true;
}

static bool c_unfold(void) {
    struct Node* d = cursor;
    if (Type_LNK == d->type) d = d->as.link.tail;
    if (!d || Type_DIR != d->type) return false;
    dir_unfold(cursor);
    return true;
}

static bool c_fold(void) {
    struct Node* d = cursor;
    if (Type_LNK == d->type) d = d->as.link.tail;
    if (!d || Type_DIR != d->type) return false;
    dir_fold(cursor);
    return true;
}

static bool c_previous(void) {
    struct Node* p = cursor->parent;
    if (!p) return false;
    if (Type_LNK == p->type) p = p->as.link.tail;
    if (0 == cursor->index) return false;
    cursor = p->as.dir.children[cursor->index-1];
    return true;
}

static bool c_next(void) {
    struct Node* p = cursor->parent;
    if (!p) return false;
    if (Type_LNK == p->type) p = p->as.link.tail;
    if (p->count-1 == cursor->index) return false;
    cursor = p->as.dir.children[cursor->index+1];
    return true;
}

static bool c_child(void) {
    if (!c_unfold()) return false;
    struct Node* d = cursor;
    if (Type_LNK == d->type) d = d->as.link.tail;
    if (0 != d->count) cursor = d->as.dir.children[0];
    return true; // YYY: even when no child..?
}

static bool c_firstchild(void) {
    struct Node* p = cursor->parent;
    if (!p) return false;
    if (Type_LNK == p->type) p = p->as.link.tail;
    cursor = p->as.dir.children[0];
    return true;
}

static bool c_lastchild(void) {
    struct Node* p = cursor->parent;
    if (!p) return false;
    if (Type_LNK == p->type) p = p->as.link.tail;
    cursor = p->as.dir.children[p->count-1];
    return true;
}

static bool c_parent(void) {
    if (!cursor->parent) return false;
    cursor = cursor->parent;
    return true;
}

static bool c_visiblechild(void) {
    struct Node* d = cursor;
    if (Type_LNK == d->type) d = d->as.link.tail;
    if (!d || Type_DIR != d->type) return false;
    if (!d->as.dir.unfolded || 0 == d->count) return false;
    cursor = d->as.dir.children[0];
    return true;
}

static bool c_visibleprevious(void) {
    if (c_previous()) {
        while (c_visiblechild()) c_lastchild();
        return true;
    }
    return c_parent();
}

static bool c_visiblenext(void) {
    if (c_visiblechild()) return true;
    if (c_next()) return true;
    struct Node* pcursor = cursor;
    if (!cursor->parent) return false;
    while (c_parent() && !c_next());
    if (&root != cursor) return true;
    cursor = pcursor;
    return false;
}

static bool c_goroot(void) {
    cursor = &root;
    return true;
}

static bool c_reload(void) {
    struct Node* d = cursor;
    if (Type_LNK == d->type) d = d->as.link.tail;
    if (d && Type_DIR == d->type) {
        dir_reload(cursor);
        return true;
    }
    return false;
}

static bool c_findnext(void) {
    if (!register_map['/']) return false;
    char* text = register_map['/']+1;
    size_t len = strlen(text);

    struct Node* p = cursor->parent;
    if (!p) return false;
    if (Type_LNK == p->type) p = p->as.link.tail;

    switch (text[-1]) {
        case '<':
            for (size_t k = cursor->index+1; k < p->count; k++)
                if (0 == memcmp(text, p->as.dir.children[k]->name, len)) {
                    cursor = p->as.dir.children[k];
                    return true;
                }
            for (size_t k = 0; k < cursor->index; k++)
                if (0 == memcmp(text, p->as.dir.children[k]->name, len)) {
                    cursor = p->as.dir.children[k];
                    return true;
                }
            return false;

        case '=':
            for (size_t k = cursor->index+1; k < p->count; k++)
                if (NULL != strstr(p->as.dir.children[k]->name, text)) {
                    cursor = p->as.dir.children[k];
                    return true;
                }
            for (size_t k = 0; k < cursor->index; k++) {
                if (NULL != strstr(p->as.dir.children[k]->name, text)) {
                    cursor = p->as.dir.children[k];
                    return true;
                }
            }
            return false;

        case '>':
            for (size_t k = cursor->index+1; k < p->count; k++)
                if (0 == memcmp(text, p->as.dir.children[k]->name+strlen(p->as.dir.children[k]->name)-len, len)) {
                    cursor = p->as.dir.children[k];
                    return true;
                }
            for (size_t k = 0; k < cursor->index; k++)
                if (0 == memcmp(text, p->as.dir.children[k]->name+strlen(p->as.dir.children[k]->name)-len, len)) {
                    cursor = p->as.dir.children[k];
                    return true;
                }
            return false;
    }
    return false;
}

static bool c_findprevious(void) {
    if (!register_map['/']) return false;
    char* text = register_map['/']+1;
    size_t len = strlen(text);

    struct Node* p = cursor->parent;
    if (!p) return false;
    if (Type_LNK == p->type) p = p->as.link.tail;

    switch (text[-1]) {
        case '<':
            for (size_t k = cursor->index-1; 0 < k+1; k--)
                if (0 == memcmp(text, p->as.dir.children[k]->name, len)) {
                    cursor = p->as.dir.children[k];
                    return true;
                }
            for (size_t k = p->count-1; cursor->index < k; k--)
                if (0 == memcmp(text, p->as.dir.children[k]->name, len)) {
                    cursor = p->as.dir.children[k];
                    return true;
                }
            return false;

        case '=':
            for (size_t k = cursor->index-1; 0 < k+1; k--)
                if (NULL != strstr(p->as.dir.children[k]->name, text)) {
                    cursor = p->as.dir.children[k];
                    return true;
                }
            for (size_t k = p->count-1; cursor->index < k; k--)
                if (NULL != strstr(p->as.dir.children[k]->name, text)) {
                    cursor = p->as.dir.children[k];
                    return true;
                }
            return false;

        case '>':
            for (size_t k = cursor->index-1; 0 < k+1; k--)
                if (0 == memcmp(text, p->as.dir.children[k]->name+strlen(p->as.dir.children[k]->name)-len, len)) {
                    cursor = p->as.dir.children[k];
                    return true;
                }
            for (size_t k = p->count-1; cursor->index < k; k--)
                if (0 == memcmp(text, p->as.dir.children[k]->name+strlen(p->as.dir.children[k]->name)-len, len)) {
                    cursor = p->as.dir.children[k];
                    return true;
                }
            return false;
    }
    return false;
}

static bool c_findstartswith(void) {
    char* text = prompt("find-startswith");
    may_realloc(register_map['/'], strlen(text)+2);
    strcpy(register_map['/']+1, text);
    free(text);
    *register_map['/'] = '<';
    return c_findnext();
}

static bool c_findcontains(void) {
    char* text = prompt("find-contains");
    may_realloc(register_map['/'], strlen(text)+2);
    strcpy(register_map['/']+1, text);
    free(text);
    *register_map['/'] = '=';
    return c_findnext();
}

static bool c_findendswith(void) {
    char* text = prompt("find-endswith");
    may_realloc(register_map['/'], strlen(text)+2);
    strcpy(register_map['/']+1, text);
    free(text);
    *register_map['/'] = '>';
    return c_findnext();
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

// XXX: hidden cursor
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

static bool c_toggleignore(void) {
    TOGGLE(gflags.ignore);
    return true;
}

static bool c_rerun(void) {
    if (!register_map['.'] && '\0' == *register_map['.'])
        return false;
    run_commands(register_map['.']);
    return true;
}

// XXX: idk
static bool c_command(void) {
    char* c = prompt("command");
    if (!c) return false;
    bool r = selected_printer->command && selected_printer->command(c);
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
    size_t clen = strlen(c);

    char* quoted = quote(cursor->path);
    size_t nlen = strlen(quoted);

    char* com; may_malloc(com, clen * sizeof(char));
    char* into = com;
    size_t ilen = 0;

    char* head = c;
    char* tail;
    while ((tail = strstr(head, "{}"))) {
        memcpy(into, head, tail-head);
        into+= tail-head;
        ilen+= tail-head;

        clen+= nlen;
        may_realloc(com, clen * sizeof(char));
        into = com + ilen;

        strcpy(into, quoted);
        into+= nlen;
        ilen+= nlen;

        head = tail+2;
    }
    strcpy(into, head);
    free(c);
    free(quoted);

    term_restore();
    int r = system(com); // YYY
    free(com);
    term_raw_mode();

    prompt1("! done"); // YYY

    c_reloadroot();
    return EXIT_SUCCESS == r;
}

static bool c_pipe(void) {
    if (0 == system(NULL)) {
        putstr("! no shell available");
        return false;
    }

    char* c = prompt("pipe-command");
    if (!c) return false;
    size_t clen = strlen(c);

    char* quoted = quote(cursor->path);
    size_t nlen = strlen(quoted);

    clen+= nlen+1;

    char* com; may_malloc(com, clen * sizeof(char));
    char* into = com;
    size_t ilen = 0;

    char* head = c;
    char* tail;
    while ((tail = strstr(head, "{}"))) {
        memcpy(into, head, tail-head);
        into+= tail-head;
        ilen+= tail-head;

        clen+= nlen;
        may_realloc(com, clen * sizeof(char));
        into = com + ilen;

        strcpy(into, quoted);
        into+= nlen;
        ilen+= nlen;

        head = tail+2;
    }
    strcpy(into, head);

    into+= strlen(head);
    free(c);

    *into++ = '<';
    strcpy(into, quoted);
    *(into+nlen) = '\0';
    free(quoted);

    term_restore();
    int r = system(com); // YYY
    free(com);
    term_raw_mode();

    prompt1("! done"); // YYY

    c_reloadroot();
    return EXIT_SUCCESS == r;
}

static bool c_if(void) {
    char a = prompt1("if-command");
    if (!a) return false;
    bool r = run_command(a);
    if (r) {
        char* c = prompt("then-commands");
        if (!c) return false;
        run_commands(c);
        free(c);
    }
    return r;
}

static bool c_ifnot(void) {
    char a = prompt1("ifnot-command");
    if (!a) return false;
    bool r = !run_command(a);
    if (r) {
        char* c = prompt("then-commands");
        if (!c) return false;
        run_commands(c);
        free(c);
    }
    return r;
}

static bool c_while(void) {
    char a = prompt1("while-command");
    if (!a) return false;
    bool r = run_command(a);
    while (r) {
        char* c = prompt("do-commands");
        if (!c) return false;
        run_commands(c);
        free(c);
    }
    return r;
}

static bool c_whilenot(void) {
    char a = prompt1("whilenot-command");
    if (!a) return false;
    bool r = !run_command(a);
    while (r) {
        char* c = prompt("do-commands");
        if (!c) return false;
        run_commands(c);
        free(c);
    }
    return r;
}

static bool c_register(void) {
    unsigned char a = prompt1("register-name");
    if (!a) return false;
    if (127 < a) {
        putstr("! not a valid register name");
        putln();
        return false;
    }
    free(register_map[a]);
    char* c = prompt("register-commands");
    register_map[a] = c;
    return !!c;
}

static bool c_runregister(void) {
    unsigned char a = prompt1("register-name");
    if (!a) return false;
    if (127 < a) {
        putstr("! not a valid register name");
        putln();
        return false;
    }
    if (register_map[a]) {
        run_commands(register_map[a]);
        return true;
    }
    return false;
}

static bool c_help(void) {
    unsigned char a = prompt1("help-command");
    if (a < 128 && command_map[a].h) {
        // YYY: deal with long lines?
        putstr(command_map[a].h);
    } else putstr("! not a command");
    putln();
    return false;
}

// REM: `LC_ALL=C sort`
struct Command command_map[128] = {
    [CTRL('C')]={c_quit,                 "quit"},
    [CTRL('H')]={c_toggleignore,         "toggle the ignore global flag"},
    [CTRL('L')]={c_refresh,              "refresh the view"},
    [CTRL('N')]={c_visiblenext,          "go to the next visible node"},
    [CTRL('P')]={c_visibleprevious,      "go to the previous visible node"},
    [CTRL('R')]={c_reload,               "reload the directory at the cursor"},
    [CTRL('Z')]={c_suspend,              "suspend"},
    ['!']      ={c_shell,                "execute a shell command"},
    ['"']      ={c_register,             "fill or empty a register"},
    ['#']      ={c_ignore,               "(comment) ignore input until the end of line"},
    ['$']      ={c_findendswith,         "find the next node which name ends with"},
    ['(']      ={c_if,                   "run commands if"},
    [')']      ={c_ifnot,                "run commands ifnot"},
    ['-']      ={c_toggle,               "toggle a flag"},
    ['.']      ={c_rerun,                "re-run the last command"},
    ['/']      ={c_findcontains,         "find the next node which name contains"},
    [':']      ={c_command,              "execute a printer command"},
    [';']      ={c_refresh,              "refresh the view"},
    ['=']      ={c_foldrec,              "fold recursively at the cursor"},
    ['?']      ={c_help,                 "print help for a given command"},
    ['C']      ={c_promptfold,           "fold at the given path"},
    ['H']      ={c_fold,                 "fold at the cursor"},
    ['L']      ={c_unfold,               "unfold at the cursor"},
    ['N']      ={c_findprevious,         "continue search backward"},
    ['O']      ={c_promptunfold,         "unfold at the given path"},
    ['Q']      ={c_cquit,                "quit with an exit code (by default indicating failure)"},
    ['[']      ={c_firstchild,           "go to the parent's first child"},
    ['\\']     ={c_runregister,          "run a register as a sequence of commands"},
    [']']      ={c_lastchild,            "go to the parent's last child"},
    ['^']      ={c_findstartswith,       "find the next node which name starts with"},
    ['`']      ={c_goroot,               "go to the root"},
    ['c']      ={c_promptgofold,         "go to and fold at the given path"}, // XXX: ?
    ['h']      ={c_parent,               "go to the parent directory"},
    ['j']      ={c_next,                 "go to the next node"},
    ['k']      ={c_previous,             "go to the previous node"},
    ['l']      ={c_child,                "go to the directory's first child (unfold if needed)"},
    ['n']      ={c_findnext,             "continue search forward"},
    ['o']      ={c_promptgounfold,       "go to and unfold at the given path"},
    ['q']      ={c_quit,                 "quit"},
    ['{']      ={c_while,                "run commands while"},
    ['|']      ={c_pipe,                 "pipe content into a shell command"},
    ['}']      ={c_whilenot,             "run commands whilenot"},
    ['~']      ={c_reloadroot,           "reload at the root (read the whole tree from file system)"},
};
char* register_map[128] = {0};

static void _free_before_normal_exit(void) {
    if (selected_printer->del) selected_printer->del();
    for (int k = 0; k < 128; k++) free(register_map[k]);
    node_free(&root);
}
