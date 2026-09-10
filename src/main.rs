// gpkbd is an on-screen keyboard driven by a gamepad.
//
// It draws a grid of keys on a layer-shell surface that never takes keyboard
// focus, reads the pad straight from evdev, and types through a uinput device.
// Keystrokes therefore land in whatever the compositor has focused, and no
// pointer, no compositor-specific IPC and no input-method support is needed.

mod font8x8;

use std::ffi::CString;
use std::fs::{File, OpenOptions};
use std::os::fd::{AsFd, AsRawFd, FromRawFd, OwnedFd};
use std::os::unix::fs::OpenOptionsExt;
use std::time::{Duration, Instant};

use input_linux::sys;
use input_linux::{
    EventKind, EvdevHandle, InputId, Key, KeyEvent, KeyState, SynchronizeEvent, UInputHandle,
};
use wayland_client::protocol::{
    wl_buffer, wl_compositor, wl_output, wl_registry, wl_shm, wl_shm_pool, wl_surface,
};
use wayland_client::{delegate_noop, Connection, Dispatch, EventQueue, QueueHandle};
use wayland_protocols_wlr::layer_shell::v1::client::{zwlr_layer_shell_v1, zwlr_layer_surface_v1};

use font8x8::FONT8X8_BASIC;

const COLS: usize = 10;
const ROWS: usize = 5;
const GLYPH: i32 = 8;
const LEGEND_H: i32 = 18;
const DEFAULT_HEIGHT: i32 = 180 + LEGEND_H;

const REPEAT_DELAY: Duration = Duration::from_millis(400);
const REPEAT_INTERVAL: Duration = Duration::from_millis(120);

const COLOR_BG: u32 = 0xff1d1d1d;
const COLOR_KEY: u32 = 0xff2f2f36;
const COLOR_SELECTED: u32 = 0xff5f6f9f;
const COLOR_LATCHED: u32 = 0xff8f5f3f;
const COLOR_TEXT: u32 = 0xffffffff;
const COLOR_LEGEND: u32 = 0xff9a9aa5;

const LEGEND: &str = "A type  B back  Y space  X alt  L shift  R ctrl  Start enter  Select quit";

#[derive(Copy, Clone)]
struct KeyDef {
    label: &'static str,
    shifted: &'static str,
    code: Key,
}

const fn kd(label: &'static str, shifted: &'static str, code: Key) -> KeyDef {
    KeyDef { label, shifted, code }
}

// The grid is uniform, which keeps both hit testing and navigation trivial.
static KEYS: [[KeyDef; COLS]; ROWS] = [
    [
        kd("1", "!", Key::Num1),
        kd("2", "@", Key::Num2),
        kd("3", "#", Key::Num3),
        kd("4", "$", Key::Num4),
        kd("5", "%", Key::Num5),
        kd("6", "^", Key::Num6),
        kd("7", "&", Key::Num7),
        kd("8", "*", Key::Num8),
        kd("9", "(", Key::Num9),
        kd("0", ")", Key::Num0),
    ],
    [
        kd("q", "Q", Key::Q),
        kd("w", "W", Key::W),
        kd("e", "E", Key::E),
        kd("r", "R", Key::R),
        kd("t", "T", Key::T),
        kd("y", "Y", Key::Y),
        kd("u", "U", Key::U),
        kd("i", "I", Key::I),
        kd("o", "O", Key::O),
        kd("p", "P", Key::P),
    ],
    [
        kd("a", "A", Key::A),
        kd("s", "S", Key::S),
        kd("d", "D", Key::D),
        kd("f", "F", Key::F),
        kd("g", "G", Key::G),
        kd("h", "H", Key::H),
        kd("j", "J", Key::J),
        kd("k", "K", Key::K),
        kd("l", "L", Key::L),
        kd(";", ":", Key::Semicolon),
    ],
    [
        kd("z", "Z", Key::Z),
        kd("x", "X", Key::X),
        kd("c", "C", Key::C),
        kd("v", "V", Key::V),
        kd("b", "B", Key::B),
        kd("n", "N", Key::N),
        kd("m", "M", Key::M),
        kd(",", "<", Key::Comma),
        kd(".", ">", Key::Dot),
        kd("/", "?", Key::Slash),
    ],
    [
        kd("Esc", "Esc", Key::Esc),
        kd("Tab", "Tab", Key::Tab),
        kd("-", "_", Key::Minus),
        kd("=", "+", Key::Equal),
        kd("'", "\"", Key::Apostrophe),
        kd("`", "~", Key::Grave),
        kd("\\", "|", Key::Backslash),
        kd("[", "{", Key::LeftBrace),
        kd("]", "}", Key::RightBrace),
        kd("Ent", "Ent", Key::Enter),
    ],
];

