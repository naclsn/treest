#include "./treest.h"

/// fancy terminal printer

#ifdef FEAT_GIT2
#include <git2.h>
#endif

#define _CL "\x1b[H\x1b[2J\x1b[3J\x1b[?25l"
#define _LC "\x1b[?25h"

#define _SP "\xc2\xa0"
#define _HZ "\xe2\x94\x80"
#define _VE "\xe2\x94\x82"
#define _AN "\xe2\x94\x94"
#define _TE "\xe2\x94\x9c"

#define _UP "\xe2\x94\x86" //"\xe2\x86\x91"
#define _DW "\xe2\x94\x86" //"\xe2\x86\x93"
#define _OV "\xe2\x80\xa6" // OVER_OFFSCRN

static const char* const INDENT      = _VE _SP _SP " ";
static const char* const INDENT_LAST = _SP _SP _SP " ";
static const char* const BRANCH      = _TE _HZ _HZ " ";
static const char* const BRANCH_LAST = _AN _HZ _HZ " ";
static const char* const TOP_OFFSCRN = _UP _SP _SP " ";
static const char* const BOT_OFFSCRN = _DW _SP _SP " ";

static void read_ls_colors(void);
static void apply_ls_colors(struct Node* node);
static void apply_decorations(struct Node* node);

#define TOGGLE(flag) flag = !(flag)

#undef CTRL
#define CTRL(x) ( (~x&64) | (~x&64)>>1 | (x&31) )

static struct {
    unsigned depth;
    unsigned indents;
    struct {
        char* LS_COLORS;
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
        char* sel; // selected (cursor)
        struct LS_COLORS_KVEntry {
            char* key;
            char* val;
        }** ext, ** exa;
        size_t ext_count; // globs on file extension (eg. '*.tar=xyz:')
        size_t exa_count; // matches on exact file name (eg. 'Makefile=xyz:')
    } ls_colors;
    #ifdef FEAT_GIT2
    git_repository* repo;
    #endif
    struct winsize winsize;
    unsigned wintop;
    unsigned wincurr;
    bool next_is_first_onscreen;
    struct Command overriden[7];
    int line_len;
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
        .or="40;31;09",
        // .mi="22;39;49",
        .ex="01;32",
        .sel="7",
    }
};

static struct {
    bool classify;
    bool colors;
    bool join;
} flags;

static void putstr(const char* c, bool visible) {
    if (!visible) { fputs(c, stdout); return; }
    if (state.winsize.ws_col == state.line_len) return;
    const char* h = c;
    size_t count = 0;
    while (*h) count += (*h++ & 0xC0) != 0x80;
    if (state.winsize.ws_col <= state.line_len+count) {
        state.line_len = state.winsize.ws_col;
        fputs(_OV, stdout);
        return;
    }
    state.line_len+= count;
    fputs(c, stdout);
}
static void putln() {
    state.line_len = 0;
    fputs(is_tty ? "\r\n" : "\n", stdout);
}
static void flush() {
    fflush(stdout);
}

static bool _c_z1down(void) {
    if (state.wintop < state.winsize.ws_row) state.wintop++;
    return true;
}
static bool _c_z1up(void) {
    if (0 < state.wintop) state.wintop--;
    return true;
}
static bool _c_zdown(void) {
    if (state.wintop+state.winsize.ws_row/2 < state.winsize.ws_row) state.wintop+= state.winsize.ws_row/2;
    else state.wintop = state.winsize.ws_row;
    return true;
}
static bool _c_zup(void) {
    if (0 < (int)state.wintop-state.winsize.ws_row/2) state.wintop-= state.winsize.ws_row/2;
    else state.wintop = 0;
    return true;
}
static bool _c_zforward(void) {
    if (state.wintop+state.winsize.ws_row < state.winsize.ws_row) state.wintop+= state.winsize.ws_row;
    else state.wintop = state.winsize.ws_row;
    return true;
}
static bool _c_zbackward(void) {
    if (0 < (int)state.wintop-state.winsize.ws_row) state.wintop-= state.winsize.ws_row;
    else state.wintop = 0;
    return true;
}
static bool _c_refresh(void) {
    if (ioctl(STDOUT_FILENO, TIOCGWINSZ, &state.winsize) < 0) {
        state.winsize.ws_col = USHRT_MAX;
        state.winsize.ws_row = USHRT_MAX;
    }
    return state.overriden[6].f();
}

