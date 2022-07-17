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

static void (* command_map[128])();

void toggle_gflag(char flag) {
    switch (flag) {
        case 'a': break; // Filter entries starting with '.'
    }
}

static void c_quit() {
    exit(0);
}

static void c_cquit() {
    exit(1);
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

static void (* command_map[128])() = {
    [  3]=c_quit,         // ^C (ETX)
    [  4]=c_quit,         // ^D (EOT)
    ['h']=c_parent,
    ['j']=c_next,
    ['k']=c_previous,
    ['l']=c_child,
    ['Q']=c_cquit,
    ['q']=c_quit,
};

/*
static void do_command(char x) {
    switch (x) {
        case 'o': break; // (prompt) unfold by path
        case 'c': break; // (prompt) fold by path
        case 'O': break; // (prompt) go to and unfold by path
        case 'C': break; // (prompt) go to and fold by path

        case 'k': break; // go to previous
        case 'j': break; // go to next
        case 'l': break; // unfold and go to 1st child if any
        case 'h': break; // go to parent
        case 'L': break; // unfold
        case 'H': break; // fold

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
