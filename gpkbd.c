// gpkbd is an on-screen keyboard driven by a gamepad.
//
// It draws a grid of keys on a layer-shell surface that never takes keyboard
// focus, reads the pad straight from evdev, and types through a uinput device.
// Keystrokes therefore land in whatever the compositor has focused, and no
// pointer, no compositor-specific IPC and no input-method support is needed.
#define _GNU_SOURCE

#include <errno.h>
#include <fcntl.h>
#include <linux/input.h>
#include <linux/uinput.h>
#include <poll.h>
#include <stdarg.h>
#include <stdbool.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/ioctl.h>
#include <sys/mman.h>
#include <time.h>
#include <unistd.h>
#include <wayland-client.h>

#include "font8x8_basic.h"
#include "wlr-layer-shell-unstable-v1-client-protocol.h"

#define COLS 10
#define ROWS 5
#define GLYPH 8

enum {
	COLOR_BG = 0xff1d1d1d,
	COLOR_KEY = 0xff2f2f36,
	COLOR_SELECTED = 0xff5f6f9f,
	COLOR_LATCHED = 0xff8f5f3f,
	COLOR_TEXT = 0xffffffff,
};

struct key {
	const char *label;
	uint16_t code;
};

// The grid is uniform, which keeps both hit testing and navigation trivial.
static const struct key keys[ROWS][COLS] = {
	{{"1", KEY_1}, {"2", KEY_2}, {"3", KEY_3}, {"4", KEY_4}, {"5", KEY_5},
	 {"6", KEY_6}, {"7", KEY_7}, {"8", KEY_8}, {"9", KEY_9}, {"0", KEY_0}},
	{{"q", KEY_Q}, {"w", KEY_W}, {"e", KEY_E}, {"r", KEY_R}, {"t", KEY_T},
	 {"y", KEY_Y}, {"u", KEY_U}, {"i", KEY_I}, {"o", KEY_O}, {"p", KEY_P}},
	{{"a", KEY_A}, {"s", KEY_S}, {"d", KEY_D}, {"f", KEY_F}, {"g", KEY_G},
	 {"h", KEY_H}, {"j", KEY_J}, {"k", KEY_K}, {"l", KEY_L}, {";", KEY_SEMICOLON}},
	{{"z", KEY_Z}, {"x", KEY_X}, {"c", KEY_C}, {"v", KEY_V}, {"b", KEY_B},
	 {"n", KEY_N}, {"m", KEY_M}, {",", KEY_COMMA}, {".", KEY_DOT}, {"/", KEY_SLASH}},
	{{"Esc", KEY_ESC}, {"Tab", KEY_TAB}, {"-", KEY_MINUS}, {"=", KEY_EQUAL},
	 {"'", KEY_APOSTROPHE}, {"`", KEY_GRAVE}, {"\\", KEY_BACKSLASH},
	 {"[", KEY_LEFTBRACE}, {"]", KEY_RIGHTBRACE}, {"Ent", KEY_ENTER}},
};

// Emitted from pad buttons rather than the grid, so they cost no cells.
static const uint16_t extra_codes[] = {
	KEY_BACKSPACE, KEY_SPACE, KEY_ENTER, KEY_LEFTSHIFT, KEY_LEFTCTRL, KEY_LEFTALT,
};

struct state {
	struct wl_display *display;
	struct wl_registry *registry;
	struct wl_compositor *compositor;
	struct wl_shm *shm;
	struct wl_output *output;
	struct zwlr_layer_shell_v1 *layer_shell;
	struct wl_surface *surface;
	struct zwlr_layer_surface_v1 *layer_surface;

	uint32_t *pixels;
	size_t pixels_size;
	struct wl_buffer *buffer;
	int32_t width, height;

	int pad_fd;
	int uinput_fd;

	int sel_row, sel_col;
	bool shift, ctrl, alt;
	bool configured, dirty, running;
};

