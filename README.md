# gpkbd

An on-screen keyboard for typing with a gamepad.

It draws a grid of keys on a layer-shell surface that never takes keyboard
focus, reads the pad straight from evdev, and types through a uinput device.
Keystrokes land in whatever the compositor has focused, so no pointer, no
compositor-specific IPC, and no input-method support is needed. Built for
ROCKNIX handhelds running sway, where a controller is the only input device.

## Status

Working:

- 14x4 QWERTY grid, D-pad/hat navigation, wraps at the edges and
  auto-repeats on a held direction. Each row holds a physical QWERTY row's
  keys at their real position, trailing punctuation included (`-`/`=`/Esc
  end the digit row, `[`/`]`/`\` end the letter row, Ent ends the home row,
  matching an ANSI keyboard). The home and bottom rows lead with a Ctrl/Shift
  indicator cell instead of a real key (see below), which — like every other
  row's leading key — keeps 1/Q/A/Z aligned in the same column, and trail
  with an R2/R1 indicator cell (see below) in the one cell each row has
  free at the end. Rows shorter than the 14-key letter row run out of real
  keys before the last column; those cells stay empty rather than fake a
  key that isn't there.
- `BTN_EAST` (A) inputs the selected key, `BTN_SOUTH` (B) is backspace,
  matching the B-is-back convention on this device's Nintendo-style layout.
  `BTN_NORTH` (Y) is space, `BTN_WEST` (X) and Start both enter, Select
  quits. The X/Y codes are swapped from the standard Nintendo-layout
  convention on this device's driver (confirmed with an evdev capture) —
  see the doc comment on `pad::decode`.
- Shift and Ctrl are held modifiers, like a real keyboard: L1 holds Shift,
  L2 holds Ctrl — grouped onto the left shoulder to match the grid, where
  their indicator cells sit together on the left — for as long as the
  button is physically held. Shifted symbol keys show their shifted glyph
  live while held, and the grid's Ctrl/Shift cells (not typeable themselves)
  light up while the matching chord is down.
- R1 and R2 are both held chords over the D-pad/hat, so the grid selection
  doesn't move while you're really moving a text cursor: R1 sends arrow
  keys, R2 sends PageUp/PageDown (vertical only). Both auto-repeat like
  grid navigation does. R2 wins if both happen to be held. Grouped by
  shoulder-button number with Ctrl/Shift: the grid's R2 indicator cell
  trails the home row (with Ctrl), R1's trails the bottom row (with
  Shift), and each lights up while its chord is held.
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
