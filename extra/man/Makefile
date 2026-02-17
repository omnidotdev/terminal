# Man page generation
.PHONY: all clean install

SCDOC = scdoc
MANDIR = /usr/local/share/man

all: omni-terminal.1 omni-terminal.5 omni-terminal-bindings.5

omni-terminal.1: omni-terminal.1.scd
	$(SCDOC) < $< > $@

omni-terminal.5: omni-terminal.5.scd
	$(SCDOC) < $< > $@

omni-terminal-bindings.5: omni-terminal-bindings.5.scd
	$(SCDOC) < $< > $@

install: all
	install -Dm644 omni-terminal.1 $(DESTDIR)$(MANDIR)/man1/omni-terminal.1
	install -Dm644 omni-terminal.5 $(DESTDIR)$(MANDIR)/man5/omni-terminal.5
	install -Dm644 omni-terminal-bindings.5 $(DESTDIR)$(MANDIR)/man5/omni-terminal-bindings.5

clean:
	rm -f omni-terminal.1 omni-terminal.5 omni-terminal-bindings.5
