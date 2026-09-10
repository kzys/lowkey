# gpkbd

An on-screen keyboard for typing with a gamepad.

It draws a grid of keys on a layer-shell surface that never takes keyboard
focus, reads the pad straight from evdev, and types through a uinput device.
Keystrokes land in whatever the compositor has focused, so no pointer, no
compositor-specific IPC, and no input-method support is needed. Built for
ROCKNIX handhelds running sway, where a controller is the only input device.

## Status

Working:

- 10x5 QWERTY-ish grid, D-pad/hat navigation, wraps at the edges.
- `BTN_SOUTH` inputs the selected key, `BTN_EAST` is backspace, `BTN_WEST`
  is space, Start is enter. (On this device's Nintendo-style layout,
  `BTN_SOUTH`/`BTN_EAST` are physically labeled B/A, not A/B.)
- One-shot Shift/Ctrl/Alt latches (`BTN_NORTH`, L1, R1), cleared after the
  next tap.
- Select to quit.
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

Rough order, not commitments:

- **L1/L2/R1/R2 as modifiers** — target device is the Anbernic RG35xx, which
  has four shoulder buttons instead of the two (L1/R1) the current mapping
  assumes; map the four to Shift/Ctrl.
- **Swap A/B** — bind backspace to `BTN_SOUTH` and input to `BTN_EAST`,
  matching this device's B-is-back convention (`BTN_SOUTH` is physically B).
- **Shifted labels** — the grid shows the unshifted key even when Shift is
  latched, so there's no way to see what a symbol key will actually type.
- **Held-direction repeat** — navigation only moves on the initial press;
  holding a direction should auto-repeat like it does on a real keyboard.
- **On-screen button legend** — a persistent hint for what A/B/X/Y/L1/R1/
  Start/Select currently do, since none of it is discoverable from the grid.
