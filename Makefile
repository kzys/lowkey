# Cross-linking is configured in .cargo/config.toml.
HOST   ?= root@192.168.8.154
TARGET := aarch64-unknown-linux-gnu
BIN    := target/$(TARGET)/release/gpkbd

.PHONY: all install clean

all:
	cargo build --release --target $(TARGET)

# The old binary may be running, and a busy executable cannot be written over.
install: all
	-ssh $(HOST) 'kill $$(pidof gpkbd) 2>/dev/null; sleep 1'
	scp $(BIN) $(HOST):/storage/.local/bin/gpkbd

clean:
	cargo clean