// Emitted from pad buttons rather than the grid, so they cost no cells.
static EXTRA_KEYS: [Key; 6] = [
    Key::Backspace,
    Key::Space,
    Key::Enter,
    Key::LeftShift,
    Key::LeftCtrl,
    Key::LeftAlt,
];

fn die(msg: &str) -> ! {
    eprintln!("{msg}");
    std::process::exit(1);
}

// emit writes one event to the uinput device. A key press is a down, a sync, an
// up and another sync; consumers rely on the syncs to see a complete report.
fn emit_key(uinput: &UInputHandle<File>, key: Key, pressed: bool) {
    let value = if pressed { KeyState::PRESSED } else { KeyState::RELEASED };
    let ev = KeyEvent::new(Default::default(), key, value).into_event();
    if !matches!(uinput.write(std::slice::from_ref(ev.as_raw())), Ok(1)) {
        eprintln!("uinput write: key event failed");
    }
}

fn emit_syn(uinput: &UInputHandle<File>) {
    let ev = SynchronizeEvent::report(Default::default()).into_event();
    if !matches!(uinput.write(std::slice::from_ref(ev.as_raw())), Ok(1)) {
        eprintln!("uinput write: sync event failed");
    }
}

fn tap(app: &mut App, key: Key) {
    if app.shift {
        emit_key(&app.uinput, Key::LeftShift, true);
    }
    if app.ctrl {
        emit_key(&app.uinput, Key::LeftCtrl, true);
    }
    if app.alt {
        emit_key(&app.uinput, Key::LeftAlt, true);
    }

    emit_key(&app.uinput, key, true);
    emit_syn(&app.uinput);
    emit_key(&app.uinput, key, false);

    if app.alt {
        emit_key(&app.uinput, Key::LeftAlt, false);
    }
    if app.ctrl {
        emit_key(&app.uinput, Key::LeftCtrl, false);
    }
    if app.shift {
        emit_key(&app.uinput, Key::LeftShift, false);
    }

    emit_syn(&app.uinput);

    // Modifiers are one-shot, the way a phone keyboard behaves.
    if app.shift || app.ctrl || app.alt {
        app.shift = false;
        app.ctrl = false;
        app.alt = false;
        app.dirty = true;
    }
}

fn open_uinput() -> UInputHandle<File> {
    let file = OpenOptions::new()
        .write(true)
        .custom_flags(libc::O_NONBLOCK)
        .open("/dev/uinput")
        .unwrap_or_else(|e| die(&format!("open /dev/uinput: {e}")));

    let uinput = UInputHandle::new(file);
    uinput
        .set_evbit(EventKind::Key)
        .unwrap_or_else(|e| die(&format!("UI_SET_EVBIT: {e}")));

    for row in &KEYS {
        for k in row {
            let _ = uinput.set_keybit(k.code);
        }
    }
    for code in EXTRA_KEYS {
        let _ = uinput.set_keybit(code);
    }

    let id = InputId {
        bustype: sys::BUS_VIRTUAL,
        vendor: 0x1209,
        product: 0x0001,
        version: 0,
    };
    uinput
        .create(&id, b"gpkbd", 0, &[])
        .unwrap_or_else(|e| die(&format!("UI_DEV_SETUP/UI_DEV_CREATE: {e}")));

    // Give the compositor a moment to notice the device before typing at it.
    std::thread::sleep(Duration::from_millis(300));
    uinput
}

// open_pad finds an evdev node whose name contains want, or the first node
// advertising a gamepad button when want is None.
fn open_pad(want: Option<&str>) -> Option<EvdevHandle<File>> {
    for i in 0..32 {
        let path = format!("/dev/input/event{i}");
        let file = match OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NONBLOCK)
            .open(&path)
        {
            Ok(f) => f,
            Err(_) => continue,
        };
        let handle = EvdevHandle::new(file);

        let Ok(raw_name) = handle.device_name() else {
            continue;
        };
        let name_bytes = raw_name.split(|&b| b == 0).next().unwrap_or(&[]);
        let name = String::from_utf8_lossy(name_bytes);

        let found = match want {
            Some(w) => name.contains(w),
            None => handle.key_bits().map(|b| b.get(Key::ButtonSouth)).unwrap_or(false),
        };
        if found {
            eprintln!("pad: {name} ({path})");
            return Some(handle);
        }
    }
    None
}

