#ifdef TREEST_COMMAND
#error This file should not be included outside treest.c
#endif // TREEST_COMMAND
#define TREEST_COMMAND(__x) {       \
    unsigned char user;             \
    do {                            \
        if (read(0, &user, 1) < 0)  \
            die("read");            \
    } while (!command_map[user]);   \
    command_map[user](user);        \
}

#include "./treest.h"

#define putstr(__c) if (write(STDERR_FILENO, __c, strlen(__c)) < 0) die("write")
#define putln() putstr("\r\n")

static void (* command_map[128])();

static ssize_t prompt(const char* c, char* dest, ssize_t size);
static char prompt1(const char* c);

static struct Node* locate(const char* path);

void toggle_gflag(char flag) {
    switch (flag) {
        case 'a': die("TODO"); break; // Filter entries starting with '.'
    }
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
    printf("TODO: locate(\"%s\")\r\n", path);
    return NULL;
}

static void c_quit() {
    exit(EXIT_SUCCESS);
}

static void c_cquit() {
    exit(EXIT_FAILURE);
}

static void c_toggle() {
    char x = prompt1("toggle");
    selected_printer->toggle(x);
}

static void c_previous() {
    struct Node* p = cursor->parent;
    if (p) {
        if (Type_LNK == p->type) p = p->as.link.tail;
        if (p && 0 < cursor->index)
            cursor = p->as.dir.children[cursor->index - 1];
    }
}

static void c_next() {
    struct Node* p = cursor->parent;
    if (p) {
        if (Type_LNK == p->type) p = p->as.link.tail;
        if (p && cursor->index < p->count - 1)
            cursor = p->as.dir.children[cursor->index + 1];
    }
}

static void c_child() {
    struct Node* d = cursor;
    if (Type_LNK == d->type) d = d->as.link.tail;
    if (d && Type_DIR == d->type) {
        dir_unfold(d);
        if (d->count) cursor = d->as.dir.children[0];
    }
}

static void c_parent() {
    struct Node* p = cursor->parent;
    if (p) cursor = p;
}

static void c_unfold() {
    struct Node* d = cursor;
    if (Type_LNK == d->type) d = d->as.link.tail;
    if (d && Type_DIR == d->type) dir_unfold(d);
}

static void c_fold() {
    if (Type_DIR == cursor->type) dir_fold(cursor);
}

static void c_foldall() {
    // TODO: better (doesn't close all)
    while (cursor != &root) {
        c_fold();
        c_parent();
    }
}

static void c_promptunfold() {
    char c[_MAX_PATH] = {0};
    if (!prompt("unfold-path", c, _MAX_PATH)) return;
    struct Node* found = locate(c);
    if (found) {
        struct Node* pre = cursor;
        cursor = found;
        c_unfold();
        cursor = pre;
    }
}

static void c_promptfold() {
    char c[_MAX_PATH] = {0};
    if (!prompt("fold-path", c, _MAX_PATH)) return;
    struct Node* found = locate(c);
    if (found) {
        struct Node* pre = cursor;
        cursor = found;
        c_fold();
        cursor = pre;
    }
}

static void c_promptgounfold() {
    char c[_MAX_PATH] = {0};
    if (!prompt("gounfold-path", c, _MAX_PATH)) return;
    struct Node* found = locate(c);
    if (found) {
        cursor = found;
        c_unfold();
    }
}

static void c_promptgofold() {
    char c[_MAX_PATH] = {0};
    if (!prompt("gofold-path", c, _MAX_PATH)) return;
    struct Node* found = locate(c);
    if (found) {
        cursor = found;
        c_fold();
    }
}

// REM: please remember to `LC_ALL=C sort`
static void (* command_map[128])() = {
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
