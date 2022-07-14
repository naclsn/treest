#include "./treest.h"

/// (TODO) fancy terminal printer

void fancy_toggle(struct Printer* self, char flag) {
    write(STDERR_FILENO, "fancy_toggle\n", 13);
}

void fancy_begin(struct Printer* self) {
    write(STDERR_FILENO, "fancy_begin\n", 12);
}

void fancy_end(struct Printer* self) {
    write(STDERR_FILENO, "fancy_end\n", 10);
}

void fancy_node(struct Printer* self, struct Node* node, size_t index, size_t count) {
    write(STDERR_FILENO, "fancy_node\n", 11);
}

void fancy_enter(struct Printer* self, struct Node* node, size_t index, size_t count) {
    write(STDERR_FILENO, "fancy_enter\n", 12);
}

void fancy_leave(struct Printer* self, struct Node* node, size_t index, size_t count) {
    write(STDERR_FILENO, "fancy_leave\n", 12);
}

struct Printer fancy_printer = {
    .toggle=fancy_toggle,
    .begin=fancy_begin,
    .end=fancy_end,
    .node=fancy_node,
    .enter=fancy_enter,
    .leave=fancy_leave,
};