void fancy_init(void) {
    read_ls_colors();

    #ifdef FEAT_GIT2
    git_libgit2_init();
    if (git_repository_open_ext(&state.repo, cwd, GIT_REPOSITORY_OPEN_NO_SEARCH, NULL) < 0)
        state.repo = NULL;
    #endif

    if (ioctl(STDOUT_FILENO, TIOCGWINSZ, &state.winsize) < 0) {
        state.winsize.ws_col = USHRT_MAX;
        state.winsize.ws_row = USHRT_MAX;
    }

    state.overriden[0] = command_map[CTRL('E')];
    state.overriden[1] = command_map[CTRL('Y')];
    state.overriden[2] = command_map[CTRL('D')];
    state.overriden[3] = command_map[CTRL('U')];
    state.overriden[4] = command_map[CTRL('F')];
    state.overriden[5] = command_map[CTRL('B')];
    state.overriden[6] = command_map[CTRL('L')];
    command_map[CTRL('E')] = (struct Command){_c_z1down,    "forward one line"};
    command_map[CTRL('Y')] = (struct Command){_c_z1up,      "backward one line"};
    command_map[CTRL('D')] = (struct Command){_c_zdown,     "forward one half-window"};
    command_map[CTRL('U')] = (struct Command){_c_zup,       "backward one half-window"};
    command_map[CTRL('F')] = (struct Command){_c_zforward,  "forward one window"};
    command_map[CTRL('B')] = (struct Command){_c_zbackward, "backward one window"};
    command_map[CTRL('L')] = (struct Command){_c_refresh,   command_map[CTRL('L')].h};
}

void fancy_del(void) {
    for (size_t k = 0; k < state.ls_colors.ext_count; k++) free(state.ls_colors.ext[k]);
    for (size_t k = 0; k < state.ls_colors.exa_count; k++) free(state.ls_colors.exa[k]);
    free(state.ls_colors.ext);
    free(state.ls_colors.exa);
    free(state.ls_colors.LS_COLORS);

    #ifdef FEAT_GIT2
    git_repository_free(state.repo);
    git_libgit2_shutdown();
    #endif

    command_map[CTRL('E')] = state.overriden[0];
    command_map[CTRL('Y')] = state.overriden[1];
    command_map[CTRL('D')] = state.overriden[2];
    command_map[CTRL('U')] = state.overriden[3];
    command_map[CTRL('F')] = state.overriden[4];
    command_map[CTRL('B')] = state.overriden[5];
    command_map[CTRL('L')] = state.overriden[6];
}

bool fancy_toggle(char flag) {
    switch (flag) {
        case 'F': TOGGLE(flags.classify); return true;
        case 'c': TOGGLE(flags.colors);   return true;
        case 'j': TOGGLE(flags.join);     return true;
    }
    return toggle_gflag(flag);
}

bool fancy_command(const char* _UNUSED(c)) {
    return false;
}

bool fancy_filter(struct Node* node) {
    #ifdef FEAT_GIT2
    if (state.repo) {
        int r;
        git_ignore_path_is_ignored(&r, state.repo, node->path);
        return r;
    }
    #endif
    return node_ignore(node);
}

void fancy_begin(void) {
    state.depth = -1;
    state.indents = 0;
    putstr(_CL, false);
    state.wincurr = 0;
    state.next_is_first_onscreen = false;
    state.line_len = 0;
}

void fancy_end(void) {
    putstr(_LC, false);
    flush();
}