fn draw_glyph(pixels: &mut [u32], width: i32, height: i32, x: i32, y: i32, scale: i32, ch: char) {
    draw_glyph_color(pixels, width, height, x, y, scale, ch, COLOR_TEXT);
}

fn draw_glyph_color(
    pixels: &mut [u32],
    width: i32,
    height: i32,
    x: i32,
    y: i32,
    scale: i32,
    ch: char,
    color: u32,
) {
    if !ch.is_ascii() {
        return;
    }
    let rows = &FONT8X8_BASIC[ch as usize];

    for gy in 0..GLYPH {
        for gx in 0..GLYPH {
            if rows[gy as usize] & (1 << gx) == 0 {
                continue;
            }
            for py in 0..scale {
                for px in 0..scale {
                    let fx = x + gx * scale + px;
                    let fy = y + gy * scale + py;
                    if fx >= 0 && fx < width && fy >= 0 && fy < height {
                        pixels[(fy * width + fx) as usize] = color;
                    }
                }
            }
        }
    }
}

fn fill(pixels: &mut [u32], width: i32, height: i32, x: i32, y: i32, w: i32, h: i32, color: u32) {
    for py in y..y + h {
        if py < 0 || py >= height {
            continue;
        }
        for px in x..x + w {
            if px >= 0 && px < width {
                pixels[(py * width + px) as usize] = color;
            }
        }
    }
}

fn draw(app: &mut App) {
    let (width, height) = (app.width, app.height);
    let (sel_row, sel_col) = (app.sel_row, app.sel_col);
    let shift = app.shift;
    let latched = app.shift || app.ctrl || app.alt;
    let pixels = app.pixels_mut();

    let grid_y0 = LEGEND_H;
    let grid_h = height - LEGEND_H;
    let cw = width / COLS as i32;
    let ch = grid_h / ROWS as i32;
    let scale = (ch / (GLYPH * 2)).max(1);

    fill(pixels, width, height, 0, 0, width, height, COLOR_BG);

    for (i, ch_) in LEGEND.chars().enumerate() {
        draw_glyph_color(pixels, width, height, 4 + i as i32 * GLYPH, 2, 1, ch_, COLOR_LEGEND);
    }

    for r in 0..ROWS {
        for c in 0..COLS {
            let key = &KEYS[r][c];
            let label = if shift { key.shifted } else { key.label };
            let len = label.chars().count() as i32;
            let x = c as i32 * cw;
            let y = grid_y0 + r as i32 * ch;
            let bg = if r == sel_row && c == sel_col {
                COLOR_SELECTED
            } else {
                COLOR_KEY
            };

            fill(pixels, width, height, x + 1, y + 1, cw - 2, ch - 2, bg);

            let tx = x + (cw - len * GLYPH * scale) / 2;
            let ty = y + (ch - GLYPH * scale) / 2;
            for (i, ch_) in label.chars().enumerate() {
                draw_glyph(pixels, width, height, tx + i as i32 * GLYPH * scale, ty, scale, ch_);
            }
        }
    }

    // A latched modifier tints the top-left corner of the grid, which is
    // cheaper to read at a glance than a status line.
    if latched {
        fill(pixels, width, height, 0, grid_y0, cw / 4, ch / 4, COLOR_LATCHED);
    }
}

fn commit(app: &mut App) {
    draw(app);
    let buffer = app.buffer.as_ref().unwrap();
    let surface = app.surface.as_ref().unwrap();
    surface.attach(Some(buffer), 0, 0);
    surface.damage_buffer(0, 0, app.width, app.height);
    surface.commit();
    app.dirty = false;
}

