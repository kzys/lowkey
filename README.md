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
- Cross-builds for aarch64 against headers/libs pulled off the device.

## Build

```
make            # cross-builds ./gpkbd
make install    # scp to the device, killing any running instance first
make sync-libs  # refresh .sysroot/ from the device
```

`CROSS` and `HOST` in the Makefile can be overridden on the command line.

## Roadmap

Rough order, not commitments:

- **Rewrite in Rust** — under consideration, for memory safety and better
  error handling than the current `die()`-on-anything C style. `wayland-client`
  + `wayland-protocols-wlr` cover layer-shell, and `evdev`/`uinput` crates
  cover the pad and virtual keyboard, so the ecosystem fit is fine; the open
  question is cross-compiling against the device's aarch64 glibc/libwayland,
  same constraint the Makefile's `.sysroot` works around today.
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