void fancy_node(struct Node* node) {
    state.wincurr++;
    if (0 != state.wintop && state.wincurr == state.wintop+1) {
        state.next_is_first_onscreen = true;
        return;
    }
    if (state.winsize.ws_row+state.wintop == state.wincurr+1) {
        for (int k = state.depth-1; -1 < k; k--)
            putstr(state.indents & (1<<k) ? INDENT_LAST : BOT_OFFSCRN, true);
        putstr(BOT_OFFSCRN, true);
        putln();
        return;
    }
    if (state.wincurr <= state.wintop || state.winsize.ws_row+state.wintop < state.wincurr+1)
        return;

    if (state.next_is_first_onscreen) {
        for (int k = state.depth-1; -1 < k; k--)
            putstr(state.indents & (1<<k) ? INDENT_LAST : TOP_OFFSCRN, true);
        putstr(TOP_OFFSCRN, true);
        putln();
        state.next_is_first_onscreen = false;
    }

    if (&root != node && !(flags.join && 1 == node->parent->count)) {
        for (int k = state.depth-1; -1 < k; k--)
            putstr(state.indents & (1<<k) ? INDENT_LAST : INDENT, true);
        putstr(((node->parent ? node->parent->count : 1)-1 == node->index) ? BRANCH_LAST : BRANCH, true);
    }

    if (flags.colors)
        apply_ls_colors(node);

    if (node == cursor) {
        if (flags.colors) {
            putstr("\x1b[", false);
            putstr(state.ls_colors.sel, false);
            putstr("m", false);
        } else putstr("> ", true);
    }

    putstr(node->name, true);

    if (flags.colors) {
        putstr("\x1b[", false);
        putstr(state.ls_colors.rs, false);
        putstr("m", false);
    }

    if (flags.classify)
        apply_decorations(node);

    if (flags.join && 1 == node->count && node->as.dir.unfolded) {
        if (!flags.classify) putstr("/", true);
        state.wincurr--;
    } else putln();
}

void fancy_enter(struct Node* node) {
    state.depth++;
    state.indents = state.indents << 1 | ((node->parent ? node->parent->count : 1)-1 == node->index);
}

void fancy_leave(struct Node* _UNUSED(node)) {
    state.depth--;
    state.indents = state.indents >> 1;
}

static void _sorted_insert(struct LS_COLORS_KVEntry* entry, struct LS_COLORS_KVEntry*** into, size_t* count, size_t* cap) {
    if (0 == *count) {
        *cap = 8;
        may_malloc(*into, *cap * sizeof(struct LS_COLORS_KVEntry*));
    } else if (*cap <= *count) {
        *cap*= 2;
        may_realloc(*into, *cap * sizeof(struct LS_COLORS_KVEntry*));
    }

    size_t k = 0;
    for (k = *count; 0 < k; k--) {
        if (strcmp((*into)[k-1]->key, entry->key) < 0) break;
        (*into)[k] = (*into)[k-1];
    }
    (*into)[k] = entry;
    (*count)++;
}

static void read_ls_colors(void) {
    if (state.ls_colors.LS_COLORS) return;

    char* tail = getenv("LS_COLORS");
    if (!tail) return;

    size_t ext_cap = 0;
    size_t exa_cap = 0;

    may_strdup(state.ls_colors.LS_COLORS, tail);
    tail = state.ls_colors.LS_COLORS;
    char* head;
    while ((head = strchr(tail, ':'))) {
        *head = '\0';
        char* val = strchr(tail, '=');
        if (!val) continue;
        *val++ = '\0';

        if (2+1 == val-tail) {
            if      (0 == strcmp("rs", tail)) state.ls_colors.rs = val;
            else if (0 == strcmp("di", tail)) state.ls_colors.di = val;
            else if (0 == strcmp("fi", tail)) state.ls_colors.fi = val;
            else if (0 == strcmp("ln", tail)) state.ls_colors.ln = val;
            else if (0 == strcmp("pi", tail)) state.ls_colors.pi = val;
            else if (0 == strcmp("so", tail)) state.ls_colors.so = val;
            else if (0 == strcmp("bd", tail)) state.ls_colors.bd = val;
            else if (0 == strcmp("cd", tail)) state.ls_colors.cd = val;
            else if (0 == strcmp("or", tail)) state.ls_colors.or = val;
            // else if (0 == strcmp("mi", tail)) state.ls_colors.mi = val;
            else if (0 == strcmp("ex", tail)) state.ls_colors.ex = val;
        } else if (0 == strcmp("sel", tail)) state.ls_colors.sel = val;

        else if ('*' == tail[0] && '.' == tail[1]) {
            tail+= 2;
            struct LS_COLORS_KVEntry* niw; may_malloc(niw, sizeof(struct LS_COLORS_KVEntry));
            niw->key = tail;
            niw->val = val;
            _sorted_insert(niw, &state.ls_colors.ext, &state.ls_colors.ext_count, &ext_cap);
        }

        else {
            if ('*' == *tail) tail++;
            struct LS_COLORS_KVEntry* niw; may_malloc(niw, sizeof(struct LS_COLORS_KVEntry));
            niw->key = tail;
            niw->val = val;
            _sorted_insert(niw, &state.ls_colors.exa, &state.ls_colors.exa_count, &exa_cap);
        }

        tail = head+1;
    }

    may_realloc(state.ls_colors.ext, state.ls_colors.ext_count * sizeof(struct LS_COLORS_KVEntry));
    may_realloc(state.ls_colors.exa, state.ls_colors.exa_count * sizeof(struct LS_COLORS_KVEntry));
}