fn make_buffer(app: &mut App, qh: &QueueHandle<App>) {
    let size = (app.width as usize) * (app.height as usize) * 4;

    let name = CString::new("gpkbd").unwrap();
    let raw_fd = unsafe { libc::memfd_create(name.as_ptr(), libc::MFD_CLOEXEC) };
    if raw_fd < 0 {
        die(&format!("memfd_create: {}", std::io::Error::last_os_error()));
    }
    if unsafe { libc::ftruncate(raw_fd, size as libc::off_t) } < 0 {
        die(&format!("ftruncate: {}", std::io::Error::last_os_error()));
    }

    let ptr = unsafe {
        libc::mmap(
            std::ptr::null_mut(),
            size,
            libc::PROT_READ | libc::PROT_WRITE,
            libc::MAP_SHARED,
            raw_fd,
            0,
        )
    };
    if ptr == libc::MAP_FAILED {
        die(&format!("mmap: {}", std::io::Error::last_os_error()));
    }

    app.pixels = ptr as *mut u32;
    app.pixels_len = size / 4;

    let fd = unsafe { OwnedFd::from_raw_fd(raw_fd) };
    let shm = app.shm.as_ref().unwrap();
    let pool = shm.create_pool(fd.as_fd(), size as i32, qh, ());
    let buffer = pool.create_buffer(
        0,
        app.width,
        app.height,
        app.width * 4,
        wl_shm::Format::Argb8888,
        qh,
        (),
    );
    pool.destroy();
    // fd is dropped (closed) here; the mapping stays valid.

    app.buffer = Some(buffer);
}

struct App {
    compositor: Option<wl_compositor::WlCompositor>,
    shm: Option<wl_shm::WlShm>,
    output: Option<wl_output::WlOutput>,
    layer_shell: Option<zwlr_layer_shell_v1::ZwlrLayerShellV1>,

    surface: Option<wl_surface::WlSurface>,
    buffer: Option<wl_buffer::WlBuffer>,

    pixels: *mut u32,
    pixels_len: usize,
    width: i32,
    height: i32,

    uinput: UInputHandle<File>,
    pad: EvdevHandle<File>,

    sel_row: usize,
    sel_col: usize,
    shift: bool,
    ctrl: bool,
    alt: bool,
    configured: bool,
    dirty: bool,
    running: bool,

    // Held-direction repeat: Some(dir) while a D-pad/hat direction is held,
    // with repeat_at marking when the next auto-repeat move is due.
    repeat_dir: Option<(i32, i32)>,
    repeat_at: Instant,
}

impl App {
    fn pixels_mut(&mut self) -> &mut [u32] {
        unsafe { std::slice::from_raw_parts_mut(self.pixels, self.pixels_len) }
    }
}

fn move_sel(app: &mut App, dr: i32, dc: i32) {
    app.sel_row = ((app.sel_row as i32 + dr).rem_euclid(ROWS as i32)) as usize;
    app.sel_col = ((app.sel_col as i32 + dc).rem_euclid(COLS as i32)) as usize;
    app.dirty = true;
}

// start_repeat begins auto-repeat for a just-pressed direction; stop_repeat
// ends it on release, regardless of which direction was held.
fn start_repeat(app: &mut App, dr: i32, dc: i32) {
    app.repeat_dir = Some((dr, dc));
    app.repeat_at = Instant::now() + REPEAT_DELAY;
}

fn stop_repeat(app: &mut App) {
    app.repeat_dir = None;
}

fn on_pad_event(app: &mut App, ev: &sys::input_event) {
    if ev.type_ as i32 == sys::EV_ABS {
        if ev.code as i32 == sys::ABS_HAT0X {
            if ev.value != 0 {
                let dc = if ev.value > 0 { 1 } else { -1 };
                move_sel(app, 0, dc);
                start_repeat(app, 0, dc);
            } else {
                stop_repeat(app);
            }
        } else if ev.code as i32 == sys::ABS_HAT0Y {
            if ev.value != 0 {
                let dr = if ev.value > 0 { 1 } else { -1 };
                move_sel(app, dr, 0);
                start_repeat(app, dr, 0);
            } else {
                stop_repeat(app);
            }
        }
        return;
    }

    if ev.type_ as i32 != sys::EV_KEY {
        return;
    }
    let code = ev.code as i32;

    let dpad_dir = if code == sys::BTN_DPAD_LEFT {
        Some((0, -1))
    } else if code == sys::BTN_DPAD_RIGHT {
        Some((0, 1))
    } else if code == sys::BTN_DPAD_UP {
        Some((-1, 0))
    } else if code == sys::BTN_DPAD_DOWN {
        Some((1, 0))
    } else {
        None
    };
    if let Some((dr, dc)) = dpad_dir {
        if ev.value == 1 {
            move_sel(app, dr, dc);
            start_repeat(app, dr, dc);
        } else if ev.value == 0 {
            stop_repeat(app);
        }
        return;
    }

    if ev.value != 1 {
        return;
    }

    // B is physically the bottom face button on this device's Nintendo-style
    // layout, so it carries backspace; A confirms and types the selected key.
    if code == sys::BTN_SOUTH {
        tap(app, Key::Backspace);
    } else if code == sys::BTN_EAST {
        let key = KEYS[app.sel_row][app.sel_col].code;
        tap(app, key);
    } else if code == sys::BTN_WEST {
        tap(app, Key::Space);
    } else if code == sys::BTN_NORTH {
        app.alt = !app.alt;
        app.dirty = true;
    } else if code == sys::BTN_TL || code == sys::BTN_TL2 {
        app.shift = !app.shift;
        app.dirty = true;
    } else if code == sys::BTN_TR || code == sys::BTN_TR2 {
        app.ctrl = !app.ctrl;
        app.dirty = true;
    } else if code == sys::BTN_START {
        tap(app, Key::Enter);
    } else if code == sys::BTN_SELECT {
        app.running = false;
    }
}

