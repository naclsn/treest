PREFIX    ?= /usr/local
MANPREFIX ?= $(PREFIX)/share/man
DESTDIR   ?= 

ifneq (, $(shell ldconfig -p | grep readline))
FEAT += -DFEAT_READLINE -lreadline
endif
ifneq (, $(shell ldconfig -p | grep git2))
FEAT += -DFEAT_GIT2 -lgit2
endif
ifneq (, $(DEBUGGING))
FEAT += -DTRACE_ALLOCS -g
else
FEAT += -s
endif

CFLAGS ?= -std=c99 -Wall -Wextra -Werror -pedantic -O2

VERSION = 0.0.7
PROG = treest
SRCS = $(wildcard *.[ch])

all: $(PROG)

$(PROG): $(SRCS)
	$(CC) -o $@ $^ -DTREEST_VERSION='"$(VERSION)"' $(CFLAGS) $(FEAT)

install: all
	mkdir -p $(DESTDIR)$(PREFIX)/bin
	cp -f $(PROG) $(DESTDIR)$(PREFIX)/bin
	chmod 755 $(DESTDIR)$(PREFIX)/bin/$(PROG)
	mkdir -p $(DESTDIR)$(MANPREFIX)/man1
	sed s/TREEST_VERSION/$(VERSION)/ $(PROG).1 >$(DESTDIR)$(MANPREFIX)/man1/$(PROG).1
	chmod 644 $(DESTDIR)$(MANPREFIX)/man1/$(PROG).1

uninstall:
	$(RM) $(DESTDIR)$(PREFIX)/bin/$(PROG) $(DESTDIR)$(MANPREFIX)/man1/$(PROG).1

clean:
	$(RM) $(PROG)

.PHONY: all install uninstall clean
