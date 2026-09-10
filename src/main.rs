// gpkbd is an on-screen keyboard driven by a gamepad.
//
// It draws a grid of keys on a layer-shell surface that never takes keyboard
// focus, reads the pad straight from evdev, and types through a uinput device.
// Keystrokes therefore land in whatever the compositor has focused, and no
// pointer, no compositor-specific IPC and no input-method support is needed.

mod font8x8;
mod keyboard;
mod keys;
mod pad;
mod render;
mod typing;
mod util;

use std::ffi::CString;
use std::fs::File;
use std::os::fd::{AsFd, AsRawFd, FromRawFd, OwnedFd};
use std::time::Instant;

use input_linux::{EvdevHandle, Key, UInputHandle};
use wayland_client::protocol::{
    wl_buffer, wl_compositor, wl_output, wl_registry, wl_shm, wl_shm_pool, wl_surface,
};
use wayland_client::{delegate_noop, Connection, Dispatch, EventQueue, QueueHandle};
use wayland_protocols_wlr::layer_shell::v1::client::{zwlr_layer_shell_v1, zwlr_layer_surface_v1};

use keyboard::Keyboard;
use pad::PadEvent;
use util::die;

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
    keyboard: Keyboard,

    configured: bool,
    running: bool,
}

impl App {
    fn pixels_mut(&mut self) -> &mut [u32] {
        unsafe { std::slice::from_raw_parts_mut(self.pixels, self.pixels_len) }
    }
}

fn handle_pad_event(app: &mut App, pev: PadEvent) {
    match pev {
        PadEvent::Move(dr, dc) => {
            app.keyboard.move_sel(dr, dc);
            app.keyboard.start_repeat(dr, dc, Instant::now());
        }
        PadEvent::MoveEnd => app.keyboard.stop_repeat(),
        PadEvent::Type => {
            let key = app.keyboard.selected_key();
            typing::tap(&app.uinput, &mut app.keyboard, key);
        }
        PadEvent::Backspace => typing::tap(&app.uinput, &mut app.keyboard, Key::Backspace),
        PadEvent::Space => typing::tap(&app.uinput, &mut app.keyboard, Key::Space),
        PadEvent::ToggleAlt => app.keyboard.toggle_alt(),
        PadEvent::ToggleShift => app.keyboard.toggle_shift(),
        PadEvent::ToggleCtrl => app.keyboard.toggle_ctrl(),
        PadEvent::Enter => typing::tap(&app.uinput, &mut app.keyboard, Key::Enter),
        PadEvent::Quit => app.running = false,
    }
}

fn draw(app: &mut App) {
    let (width, height) = (app.width, app.height);
    let (sel_row, sel_col) = (app.keyboard.sel_row, app.keyboard.sel_col);
    let shift = app.keyboard.shift;
    let latched = app.keyboard.latched();
    render::draw(app.pixels_mut(), width, height, sel_row, sel_col, shift, latched);
}

fn commit(app: &mut App) {
    draw(app);
    let buffer = app.buffer.as_ref().unwrap();
    let surface = app.surface.as_ref().unwrap();
    surface.attach(Some(buffer), 0, 0);
    surface.damage_buffer(0, 0, app.width, app.height);
    surface.commit();
    app.keyboard.dirty = false;
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
         -h  surface height in pixels (default {})\n  \
         -p  substring of the gamepad's evdev name",
        render::DEFAULT_HEIGHT
    );
    std::process::exit(2);
}

fn parse_args() -> (i32, Option<String>) {
    let mut height = render::DEFAULT_HEIGHT;
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

    let pad = pad::open_pad(pad_name.as_deref()).unwrap_or_else(|| die("no gamepad found"));
    if let Err(e) = pad.grab(true) {
        eprintln!("EVIOCGRAB: {e}");
    }

    let uinput = typing::open_uinput();

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
        keyboard: Keyboard::new(),
        configured: false,
        running: true,
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

        let timeout_ms = app.keyboard.repeat_timeout_ms(Instant::now());

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
                for pev in pad::read_pad(&app.pad) {
                    handle_pad_event(&mut app, pev);
                }
            }
        }

        app.keyboard.tick_repeat(Instant::now());

        if event_queue.dispatch_pending(&mut app).is_err() {
            break;
        }

        if app.keyboard.dirty && app.configured {
            commit(&mut app);
        }
    }

    let _ = app.uinput.dev_destroy();
}