static void die(const char *fmt, ...)
{
	va_list ap;

	va_start(ap, fmt);
	vfprintf(stderr, fmt, ap);
	va_end(ap);
	fputc('\n', stderr);
	exit(1);
}

// emit writes one event to the uinput device. A key press is a down, a sync, an
// up and another sync; consumers rely on the syncs to see a complete report.
static void emit(int fd, uint16_t type, uint16_t code, int32_t value)
{
	struct input_event ev = {.type = type, .code = code, .value = value};

	if (write(fd, &ev, sizeof(ev)) != sizeof(ev))
		fprintf(stderr, "uinput write: %s\n", strerror(errno));
}

static void tap(struct state *s, uint16_t code)
{
	if (s->shift)
		emit(s->uinput_fd, EV_KEY, KEY_LEFTSHIFT, 1);
	if (s->ctrl)
		emit(s->uinput_fd, EV_KEY, KEY_LEFTCTRL, 1);
	if (s->alt)
		emit(s->uinput_fd, EV_KEY, KEY_LEFTALT, 1);

	emit(s->uinput_fd, EV_KEY, code, 1);
	emit(s->uinput_fd, EV_SYN, SYN_REPORT, 0);
	emit(s->uinput_fd, EV_KEY, code, 0);

	if (s->alt)
		emit(s->uinput_fd, EV_KEY, KEY_LEFTALT, 0);
	if (s->ctrl)
		emit(s->uinput_fd, EV_KEY, KEY_LEFTCTRL, 0);
	if (s->shift)
		emit(s->uinput_fd, EV_KEY, KEY_LEFTSHIFT, 0);

	emit(s->uinput_fd, EV_SYN, SYN_REPORT, 0);

	// Modifiers are one-shot, the way a phone keyboard behaves.
	if (s->shift || s->ctrl || s->alt) {
		s->shift = s->ctrl = s->alt = false;
		s->dirty = true;
	}
}

static int open_uinput(void)
{
	struct uinput_setup setup = {
		.id = {.bustype = BUS_VIRTUAL, .vendor = 0x1209, .product = 0x0001},
		.name = "gpkbd",
	};
	int fd;

	fd = open("/dev/uinput", O_WRONLY | O_NONBLOCK);
	if (fd < 0)
		die("open /dev/uinput: %s", strerror(errno));

	if (ioctl(fd, UI_SET_EVBIT, EV_KEY) < 0)
		die("UI_SET_EVBIT: %s", strerror(errno));

	for (int r = 0; r < ROWS; r++)
		for (int c = 0; c < COLS; c++)
			ioctl(fd, UI_SET_KEYBIT, keys[r][c].code);
	for (size_t i = 0; i < sizeof(extra_codes) / sizeof(*extra_codes); i++)
		ioctl(fd, UI_SET_KEYBIT, extra_codes[i]);

	if (ioctl(fd, UI_DEV_SETUP, &setup) < 0)
		die("UI_DEV_SETUP: %s", strerror(errno));
	if (ioctl(fd, UI_DEV_CREATE) < 0)
		die("UI_DEV_CREATE: %s", strerror(errno));

	// Give the compositor a moment to notice the device before typing at it.
	nanosleep(&(struct timespec){.tv_nsec = 300 * 1000 * 1000}, NULL);
	return fd;
}

// open_pad finds an evdev node whose name contains want, or the first node
// advertising a gamepad button when want is NULL.
static int open_pad(const char *want)
{
	char path[64], name[256];
	unsigned long keybits[(KEY_MAX + 7) / 8 / sizeof(unsigned long) + 1];

	for (int i = 0; i < 32; i++) {
		int fd;

		snprintf(path, sizeof(path), "/dev/input/event%d", i);
		fd = open(path, O_RDONLY | O_NONBLOCK);
		if (fd < 0)
			continue;

		if (ioctl(fd, EVIOCGNAME(sizeof(name)), name) < 0) {
			close(fd);
			continue;
		}

		if (want) {
			if (strstr(name, want)) {
				fprintf(stderr, "pad: %s (%s)\n", name, path);
				return fd;
			}
		} else {
			memset(keybits, 0, sizeof(keybits));
			ioctl(fd, EVIOCGBIT(EV_KEY, sizeof(keybits)), keybits);
			if (keybits[BTN_SOUTH / (8 * sizeof(long))] &
			    (1UL << (BTN_SOUTH % (8 * sizeof(long))))) {
				fprintf(stderr, "pad: %s (%s)\n", name, path);
				return fd;
			}
		}
		close(fd);
	}
	return -1;
}

