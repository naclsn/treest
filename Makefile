PREFIX    = /usr/local
MANPREFIX = $(PREFIX)/share/man
DESTDIR   = 

CFLAGS += -Wall -Wextra -Werror -pedantic -O2

PROG = treest
SRCS = $(wildcard *.[ch])

all: $(PROG)

$(PROG): $(SRCS)
	$(CC) -o $@ $^ $(CFLAGS) $(LDFLAGS)

install: all
	mkdir -p $(DESTDIR)$(PREFIX)/bin
	cp -f $(PROG) $(DESTDIR)$(PREFIX)/bin
	chmod 755 $(DESTDIR)$(PREFIX)/bin/$(PROG)
	mkdir -p $(DESTDIR)$(MANPREFIX)/man1
	chmod 644 $(DESTDIR)$(MANPREFIX)/man1/$(PROG).1

uninstall:
	rm -f $(DESTDIR)$(PREFIX)/bin/$(PROG) $(DESTDIR)$(MANPREFIX)/man1/$(PROG).1

clean:
	rm $(PROG)

.PHONY: all install uninstall clean
