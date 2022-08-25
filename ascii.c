#include "./treest.h"

/// color-less ASCII printer

static const char* const INDENT      = "|   ";
static const char* const INDENT_LAST = "    ";
static const char* const BRANCH      = "|-- ";
static const char* const BRANCH_LAST = "`-- ";
#define TOKEN_SIZE 4

#define TOGGLE(flag) flag = !(flag)

#define putstr(__c) { if (write(STDOUT_FILENO, __c, strlen(__c)) < 0) die("write"); }

static struct {
    unsigned depth;
    unsigned indents;
} state;

static struct {
    bool classify;
    bool relative;
    bool index;
    bool link_dir;
} flags;

bool ascii_toggle(char flag) {
    switch (flag) {
        case 'F': TOGGLE(flags.classify); return true;
        case 'P': TOGGLE(flags.relative); return true;
        case 'i': TOGGLE(flags.index);    return true;
        case 'l': TOGGLE(flags.link_dir); return true;
    }
    return toggle_gflag(flag);
}

void ascii_begin(void) {
    state.depth = -1;
    state.indents = 0;

    if (is_tty) putchar('\r');
    putchar('\n');
}

void ascii_end(void) {
}

void ascii_node(struct Node* node) {
    if (&root != node) {
        for (int k = state.depth-1; -1 < k; k--)
            putstr(state.indents & (1<<k) ? INDENT_LAST : INDENT);
        putstr(((node->parent ? node->parent->count : 1)-1 == node->index) ? BRANCH_LAST : BRANCH);
    }

    if (node == cursor) putstr("> ");

    if (flags.index) {
        char buf[16];
        sprintf(buf, "[%2zu] ", node->index);
        putstr(buf);
    }

    size_t cwd_len = 0;
    if (flags.relative) cwd_len = strlen(cwd);
show_name: // when decorating a link, jumps back here with node moved
    if (flags.relative) {
        char* rel = '/' != memcmp(node->path, cwd, cwd_len+1)
            ? node->path
            : node->path+strlen(cwd)+1;
        putstr(rel);
    } else putstr(node->name);

    if (flags.index && (Type_DIR == node->type || Type_LNK == node->type)) {
        char buf[16];
        sprintf(buf, " [/%zu] ", node->count);
        putstr(buf);
    }

    if (flags.link_dir && Type_DIR == node->type && node->parent) {
        struct Node* parent = Type_DIR == node->parent->type
            ? node->parent
            : node->parent->as.link.tail;
        struct Node* mate = parent->as.dir.children[node->index];
        if (mate != node) {
            char buf[64];
            sprintf(buf, " (%p/%p%c -> %p) ", (void*)parent, (void*)mate,
                  Type_DIR == mate->type ? '/'
                : Type_LNK == mate->type ? '@'
                : '?'
                , (void*)node);
            putstr(buf);
        }
    }

    if (flags.classify) {
        switch (node->type) {
            case Type_LNK:
                putstr("@ -> ");
                if (node->as.link.to) {
                    node = node->as.link.to;
                    goto show_name;
                }
                putstr(node->as.link.readpath);
                break;

            case Type_DIR:
                putchar('/');
                break;

            case Type_FIFO:
                putchar('|');
                break;

            case Type_SOCK:
                putchar('=');
                break;

            case Type_EXEC:
                putchar('*');
                break;

            default:
                break;
        }
    }

    if (is_tty) putchar('\r');
    putchar('\n');
}

void ascii_enter(struct Node* node) {
    state.depth++;
    state.indents = state.indents << 1 | ((node->parent ? node->parent->count : 1)-1 == node->index);
}

void ascii_leave(struct Node* _UNUSED(node)) {
    state.depth--;
    state.indents = state.indents >> 1;
}

struct Printer ascii_printer = {
    .name="ascii",
    .toggle=ascii_toggle,
    .begin=ascii_begin,
    .end=ascii_end,
    .node=ascii_node,
    .enter=ascii_enter,
    .leave=ascii_leave,
};