static void draw_glyph(struct state *s, int x, int y, int scale, char ch)
{
	const char *rows;

	if ((unsigned char)ch > 127)
		return;
	rows = font8x8_basic[(unsigned char)ch];

	for (int gy = 0; gy < GLYPH; gy++) {
		for (int gx = 0; gx < GLYPH; gx++) {
			if (!(rows[gy] & (1 << gx)))
				continue;
			for (int py = 0; py < scale; py++) {
				for (int px = 0; px < scale; px++) {
					int fx = x + gx * scale + px;
					int fy = y + gy * scale + py;

					if (fx >= 0 && fx < s->width && fy >= 0 && fy < s->height)
						s->pixels[fy * s->width + fx] = COLOR_TEXT;
				}
			}
		}
	}
}

static void fill(struct state *s, int x, int y, int w, int h, uint32_t color)
{
	for (int py = y; py < y + h; py++) {
		if (py < 0 || py >= s->height)
			continue;
		for (int px = x; px < x + w; px++) {
			if (px >= 0 && px < s->width)
				s->pixels[py * s->width + px] = color;
		}
	}
}

static void draw(struct state *s)
{
	int cw = s->width / COLS;
	int ch = s->height / ROWS;
	int scale = ch / (GLYPH * 2);

	if (scale < 1)
		scale = 1;

	fill(s, 0, 0, s->width, s->height, COLOR_BG);

	for (int r = 0; r < ROWS; r++) {
		for (int c = 0; c < COLS; c++) {
			const char *label = keys[r][c].label;
			int len = (int)strlen(label);
			int x = c * cw, y = r * ch;
			uint32_t bg = COLOR_KEY;
			int tx, ty;

			if (r == s->sel_row && c == s->sel_col)
				bg = COLOR_SELECTED;

			fill(s, x + 1, y + 1, cw - 2, ch - 2, bg);

			tx = x + (cw - len * GLYPH * scale) / 2;
			ty = y + (ch - GLYPH * scale) / 2;
			for (int i = 0; i < len; i++)
				draw_glyph(s, tx + i * GLYPH * scale, ty, scale, label[i]);
		}
	}

	// A latched modifier tints the top-left corner, which is cheaper to read
	// at a glance than a status line.
	if (s->shift || s->ctrl || s->alt)
		fill(s, 0, 0, cw / 4, ch / 4, COLOR_LATCHED);
}

static void commit(struct state *s)
{
	draw(s);
	wl_surface_attach(s->surface, s->buffer, 0, 0);
	wl_surface_damage_buffer(s->surface, 0, 0, s->width, s->height);
	wl_surface_commit(s->surface);
	s->dirty = false;
}

static void make_buffer(struct state *s)
{
	struct wl_shm_pool *pool;
	int fd;

	s->pixels_size = (size_t)s->width * s->height * 4;

	fd = memfd_create("gpkbd", MFD_CLOEXEC);
	if (fd < 0)
		die("memfd_create: %s", strerror(errno));
	if (ftruncate(fd, s->pixels_size) < 0)
		die("ftruncate: %s", strerror(errno));

	s->pixels = mmap(NULL, s->pixels_size, PROT_READ | PROT_WRITE, MAP_SHARED, fd, 0);
	if (s->pixels == MAP_FAILED)
		die("mmap: %s", strerror(errno));

	pool = wl_shm_create_pool(s->shm, fd, (int32_t)s->pixels_size);
	s->buffer = wl_shm_pool_create_buffer(pool, 0, s->width, s->height,
					      s->width * 4, WL_SHM_FORMAT_ARGB8888);
	wl_shm_pool_destroy(pool);
	close(fd);
}

