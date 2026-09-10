# The handheld has no compiler and no headers, so everything is cross-built.
# Headers come from the host's libwayland-dev (they carry no architecture), and
# the library itself is pulled off the device by `make sync-libs`.
CROSS   ?= aarch64-linux-gnu-
CC      := $(CROSS)gcc
HOST    ?= root@192.168.8.154

SCANNER := wayland-scanner
PROTO   := protocol/wlr-layer-shell-unstable-v1.xml
# layer-shell references xdg_popup, so its glue has to be linked in as well.
XDG     := /usr/share/wayland-protocols/stable/xdg-shell/xdg-shell.xml
GEN     := wlr-layer-shell-unstable-v1-client-protocol.h \
           wlr-layer-shell-unstable-v1-protocol.c \
           xdg-shell-protocol.c
SRC     := gpkbd.c wlr-layer-shell-unstable-v1-protocol.c xdg-shell-protocol.c

CFLAGS  := -std=c11 -O2 -Wall -Wextra -I.
LDFLAGS := -L.sysroot -Wl,-rpath-link,.sysroot
LDLIBS  := -l:libwayland-client.so.0

all: gpkbd

gpkbd: $(SRC) $(GEN) | .sysroot
	$(CC) $(CFLAGS) -o $@ $(SRC) $(LDFLAGS) $(LDLIBS)

wlr-layer-shell-unstable-v1-client-protocol.h: $(PROTO)
	$(SCANNER) client-header $< $@

wlr-layer-shell-unstable-v1-protocol.c: $(PROTO)
	$(SCANNER) private-code $< $@

xdg-shell-protocol.c: $(XDG)
	$(SCANNER) private-code $< $@

# libwayland-client drags in libffi and libm at link time; take them from the
# device so the versions match what gpkbd will actually run against.
.sysroot:
	mkdir -p $@
	scp $(HOST):/usr/lib/libwayland-client.so.0 $@/
	scp $(HOST):/usr/lib/libffi.so.8 $@/ 2>/dev/null || true

sync-libs:
	rm -rf .sysroot
	$(MAKE) .sysroot

# The old binary may be running, and a busy executable cannot be written over.
install: gpkbd
	-ssh $(HOST) 'kill $$(pidof gpkbd) 2>/dev/null; sleep 1'
	scp gpkbd $(HOST):/storage/.local/bin/gpkbd

clean:
	rm -f gpkbd $(GEN)

distclean: clean
	rm -rf .sysroot

.PHONY: all clean distclean install sync-libs
