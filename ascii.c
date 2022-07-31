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
} flags;

void ascii_init(void) {
}

void ascii_del(void) {
}

bool ascii_toggle(char flag) {
    switch (flag) {
        case 'F': TOGGLE(flags.classify); return true;
    }
    return toggle_gflag(flag);
}

bool ascii_command(const char* _UNUSED(c)) {
    return false;
}

void ascii_begin(void) {
    state.depth = -1;
    state.indents = 0;
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

show_name: // when a link, jumps back here with node moved
    putstr(node->name);

    if (flags.classify) {
        switch (node->type) {
            case Type_LNK:
                putstr("@ -> ");
                node = node->as.link.to;
                goto show_name;

            case Type_DIR:
                putchar('/');
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
    .init=ascii_init,
    .del=ascii_del,
    .toggle=ascii_toggle,
    .command=ascii_command,
    .begin=ascii_begin,
    .end=ascii_end,
    .node=ascii_node,
    .enter=ascii_enter,
    .leave=ascii_leave,
};
