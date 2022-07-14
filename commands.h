#ifdef TREEST_COMMAND
#error This file should not be included outside treest.c
#endif // TREEST_COMMAND
#define TREEST_COMMAND(x) do_command(x)

#include "./treest.h"

void toggle_gflag(char flag) {
    switch (flag) {
        case 'a': break; // Filter entries starting with '.'
    }
}

static void do_command(char x) {
    switch (x) {
        case 'C': break; // (prompt) go to and fold by path
        case 'O': break; // (prompt) go to and unfold by path
        case 'o': break; // (prompt) unfold by path
        case 'c': break; // (prompt) fold by path

        case 'k': break; // previous
        case 'j': break; // next
        case 'l': break; // unfold
        case 'h': break; // fold

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