static void layer_configure(void *data, struct zwlr_layer_surface_v1 *surface,
			    uint32_t serial, uint32_t w, uint32_t h)
{
	struct state *s = data;

	zwlr_layer_surface_v1_ack_configure(surface, serial);

	if (w > 0)
		s->width = (int32_t)w;
	if (h > 0)
		s->height = (int32_t)h;

	if (!s->buffer)
		make_buffer(s);

	s->configured = true;
	commit(s);
}

static void layer_closed(void *data, struct zwlr_layer_surface_v1 *surface)
{
	struct state *s = data;

	(void)surface;
	s->running = false;
}

static const struct zwlr_layer_surface_v1_listener layer_listener = {
	.configure = layer_configure,
	.closed = layer_closed,
};

static void registry_global(void *data, struct wl_registry *registry, uint32_t name,
			    const char *interface, uint32_t version)
{
	struct state *s = data;

	(void)version;
	if (!strcmp(interface, wl_compositor_interface.name))
		s->compositor = wl_registry_bind(registry, name, &wl_compositor_interface, 4);
	else if (!strcmp(interface, wl_shm_interface.name))
		s->shm = wl_registry_bind(registry, name, &wl_shm_interface, 1);
	else if (!strcmp(interface, zwlr_layer_shell_v1_interface.name))
		s->layer_shell = wl_registry_bind(registry, name,
						  &zwlr_layer_shell_v1_interface, 1);
	else if (!strcmp(interface, wl_output_interface.name) && !s->output)
		s->output = wl_registry_bind(registry, name, &wl_output_interface, 1);
}

static void registry_global_remove(void *data, struct wl_registry *registry, uint32_t name)
{
	(void)data, (void)registry, (void)name;
}

static const struct wl_registry_listener registry_listener = {
	.global = registry_global,
	.global_remove = registry_global_remove,
};

static void move(struct state *s, int dr, int dc)
{
	s->sel_row = (s->sel_row + dr + ROWS) % ROWS;
	s->sel_col = (s->sel_col + dc + COLS) % COLS;
	s->dirty = true;
}

static void on_pad_event(struct state *s, const struct input_event *ev)
{
	if (ev->type == EV_ABS) {
		// A hat reports -1, 0 or 1; act only on the press.
		if (ev->code == ABS_HAT0X && ev->value)
			move(s, 0, ev->value > 0 ? 1 : -1);
		else if (ev->code == ABS_HAT0Y && ev->value)
			move(s, ev->value > 0 ? 1 : -1, 0);
		return;
	}

	if (ev->type != EV_KEY || ev->value != 1)
		return;

	switch (ev->code) {
	case BTN_DPAD_LEFT:
		move(s, 0, -1);
		break;
	case BTN_DPAD_RIGHT:
		move(s, 0, 1);
		break;
	case BTN_DPAD_UP:
		move(s, -1, 0);
		break;
	case BTN_DPAD_DOWN:
		move(s, 1, 0);
		break;
	case BTN_SOUTH:
		tap(s, keys[s->sel_row][s->sel_col].code);
		break;
	case BTN_EAST:
		tap(s, KEY_BACKSPACE);
		break;
	case BTN_WEST:
		tap(s, KEY_SPACE);
		break;
	case BTN_NORTH:
		s->shift = !s->shift;
		s->dirty = true;
		break;
	case BTN_TL:
		s->ctrl = !s->ctrl;
		s->dirty = true;
		break;
	case BTN_TR:
		s->alt = !s->alt;
		s->dirty = true;
		break;
	case BTN_START:
		tap(s, KEY_ENTER);
		break;
	case BTN_SELECT:
		s->running = false;
		break;
	}
}