static struct LS_COLORS_KVEntry* _binary_search(const char* needle, struct LS_COLORS_KVEntry** hay, size_t a, size_t b) {
    size_t c = (a + b) / 2;
    int cmp = strcmp(needle, hay[c]->key);

    if (0 == cmp) return hay[c];

    if (a == b) return NULL;
    if (a+1 == b) return 0 == strcmp(needle, hay[c+1]->key) ? hay[c+1] : NULL;

    if (cmp < 0) return _binary_search(needle, hay, a, c-1);
    if (0 < cmp) return _binary_search(needle, hay, c+1, b);

    return NULL;
}

static void apply_ls_colors(struct Node* node) {
    char* col = NULL;

    if (0 < state.ls_colors.exa_count) {
        struct LS_COLORS_KVEntry* found = _binary_search(node->name, state.ls_colors.exa, 0, state.ls_colors.exa_count-1);
        if (found) col = found->val;
    }

    if (!col && 0 < state.ls_colors.ext_count) {
        char* ext = node->name;
        while ((ext = strchr(ext, '.'))) {
            ext++;
            struct LS_COLORS_KVEntry* found = _binary_search(ext, state.ls_colors.ext, 0, state.ls_colors.ext_count-1);
            if (found) {
                col = found->val;
                break;
            }
        }
    }

    if (!col) {
    fill_color: // if the LC_COLOR for ln is "target", jumps back here with node moved
        switch (node->type) {
            case Type_DIR:  col = state.ls_colors.di; break;
            case Type_REG:  col = state.ls_colors.fi; break;
            case Type_LNK:
                if (node->as.link.tail) {
                    if (0 == strcmp("target", state.ls_colors.ln)) {
                        node = node->as.link.tail;
                        goto fill_color;
                    } else col = state.ls_colors.ln;
                } else col = state.ls_colors.or;
                break;
            case Type_FIFO: col = state.ls_colors.pi; break;
            case Type_SOCK: col = state.ls_colors.so; break;
            case Type_BLK:  col = state.ls_colors.bd; break;
            case Type_CHR:  col = state.ls_colors.cd; break;
            case Type_EXEC: col = state.ls_colors.ex; break;
            default:        col = "09;31";            break;
        }
    }

    if (col) {
        putstr("\x1b[", false);
        putstr(col, false);
        putstr("m", false);
    }
}

static void apply_decorations(struct Node* node) {
    switch (node->type) {
        case Type_LNK:
            putstr("@ -> ", true);
            if (node->as.link.tail) {
                if (flags.colors)
                    apply_ls_colors(node->as.link.tail);

                putstr(node->as.link.tail->name, true);

                if (flags.colors) {
                    putstr("\x1b[", false);
                    putstr(state.ls_colors.rs, false);
                    putstr("m", false);
                }

                if (flags.classify)
                    apply_decorations(node->as.link.tail);
            } else putstr(node->as.link.readpath, true);
            break;

        case Type_DIR:
            putstr("/", true);
            if (node->as.dir.unfolded && 0 == node->count)
                putstr(" (/)", true);
            break;

        case Type_FIFO:
            putstr("|", true);
            break;

        case Type_SOCK:
            putstr("=", true);
            break;

        case Type_EXEC:
            putstr("*", true);
            break;

        default:
            break;
    }
}

struct Printer fancy_printer = {
    .name="fancy",
    .init=fancy_init,
    .del=fancy_del,
    .toggle=fancy_toggle,
    .command=fancy_command,
    .filter=fancy_filter,
    .begin=fancy_begin,
    .end=fancy_end,
    .node=fancy_node,
    .enter=fancy_enter,
    .leave=fancy_leave,
};
