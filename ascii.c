#include "./treest.h"

/// color-less ASCII printer

static struct {
    unsigned depth;
    unsigned indents;
} state;

const static char* const INDENT      = "|   ";
const static char* const INDENT_LAST = "    ";
const static char* const BRANCH      = "|-- ";
const static char* const BRANCH_LAST = "`-- ";
#define TOKEN_SIZE 4

static struct {
    bool classify;
} flags;

#define TOGGLE(flag) flag = !(flag)

void ascii_toggle(struct Printer* self, char flag) {
    switch (flag) {
        case 'F': TOGGLE(flags.classify); break;
        default: toggle_gflag(flag);
    }
}

void ascii_begin(struct Printer* self) {
    state.depth = 0;
    state.indents = 0;
}

void ascii_end(struct Printer* self) {
}

void ascii_node(struct Printer* self, struct Node* node, size_t index, size_t count) {
    for (int k = state.depth-1; -1 < k; k--)
        write(STDOUT_FILENO, state.indents & (1<<k) ? INDENT_LAST : INDENT, TOKEN_SIZE);
    write(STDOUT_FILENO, count-1 == index ? BRANCH_LAST : BRANCH, TOKEN_SIZE);

show_name: // when a link, jumps back here with node moved
    write(STDOUT_FILENO, node->name, strlen(node->name));

    if (flags.classify) {
        switch (node->type) {
            case Type_LNK:
                write(STDOUT_FILENO, "@ -> ", 4);
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

void ascii_enter(struct Printer* self, struct Node* node, size_t index, size_t count) {
    state.depth++;
    state.indents = state.indents << 1 | (count-1 == index);
}

void ascii_leave(struct Printer* self, struct Node* node, size_t index, size_t count) {
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
