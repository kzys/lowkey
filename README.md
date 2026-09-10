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
  is space, Start is enter, Select quits. (On this device's Nintendo-style
  layout, `BTN_SOUTH`/`BTN_EAST` are physically labeled B/A, matching the
  B-is-back convention.)
- One-shot Shift/Ctrl/Alt latches: either shoulder button on the left
  (L1/L2) is Shift, either on the right (R1/R2) is Ctrl, `BTN_NORTH` (X)
  is Alt. Latches clear after the next tap, and shifted symbol keys show
  their shifted glyph while latched.
- A legend line above the grid spells out what every button currently does.
- Bottom-anchored overlay sized with `-h`; pad picked by name with `-p` or
  auto-detected as the first device with a `BTN_SOUTH`.
- Cross-builds for aarch64, with no runtime dependency on the device's
  libwayland: `wayland-client`'s pure-Rust backend speaks the wire protocol
  directly, so there's nothing to pull off the device before linking.

## Build

```
make            # cross-builds target/aarch64-unknown-linux-gnu/release/gpkbd
make install    # scp to the device, killing any running instance first
```

`HOST` in the Makefile can be overridden on the command line. The target
needs `rustup target add aarch64-unknown-linux-gnu` and an
`aarch64-linux-gnu-gcc` on `PATH` for linking (configured in
`.cargo/config.toml`).

## Roadmap

Nothing queued right now.
