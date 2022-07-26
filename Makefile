PREFIX    ?= /usr/local
MANPREFIX ?= $(PREFIX)/share/man
DESTDIR   ?= 

CFLAGS += -Wall -Wextra -Werror -pedantic -O2
VERSION = 0.0.6

PROG = treest
SRCS = $(wildcard *.[ch])

all: $(PROG)

$(PROG): $(SRCS)
	$(CC) -o $@ $^ -DTREEST_VERSION='"$(VERSION)"' $(CFLAGS)

install: all
	mkdir -p $(DESTDIR)$(PREFIX)/bin
	cp -f $(PROG) $(DESTDIR)$(PREFIX)/bin
	chmod 755 $(DESTDIR)$(PREFIX)/bin/$(PROG)
	mkdir -p $(DESTDIR)$(MANPREFIX)/man1
	sed s/TREEST_VERSION/$(VERSION)/ $(PROG).1 >$(DESTDIR)$(MANPREFIX)/man1/$(PROG).1
	chmod 644 $(DESTDIR)$(MANPREFIX)/man1/$(PROG).1

uninstall:
	rm -f $(DESTDIR)$(PREFIX)/bin/$(PROG) $(DESTDIR)$(MANPREFIX)/man1/$(PROG).1

clean:
	rm -f $(PROG)

.PHONY: all install uninstall clean
