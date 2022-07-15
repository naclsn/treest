#include "./treest.h"

/// (TODO) fancy terminal printer

void fancy_toggle(char flag) {
    fprintf(stderr, "fancy_toggle('%c')\n", flag);
}

void fancy_begin() {
    fprintf(stderr, "fancy_begin()\n");
}

void fancy_end() {
    fprintf(stderr, "fancy_end()\n");
}

void fancy_node(struct Node* node, size_t index, size_t count) {
    fprintf(stderr, "fancy_node(<%s>, %zu, %zu)\n", node->path, index, count);
}

void fancy_enter(struct Node* node, size_t index, size_t count) {
    fprintf(stderr, "fancy_enter(<%s>, %zu, %zu)\n", node->path, index, count);
}

void fancy_leave(struct Node* node, size_t index, size_t count) {
    fprintf(stderr, "fancy_leave(<%s>, %zu, %zu)\n", node->path, index, count);
}

struct Printer fancy_printer = {
    .toggle=fancy_toggle,
    .begin=fancy_begin,
    .end=fancy_end,
    .node=fancy_node,
    .enter=fancy_enter,
    .leave=fancy_leave,
};
