#include "./treest.h"

/// fancy terminal printer

// #define _CL "\e[H\e[2J\e[3J"
#define _CL "\x1b[H\x1b[2J\x1b[3J"

#define _SP "\xc2\xa0"
#define _HZ "\xe2\x94\x80"
#define _VE "\xe2\x94\x82"
#define _AN "\xe2\x94\x94"
#define _TE "\xe2\x94\x9c"

static const char* const INDENT      = _VE _SP _SP " ";
static const char* const INDENT_LAST = _SP _SP _SP " ";
static const char* const BRANCH      = _TE _HZ _HZ " ";
static const char* const BRANCH_LAST = _AN _HZ _HZ " ";

static void read_ls_colors();
static void apply_ls_colors(struct Node* node);
static void apply_decorations(struct Node* node);

#define TOGGLE(flag) flag = !(flag)
// TODO: this is dumb; should use existing buffering
#define putstr(__c) { if (write(STDOUT_FILENO, __c, strlen(__c)) < 0) die("write"); }

static struct {
    unsigned depth;
    unsigned indents;
    struct {
        char* LS_COLORS;
        bool loaded;
        // only the following are handled
        char* rs; // (reset)
        char* di; // directory
        char* fi; // file
        char* ln; // symlink
        char* pi; // FIFO
        char* so; // socket
        char* bd; // block device
        char* cd; // character device
        char* or; // orphan (broken link)
        // char* mi; // missing (??)
        char* ex; // executable
        char* sel; // (added) selected
    } ls_colors;
} state = {
    .ls_colors = {
        .rs="0",
        .di="01;34",
        .fi="22;39;49",
        .ln="01;36",
        .pi="40;33",
        .so="01;35",
        .bd="40;33;01",
        .cd="40;33;01",
        .or="40;31;01",
        // .mi="22;39;49",
        .ex="01;32",
        .sel="7",
    }
};

static struct {
    bool classify;
    bool colors;
} flags;

void fancy_toggle(char flag) {
    switch (flag) {
        case 'F': TOGGLE(flags.classify); break;
        case 'c': TOGGLE(flags.colors); break;
        default: toggle_gflag(flag);
    }
}

void fancy_begin() {
    if (!state.ls_colors.loaded)
        read_ls_colors();
    state.depth = 0;
    state.indents = 0;
    putstr(_CL);
}

void fancy_end() {
}

void fancy_node(struct Node* node) {
    for (int k = state.depth-1; -1 < k; k--)
        putstr(state.indents & (1<<k) ? INDENT_LAST : INDENT);
    putstr(((node->parent ? node->parent->count : 1)-1 == node->index) ? BRANCH_LAST : BRANCH);

    // TODO: idk
    if (node == cursor) {
        if (flags.colors) {
            putstr("\x1b[");
            putstr(state.ls_colors.sel)
            putstr("m");
        } else putstr("> ");
    }

    if (flags.colors)
        apply_ls_colors(node);

    putstr(node->name);

    if (flags.colors) {
        putstr("\x1b[");
        putstr(state.ls_colors.rs)
        putstr("m");
    }

    if (flags.classify)
        apply_decorations(node);

    if (is_tty) putchar('\r');
    putchar('\n');
}

void fancy_enter(struct Node* node) {
    state.depth++;
    state.indents = state.indents << 1 | ((node->parent ? node->parent->count : 1)-1 == node->index);
}

void fancy_leave(struct Node* _UNUSED(node)) {
    state.depth--;
    state.indents = state.indents >> 1;
}

static void read_ls_colors() {
    char* var = getenv("LS_COLORS");
    state.ls_colors.loaded = true;
    if (!var) return;
    var = state.ls_colors.LS_COLORS = strdup(var);
    // TODO: by extension
    char* head;
    while ((head = strchr(var, ':'))) {
        *head = '\0';
        char* eq = strchr(head, '=');
        if (2 == eq++-head) {
                 if (0 == strcmp("rs", head)) state.ls_colors.rs = eq;
            else if (0 == strcmp("di", head)) state.ls_colors.di = eq;
            else if (0 == strcmp("fi", head)) state.ls_colors.fi = eq;
            else if (0 == strcmp("ln", head)) state.ls_colors.ln = eq;
            else if (0 == strcmp("pi", head)) state.ls_colors.pi = eq;
            else if (0 == strcmp("so", head)) state.ls_colors.so = eq;
            else if (0 == strcmp("bd", head)) state.ls_colors.bd = eq;
            else if (0 == strcmp("cd", head)) state.ls_colors.cd = eq;
            else if (0 == strcmp("or", head)) state.ls_colors.or = eq;
            // else if (0 == strcmp("mi", head)) state.ls_colors.mi = eq;
            else if (0 == strcmp("ex", head)) state.ls_colors.ex = eq;
        }
        var = head+1;
    }
}

static void apply_ls_colors(struct Node* node) {
    char* col;
    // TODO: by extension
    switch (node->type) {
        case Type_DIR:  col = state.ls_colors.di; break;
        case Type_REG:  col = state.ls_colors.fi; break;
        case Type_LNK:  col = node->as.link.to // XXX
                                ? state.ls_colors.ln
                                : state.ls_colors.or;
                            break;
        case Type_FIFO: col = state.ls_colors.pi; break;
        case Type_SOCK: col = state.ls_colors.so; break;
        case Type_BLK:  col = state.ls_colors.bd; break;
        case Type_CHR:  col = state.ls_colors.cd; break;
        case Type_EXEC: col = state.ls_colors.ex; break;
        default:        col = "";                 break;
    }
    putstr("\x1b[");
    putstr(col);
    putstr("m");
}

static void apply_decorations(struct Node* node) {
    switch (node->type) {
        case Type_LNK:
            putstr("@ -> ");
            putstr(node->as.link.to->name);
            break;

        case Type_DIR:
            putstr("/");
            if (node->as.dir.unfolded && 0 == node->count)
                putstr(" (/)");
            break;

        case Type_EXEC:
            putstr("*");
            break;

        default:
            break;
    }
}

struct Printer fancy_printer = {
    .toggle=fancy_toggle,
    .begin=fancy_begin,
    .end=fancy_end,
    .node=fancy_node,
    .enter=fancy_enter,
    .leave=fancy_leave,
};
