# gpkbd

An on-screen keyboard for typing with a gamepad.

It draws a grid of keys on a layer-shell surface that never takes keyboard
focus, reads the pad straight from evdev, and types through a uinput device.
Keystrokes land in whatever the compositor has focused, so no pointer, no
compositor-specific IPC, and no input-method support is needed. Built for
ROCKNIX handhelds running sway, where a controller is the only input device.

## Status

Working:

- 10x5 QWERTY-ish grid, D-pad/hat navigation, wraps at the edges and
  auto-repeats on a held direction.
- `BTN_EAST` inputs the selected key, `BTN_SOUTH` is backspace, `BTN_WEST`
  is space, `BTN_NORTH` and Start both enter, Select quits. (On this
  device's Nintendo-style layout, `BTN_SOUTH`/`BTN_EAST`/`BTN_WEST`/
  `BTN_NORTH` are physically labeled B/A/Y/X, matching the B-is-back
  convention.)
- Shift and Ctrl are held modifiers, like a real keyboard: either shoulder
  button on the left (L1/L2) holds Shift, R1 holds Ctrl, both for as long as
  they're physically held. Shifted symbol keys show their shifted glyph
  live while held.
- R2 is a held chord: while it's down, D-pad/hat up and down send
  PageUp/PageDown (auto-repeating) instead of moving the grid selection.
- A legend line above the grid spells out what every button currently does.
- Bottom-anchored overlay sized with `-h`; pad picked by name with `-p` or
  auto-detected as the first device with a `BTN_SOUTH`.
- Key/legend text is rendered with `fontdue` from a TTF/OTF given with `-f`
  or `$GPKBD_FONT` (default `/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf`,
  which doesn't exist on ROCKNIX — see `rocknix/Terminal.sh`).
- `--print-height` prints the effective height and exits, so a launcher
  script can reserve that much screen space without hardcoding it.
- Cross-builds for aarch64, with no runtime dependency on the device's
  libwayland: `wayland-client`'s pure-Rust backend speaks the wire protocol
  directly, so there's nothing to pull off the device before linking.

## Build

```
make                # cross-builds target/aarch64-unknown-linux-gnu/release/gpkbd
make install        # scp to the device, killing any running instance first
make install-config # scp rocknix/foot.ini and rocknix/Terminal.sh into place
```

`HOST` in the Makefile can be overridden on the command line. The target
needs `rustup target add aarch64-unknown-linux-gnu` and an
`aarch64-linux-gnu-gcc` on `PATH` for linking (configured in
`.cargo/config.toml`).

## rocknix/

`foot.ini` and `Terminal.sh` (the EmulationStation Ports launcher that opens
a shell with gpkbd's overlay) live here because gpkbd's own behavior drives
their content directly: `foot.ini`'s `scrollback-up-page`/`down-page` bind to
bare `Page_Up`/`Page_Down` because that's what gpkbd's R2 chord sends, and
`Terminal.sh` hardcodes gpkbd's font path and reserves screen space with
`--print-height`. `make install-config` deploys both.

## Roadmap

Nothing queued right now.