static void read_pad(struct state *s)
{
	struct input_event evs[32];
	ssize_t n;

	n = read(s->pad_fd, evs, sizeof(evs));
	if (n < 0)
		return;

	for (size_t i = 0; i < (size_t)n / sizeof(*evs); i++)
		on_pad_event(s, &evs[i]);
}

static void usage(void)
{
	fprintf(stderr,
		"usage: gpkbd [-h height] [-p pad-name]\n"
		"  -h  surface height in pixels (default 180)\n"
		"  -p  substring of the gamepad's evdev name\n");
	exit(2);
}

int main(int argc, char **argv)
{
	struct state s = {.height = 180, .running = true};
	const char *pad_name = NULL;
	struct pollfd fds[2];
	int opt;

	while ((opt = getopt(argc, argv, "h:p:")) != -1) {
		switch (opt) {
		case 'h':
			s.height = atoi(optarg);
			break;
		case 'p':
			pad_name = optarg;
			break;
		default:
			usage();
		}
	}

	s.pad_fd = open_pad(pad_name);
	if (s.pad_fd < 0)
		die("no gamepad found");
	if (ioctl(s.pad_fd, EVIOCGRAB, 1) < 0)
		fprintf(stderr, "EVIOCGRAB: %s\n", strerror(errno));

	s.uinput_fd = open_uinput();

	s.display = wl_display_connect(NULL);
	if (!s.display)
		die("cannot connect to a Wayland display");

	s.registry = wl_display_get_registry(s.display);
	wl_registry_add_listener(s.registry, &registry_listener, &s);
	wl_display_roundtrip(s.display);

	if (!s.compositor || !s.shm || !s.layer_shell)
		die("compositor lacks wl_shm or wlr-layer-shell");

	s.surface = wl_compositor_create_surface(s.compositor);
	// Naming the output explicitly matters: left to pick one itself, sway can
	// leave the surface uncomposited.
	// The overlay layer, not the top one: EmulationStation runs fullscreen, and
	// sway draws fullscreen surfaces over everything below overlay.
	s.layer_surface = zwlr_layer_shell_v1_get_layer_surface(
		s.layer_shell, s.surface, s.output, ZWLR_LAYER_SHELL_V1_LAYER_OVERLAY, "gpkbd");

	zwlr_layer_surface_v1_set_anchor(s.layer_surface,
					 ZWLR_LAYER_SURFACE_V1_ANCHOR_BOTTOM |
					 ZWLR_LAYER_SURFACE_V1_ANCHOR_LEFT |
					 ZWLR_LAYER_SURFACE_V1_ANCHOR_RIGHT);
	zwlr_layer_surface_v1_set_size(s.layer_surface, 0, (uint32_t)s.height);
	// Never take focus, or the keystrokes would come straight back to us.
	zwlr_layer_surface_v1_set_keyboard_interactivity(s.layer_surface, 0);
	zwlr_layer_surface_v1_add_listener(s.layer_surface, &layer_listener, &s);

	wl_surface_commit(s.surface);
	wl_display_roundtrip(s.display);

	fds[0] = (struct pollfd){.fd = wl_display_get_fd(s.display), .events = POLLIN};
	fds[1] = (struct pollfd){.fd = s.pad_fd, .events = POLLIN};

	while (s.running) {
		wl_display_flush(s.display);

		if (poll(fds, 2, -1) < 0) {
			if (errno == EINTR)
				continue;
			break;
		}

		if (fds[0].revents & POLLIN && wl_display_dispatch(s.display) < 0)
			break;
		if (fds[1].revents & POLLIN)
			read_pad(&s);

		if (s.dirty && s.configured)
			commit(&s);
	}

	ioctl(s.uinput_fd, UI_DEV_DESTROY);
	close(s.uinput_fd);
	close(s.pad_fd);
	return 0;
}
