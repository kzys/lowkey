#!/bin/bash
# Opens a shell with gpkbd's on-screen keyboard, so the gamepad can drive it.
# Either SELECT or exiting the shell ends both and returns to the frontend.

GPKBD=/storage/.local/bin/gpkbd
# Rocknix has no /usr/share/fonts; gpkbd needs an explicit path to a font
# that actually exists on-device. Sans-serif reads better at the grid's
# small key size than a monospace font does; EmulationStation already
# ships one, matching the frontend's own look, so use that instead of
# reaching into ScummVM's assets a second time (foot uses those for its
# own monospace font, see rocknix/foot.ini).
FONT=/usr/config/emulationstation/resources/Rubik-Regular.ttf
KBD_HEIGHT=$("${GPKBD}" --print-height)

# Leave room for gpkbd's overlay: without this, foot tiles to the full
# 640x480 panel and its bottom rows end up hidden behind the keyboard.
foot --window-size-pixels=640x$((480 - KBD_HEIGHT)) &
FOOT_PID=$!

for i in $(seq 1 20); do
	swaymsg "[pid=${FOOT_PID}] floating enable, move position 0 0" 2>/dev/null \
		| grep -q '"success": *true' && break
	sleep 0.05
done
swaymsg "[pid=${FOOT_PID}] focus" >/dev/null

"${GPKBD}" -f "${FONT}" &
GPKBD_PID=$!

wait -n "${GPKBD_PID}" "${FOOT_PID}"

kill "${GPKBD_PID}" "${FOOT_PID}" 2>/dev/null
wait
