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

void ascii_toggle(char flag) {
    switch (flag) {
        case 'F': TOGGLE(flags.classify); break;
        default: toggle_gflag(flag);
    }
}

void ascii_begin() {
    state.depth = 0;
    state.indents = 0;
}

void ascii_end() {
}

void ascii_node(struct Node* node, size_t index, size_t count) {
    for (int k = state.depth-1; -1 < k; k--)
        putstr(state.indents & (1<<k) ? INDENT_LAST : INDENT);
    putstr(count-1 == index ? BRANCH_LAST : BRANCH);

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

    putchar('\r');
    putchar('\n');
}

void ascii_enter(struct Node* _UNUSED(node), size_t index, size_t count) {
    state.depth++;
    state.indents = state.indents << 1 | (count-1 == index);
}

void ascii_leave(struct Node* _UNUSED(node), size_t _UNUSED(index), size_t _UNUSED(count)) {
    state.depth--;
    state.indents = state.indents >> 1;
}

struct Printer ascii_printer = {
    .toggle=ascii_toggle,
    .begin=ascii_begin,
    .end=ascii_end,
    .node=ascii_node,
    .enter=ascii_enter,
    .leave=ascii_leave,
};