fn read_pad(app: &mut App) {
    let mut evs: [sys::input_event; 32] = unsafe { std::mem::zeroed() };
    let n = match app.pad.read(&mut evs) {
        Ok(n) => n,
        Err(_) => return,
    };
    for ev in &evs[..n] {
        on_pad_event(app, ev);
    }
}

impl Dispatch<wl_registry::WlRegistry, ()> for App {
    fn event(
        app: &mut Self,
        registry: &wl_registry::WlRegistry,
        event: wl_registry::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        let wl_registry::Event::Global { name, interface, .. } = event else {
            return;
        };
        match interface.as_str() {
            "wl_compositor" => {
                app.compositor = Some(registry.bind::<wl_compositor::WlCompositor, _, _>(name, 4, qh, ()));
            }
            "wl_shm" => {
                app.shm = Some(registry.bind::<wl_shm::WlShm, _, _>(name, 1, qh, ()));
            }
            "zwlr_layer_shell_v1" => {
                app.layer_shell =
                    Some(registry.bind::<zwlr_layer_shell_v1::ZwlrLayerShellV1, _, _>(name, 1, qh, ()));
            }
            "wl_output" if app.output.is_none() => {
                app.output = Some(registry.bind::<wl_output::WlOutput, _, _>(name, 1, qh, ()));
            }
            _ => {}
        }
    }
}

impl Dispatch<zwlr_layer_surface_v1::ZwlrLayerSurfaceV1, ()> for App {
    fn event(
        app: &mut Self,
        surface: &zwlr_layer_surface_v1::ZwlrLayerSurfaceV1,
        event: zwlr_layer_surface_v1::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        match event {
            zwlr_layer_surface_v1::Event::Configure { serial, width, height } => {
                surface.ack_configure(serial);

                if width > 0 {
                    app.width = width as i32;
                }
                if height > 0 {
                    app.height = height as i32;
                }

                if app.buffer.is_none() {
                    make_buffer(app, qh);
                }

                app.configured = true;
                commit(app);
            }
            zwlr_layer_surface_v1::Event::Closed => {
                app.running = false;
            }
            _ => {}
        }
    }
}

delegate_noop!(App: ignore wl_compositor::WlCompositor);
delegate_noop!(App: ignore wl_shm::WlShm);
delegate_noop!(App: ignore wl_shm_pool::WlShmPool);
delegate_noop!(App: ignore wl_buffer::WlBuffer);
delegate_noop!(App: ignore wl_surface::WlSurface);
delegate_noop!(App: ignore wl_output::WlOutput);
delegate_noop!(App: ignore zwlr_layer_shell_v1::ZwlrLayerShellV1);

fn usage() -> ! {
    eprintln!(
        "usage: gpkbd [-h height] [-p pad-name]\n  \
         -h  surface height in pixels (default {DEFAULT_HEIGHT})\n  \
         -p  substring of the gamepad's evdev name"
    );
    std::process::exit(2);
}

fn parse_args() -> (i32, Option<String>) {
    let mut height = DEFAULT_HEIGHT;
    let mut pad_name = None;

    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-h" => {
                let Some(v) = args.next() else { usage() };
                height = v.parse().unwrap_or_else(|_| usage());
            }
            "-p" => {
                let Some(v) = args.next() else { usage() };
                pad_name = Some(v);
            }
            _ => usage(),
        }
    }
    (height, pad_name)
}

