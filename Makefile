# Cross-linking is configured in .cargo/config.toml.
TARGET := aarch64-unknown-linux-gnu
BIN    := target/$(TARGET)/release/lowkey

.PHONY: all install install-config clean

all:
	cargo build --release --target $(TARGET)

# The old binary may be running, and a busy executable cannot be written over.
install: all
	@test -n "$(HOST)" || { echo "HOST is required, e.g. make install HOST=root@192.0.2.1" >&2; exit 1; }
	-ssh $(HOST) 'kill $$(pidof lowkey) 2>/dev/null; sleep 1'
	scp $(BIN) $(HOST):/storage/.local/bin/lowkey

# foot.ini and the Ports launcher live here because lowkey's own behavior
# (bare Page_Up/Page_Down, its font flag, its overlay height) is what drives
# their content.
install-config:
	@test -n "$(HOST)" || { echo "HOST is required, e.g. make install-config HOST=root@192.0.2.1" >&2; exit 1; }
	scp rocknix/foot.ini $(HOST):/storage/.config/foot/foot.ini
	scp rocknix/Terminal.sh $(HOST):/storage/roms/ports/Terminal.sh
	ssh $(HOST) 'chmod +x /storage/roms/ports/Terminal.sh'

clean:
	cargo clean
