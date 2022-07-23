#ifdef TREEST_COMMAND
#error This file should not be included outside treest.c
#endif // TREEST_COMMAND
#define TREEST_COMMAND(__x) {                                 \
    unsigned char user;                                       \
    do {                                                      \
        if (read(0, &user, 1) < 0) die("read");               \
    } while (!command_map[user] || !command_map[user](user)); \
}

#include "./treest.h"

#define putstr(__c) if (write(STDERR_FILENO, __c, strlen(__c)) < 0) die("write")
#define putln() putstr("\r\n")

static bool (* command_map[128])();

static ssize_t prompt(const char* c, char* dest, ssize_t size);
static char prompt1(const char* c);

static struct Node* locate(const char* path);

bool toggle_gflag(char flag) {
    switch (flag) {
        case 'a': die("TODO"); return true; // Filter entries starting with '.'
    }
    return false;
}

static ssize_t prompt(const char* c, char* dest, ssize_t size) {
    ssize_t r = 0;
    char last;
    putstr(c);
    putstr(": ");
    do {
        if (read(STDIN_FILENO, dest, 1) < 0) die("read");
        last = *dest;
        if (3 == last || 4 == last || 7 == last || 27 == last)
            return 0;
        putstr(dest);
        if ('\b' == last) {
            putstr(" \b");
            r--; dest--;
            continue;
        }
        r++; dest++;
    } while (r < size && '\r' != last && '\n' != last);
    putln();
    *--dest = '\0';
    return r-1;
}

static char prompt1(const char* c) {
    char r;
    putstr(c);
    putstr(": ");
    if (read(STDIN_FILENO, &r, 1) < 0) die("read");
    putln();
    return r;
}

static struct Node* locate(const char* path) {
    if ('/' != *path) {
        putstr("! absolute path must start with a /\r\n");
        return NULL;
    }

    ssize_t rlen = strlen(root.path);
    if (0 != memcmp(root.path, path, rlen)) {
        putstr("! unrelated root\r\n");
        return NULL;
    }

    const char* cast = path + rlen;
    const char* head;
    bool istail = false;
    bool isfail = false;
    struct Node* cd = &root;
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
                if (&root == cd) {
                    putstr("! '..' goes above root\r\n");
                    isfail = true;
                    break;
                }
                cd = cd->parent;
                continue;
            }
        }

        if (Type_DIR != cd->type) {
            putstr("! path element is not a directory\r\n");
            isfail = true;
            break;
        }

        dir_unfold(cd);
        bool found = false;
        for (size_t k = 0; k < cd->count; k++) {
            struct Node* it = cd->as.dir.children[k];
            if (0 == memcmp(it->name, head, cast-head)) {
                found = true;
                cd = Type_LNK == it->type && it->as.link.tail
                    ? it->as.link.tail
                    : it;
                break;
            }
        }

        if (!found) {
            putstr("! path not found\r\n");
            isfail = true;
            break;
        }
    } while (!istail && *head);

    if (isfail) {
        die("TODO: re-fold (cd, cd->parent, cd->parent->parent, ...)");
        return NULL;
    }
    return cd;
}

static bool c_quit() {
    exit(EXIT_SUCCESS);
    return false;
}

static bool c_cquit() {
    exit(EXIT_FAILURE);
    return false;
}

static bool c_toggle() {
    char x = prompt1("toggle");
    return selected_printer->toggle(x);
    return false;
}

static bool c_previous() {
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

static bool c_next() {
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

static bool c_child() {
    struct Node* d = cursor;
    if (Type_LNK == d->type) d = d->as.link.tail;
    if (d && Type_DIR == d->type) {
        dir_unfold(d);
        if (d->count) cursor = d->as.dir.children[0];
        return true;
    }
    return false;
}

static bool c_parent() {
    struct Node* p = cursor->parent;
    if (p) {
        cursor = p;
        return true;
    }
    return false;
}

static bool c_unfold() {
    struct Node* d = cursor;
    if (Type_LNK == d->type) d = d->as.link.tail;
    if (d && Type_DIR == d->type) {
        dir_unfold(d);
        return true;
    }
    return false;
}

static bool c_fold() {
    if (Type_DIR == cursor->type) {
        dir_fold(cursor);
        return true;
    }
    return false;
}

static bool c_foldall() {
    // TODO: better (doesn't close all)
    while (cursor != &root) {
        c_fold();
        c_parent();
    }
    return true;
}

static bool c_promptunfold() {
    char c[_MAX_PATH] = {0};
    if (!prompt("unfold-path", c, _MAX_PATH)) return false;
    struct Node* found = locate(c);
    if (found) {
        struct Node* pre = cursor;
        cursor = found;
        c_unfold();
        cursor = pre;
        return true;
    }
    return false;
}

static bool c_promptfold() {
    char c[_MAX_PATH] = {0};
    if (!prompt("fold-path", c, _MAX_PATH)) return false;
    struct Node* found = locate(c);
    if (found) {
        struct Node* pre = cursor;
        cursor = found;
        c_fold();
        cursor = pre;
        return true;
    }
    return false;
}

static bool c_promptgounfold() {
    char c[_MAX_PATH] = {0};
    if (!prompt("gounfold-path", c, _MAX_PATH)) return false;
    struct Node* found = locate(c);
    if (found) {
        cursor = found;
        c_unfold();
        return true;
    }
    return false;
}

static bool c_promptgofold() {
    char c[_MAX_PATH] = {0};
    if (!prompt("gofold-path", c, _MAX_PATH)) return false;
    struct Node* found = locate(c);
    if (found) {
        cursor = found;
        c_fold();
        return true;
    }
    return false;
}

// REM: please remember to `LC_ALL=C sort`
static bool (* command_map[128])() = {
    [  3]=c_quit,         // ^C (ETX)
    [  4]=c_quit,         // ^D (EOT)
    ['-']=c_toggle,
    ['0']=c_foldall,
    ['C']=c_promptfold,
    ['H']=c_fold,
    ['L']=c_unfold,
    ['O']=c_promptunfold,
    ['Q']=c_cquit,
    ['c']=c_promptgofold,
    ['h']=c_parent,
    ['j']=c_next,
    ['k']=c_previous,
    ['l']=c_child,
    ['o']=c_promptgounfold,
    ['q']=c_quit,
};

/*
static void do_command(char x) {
    switch (x) {
        case 'O': break; // (prompt) unfold by path
        case 'C': break; // (prompt) fold by path
        case 'o': break; // (prompt) go to and unfold by path
        case 'c': break; // (prompt) go to and fold by path

        case '0': break; // fold all

        case '^': break; // (prompt) go to basename starting with
        case '$': break; // (prompt) go to basename ending with
        case '/': break; // (prompt) go to basename containing

        case '-': break; // (prompt-1) toggle flag

        case '?': break; // help? (prompt-1?)

        case  5: // ^E (mouse down)
        case 25: // ^Y (mouse up)
            break;

        case 10: // ^J
        case 13: // ^M (return)
            break;

        case  3: // ^C
        case  4: // ^D
        case  7: // ^G

        //case 27: // ^[ // note to self: shall escape from any prompt/pending

        case 'q': exit(0);
        case 'Q': exit(1);
    }
}
*/