fn main() {
    let (height, pad_name) = parse_args();

    let pad = open_pad(pad_name.as_deref()).unwrap_or_else(|| die("no gamepad found"));
    if let Err(e) = pad.grab(true) {
        eprintln!("EVIOCGRAB: {e}");
    }

    let uinput = open_uinput();

    let conn = Connection::connect_to_env().unwrap_or_else(|e| die(&format!("cannot connect to a Wayland display: {e}")));
    let mut event_queue: EventQueue<App> = conn.new_event_queue();
    let qh = event_queue.handle();

    let display = conn.display();
    display.get_registry(&qh, ());

    let mut app = App {
        compositor: None,
        shm: None,
        output: None,
        layer_shell: None,
        surface: None,
        buffer: None,
        pixels: std::ptr::null_mut(),
        pixels_len: 0,
        width: 0,
        height,
        uinput,
        pad,
        sel_row: 0,
        sel_col: 0,
        shift: false,
        ctrl: false,
        alt: false,
        configured: false,
        dirty: false,
        running: true,
        repeat_dir: None,
        repeat_at: Instant::now(),
    };

    event_queue
        .roundtrip(&mut app)
        .unwrap_or_else(|e| die(&format!("initial roundtrip: {e}")));

    let compositor = app
        .compositor
        .clone()
        .unwrap_or_else(|| die("compositor lacks wl_compositor"));
    if app.shm.is_none() || app.layer_shell.is_none() {
        die("compositor lacks wl_shm or wlr-layer-shell");
    }

    app.surface = Some(compositor.create_surface(&qh, ()));

    // Naming the output explicitly matters: left to pick one itself, sway can
    // leave the surface uncomposited.
    // The overlay layer, not the top one: EmulationStation runs fullscreen, and
    // sway draws fullscreen surfaces over everything below overlay.
    let layer_surface = app.layer_shell.as_ref().unwrap().get_layer_surface(
        app.surface.as_ref().unwrap(),
        app.output.as_ref(),
        zwlr_layer_shell_v1::Layer::Overlay,
        "gpkbd".to_string(),
        &qh,
        (),
    );

    layer_surface.set_anchor(
        zwlr_layer_surface_v1::Anchor::Bottom
            | zwlr_layer_surface_v1::Anchor::Left
            | zwlr_layer_surface_v1::Anchor::Right,
    );
    layer_surface.set_size(0, app.height as u32);
    // Never take focus, or the keystrokes would come straight back to us.
    layer_surface.set_keyboard_interactivity(zwlr_layer_surface_v1::KeyboardInteractivity::None);

    app.surface.as_ref().unwrap().commit();
    event_queue
        .roundtrip(&mut app)
        .unwrap_or_else(|e| die(&format!("configure roundtrip: {e}")));

    while app.running {
        event_queue.flush().ok();

        let timeout_ms: i32 = match app.repeat_dir {
            Some(_) => {
                let now = Instant::now();
                if app.repeat_at <= now {
                    0
                } else {
                    (app.repeat_at - now).as_millis().min(i32::MAX as u128) as i32
                }
            }
            None => -1,
        };

        if let Some(guard) = event_queue.prepare_read() {
            let wl_fd = guard.connection_fd().as_raw_fd();
            let pad_fd = app.pad.as_inner().as_raw_fd();
            let mut fds = [
                libc::pollfd { fd: wl_fd, events: libc::POLLIN, revents: 0 },
                libc::pollfd { fd: pad_fd, events: libc::POLLIN, revents: 0 },
            ];

            let ret = unsafe { libc::poll(fds.as_mut_ptr(), fds.len() as libc::nfds_t, timeout_ms) };
            if ret < 0 {
                drop(guard);
                if std::io::Error::last_os_error().kind() == std::io::ErrorKind::Interrupted {
                    continue;
                }
                break;
            }

            if fds[0].revents & libc::POLLIN != 0 {
                let _ = guard.read();
            } else {
                drop(guard);
            }

            if fds[1].revents & libc::POLLIN != 0 {
                read_pad(&mut app);
            }
        }

        if let Some((dr, dc)) = app.repeat_dir {
            if Instant::now() >= app.repeat_at {
                move_sel(&mut app, dr, dc);
                app.repeat_at = Instant::now() + REPEAT_INTERVAL;
            }
        }

        if event_queue.dispatch_pending(&mut app).is_err() {
            break;
        }

        if app.dirty && app.configured {
            commit(&mut app);
        }
    }

    let _ = app.uinput.dev_destroy();
}
