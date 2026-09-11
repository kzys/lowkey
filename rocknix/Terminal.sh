#!/bin/bash
# Opens a shell with lowkey's on-screen keyboard, so the gamepad can drive it.
# Either SELECT or exiting the shell ends both and returns to the frontend.

LOWKEY=/storage/.local/bin/lowkey
# Rocknix has no /usr/share/fonts; lowkey needs an explicit path to a font
# that actually exists on-device. Sans-serif reads better at the grid's
# small key size than a monospace font does; EmulationStation already
# ships one, matching the frontend's own look, so use that instead of
# reaching into ScummVM's assets a second time (foot uses those for its
# own monospace font, see rocknix/foot.ini).
FONT=/usr/config/emulationstation/resources/Rubik-Regular.ttf
NOMINAL_HEIGHT=$("${LOWKEY}" --print-height)

# Leave room for lowkey's overlay: without this, foot tiles to the full
# 640x480 panel and its bottom rows end up hidden behind the keyboard.
# Floating (rather than tiled) so foot renders above EmulationStation's
# fullscreen frontend surface.
foot &
FOOT_PID=$!

for i in $(seq 1 20); do
	swaymsg "[pid=${FOOT_PID}] floating enable, resize set 640px $((480 - NOMINAL_HEIGHT))px, move position 0 0" 2>/dev/null \
		| grep -q '"success": *true' && break
	sleep 0.05
done
swaymsg "[pid=${FOOT_PID}] focus" >/dev/null

# foot rounds the height we asked for down to a whole number of terminal
# rows, so it ends up a few pixels short of NOMINAL_HEIGHT's complement.
# Rather than hardcode that shortfall (font- and pixel-size-dependent), ask
# sway what foot actually settled on and give lowkey exactly what's left,
# so the two meet with no gap between them.
FOOT_HEIGHT=$(swaymsg -t get_tree | jq '[.. | objects | select(.name == "foot") | .rect.height] | first')
KBD_HEIGHT=$((480 - FOOT_HEIGHT))

"${LOWKEY}" -f "${FONT}" -h "${KBD_HEIGHT}" &
LOWKEY_PID=$!

wait -n "${LOWKEY_PID}" "${FOOT_PID}"

kill "${LOWKEY_PID}" "${FOOT_PID}" 2>/dev/null
wait
