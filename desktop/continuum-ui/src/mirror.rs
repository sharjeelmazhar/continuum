//! Phone mirroring window, in the style of iPhone Mirroring: only the phone's screen, with
//! rounded corners, floating on the desktop. A small bar fades in above it while the pointer
//! is over the window (drag it to move the window; Back, Home, Recents, Screenshot, Close).
//!
//! The daemon hands us a socket with the phone's H.264 stream (4-byte length + one access
//! unit, see `StartMirror`); GStreamer decodes it into a GTK paintable. Clicks, drags, the
//! scroll wheel and typing go back to the phone as `mirror.input` commands.

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, VecDeque};
use std::io::Read;
use std::os::unix::net::UnixStream;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};

use adw::prelude::*;
use futures_util::StreamExt;
use gstreamer as gst;
use gstreamer::prelude::*;
use serde_json::json;

use crate::ctx::{Device, dbus_error};
use crate::daemon::DaemonProxy;

/// Longest side of the video the phone sends, in pixels.
const MAX_SIDE: u32 = 1920;
/// Largest access unit we accept from the socket (a key frame at high quality is ~1 MB).
const MAX_FRAME: usize = 8 << 20;
/// Corner radius as a share of the screen's width (close to modern phones).
const CORNER: f64 = 0.115;
/// The bar appears only when the pointer is above the phone (never over its status bar).
const TOP_ZONE: f64 = 0.0;
/// Smallest the phone gets on screen, in pixels.
const MIN_HEIGHT: i32 = 320;
/// An invisible border around the phone (not above it: that's the bar) where you drag to
/// resize, like a window edge.
const MARGIN_SIDE: i32 = GRIP;
const MARGIN_TOP: i32 = 0;
const MARGIN_BOTTOM: i32 = GRIP;
const GRIP: i32 = 8;
/// Decoders to try, best first. Software decoding is the most reliable with the GTK sink.
const DECODERS: &[&str] = &["avdec_h264", "openh264dec", "vah264dec", "nvh264dec"];

thread_local! {
    /// Open windows by device; this keeps each one alive until it closes.
    static OPEN: RefCell<HashMap<String, Rc<Mirror>>> = RefCell::new(HashMap::new());
    static NEXT_ID: Cell<u32> = const { Cell::new(1) };
}

/// Opens (or brings forward) the mirroring window for `device`.
pub fn open(app: &gtk::Application, proxy: DaemonProxy<'static>, device: Device) {
    if let Some(m) = OPEN.with(|o| o.borrow().get(&device.id).cloned()) {
        m.window.present();
        return;
    }
    let m = Mirror::build(app, proxy, device);
    OPEN.with(|o| o.borrow_mut().insert(m.device.id.clone(), m.clone()));
    m.window.present();
    crate::window::dev_snapshots(&m.window, "mirror");
    m.clone().start();
}

/// What the phone reported last (`mirror.state`), as text for the window.
fn state_text(state: &str, phone: &str) -> Option<String> {
    Some(match state {
        "consent" => format!("Unlock {phone} and allow screen sharing there"),
        "denied" => format!("Screen sharing wasn't allowed on {phone}"),
        "stopped" => "Mirroring stopped on the phone".into(),
        "error" => format!("{phone} couldn't start mirroring"),
        "busy" => format!("{phone} is already sharing its screen with another app"),
        _ => return None,
    })
}

/// Normalised position (0..1) of a point in a `w`×`h` widget.
fn norm(x: f64, y: f64, w: f64, h: f64) -> (f64, f64) {
    if w <= 0.0 || h <= 0.0 {
        return (0.0, 0.0);
    }
    ((x / w).clamp(0.0, 1.0), (y / h).clamp(0.0, 1.0))
}

/// Window size for a phone screen of `pw`×`ph` pixels shown `height` pixels tall.
fn screen_size(pw: i32, ph: i32, height: i32) -> (i32, i32) {
    let (pw, ph) = (pw.max(1) as f64, ph.max(1) as f64);
    // Landscape: keep the same area on screen, so it doesn't become huge.
    let h = if pw > ph {
        (height as f64 * (ph / pw).sqrt()).round()
    } else {
        height as f64
    };
    ((h * pw / ph).round() as i32, h as i32)
}

/// Sends inputs one at a time, in order (D-Bus calls could otherwise overtake each other).
struct Outbox {
    queue: RefCell<VecDeque<String>>,
    busy: Cell<bool>,
}

struct Mirror {
    device: Device,
    proxy: DaemonProxy<'static>,
    window: gtk::Window,
    screen: gtk::Overlay,
    picture: gtk::Picture,
    bar: gtk::Box,
    status: gtk::Box,
    status_label: gtk::Label,
    spinner: gtk::Spinner,
    hint: gtk::Label,
    css: gtk::CssProvider,
    css_name: String,
    /// Phone screen size in pixels (from the video), and how tall we show it.
    phone: Cell<(i32, i32)>,
    height: Cell<i32>,
    /// Never taller than this (95% of the screen): it must not cover the whole screen.
    max_height: i32,
    pipeline: RefCell<Option<gst::Pipeline>>,
    socket: RefCell<Option<UnixStream>>,
    _bus: RefCell<Option<gst::bus::BusWatchGuard>>,
    outbox: Outbox,
    /// Pointer position over the screen, for the scroll wheel.
    pointer: Cell<(f64, f64)>,
    /// When the last `move` went out (they're sent at most once a frame).
    last_move: Cell<Instant>,
    /// The touchpad's finger on the phone during a two-finger scroll.
    finger: Cell<Option<(f64, f64)>>,
    scroll: Cell<(f64, f64)>,
    scroll_due: Cell<bool>,
    /// The phone can be controlled (its Accessibility helper is on).
    control: Cell<bool>,
    closed: Cell<bool>,
    /// The picture is showing.
    live: Cell<bool>,
}

impl Mirror {
    fn build(app: &gtk::Application, proxy: DaemonProxy<'static>, device: Device) -> Rc<Self> {
        let window = gtk::Window::builder()
            .application(app)
            .title(format!("{} Mirroring", device.name))
            .decorated(false)
            // Resizable only so we can set its size: a fixed-size window would grow to the
            // video's own size. Without decorations it has no edges to drag anyway.
            .resizable(true)
            .css_classes(["mirror-window"])
            .build();

        let button = |icon: &str, tip: &str| {
            gtk::Button::builder()
                .icon_name(icon)
                .tooltip_text(tip)
                .css_classes(["flat", "circular", "mirror-button"])
                .build()
        };
        let close = button("window-close-symbolic", "Close (Ctrl+W)");
        close.add_css_class("mirror-close");
        let back = button("go-previous-symbolic", "Back (Esc or right-click)");
        let home = button("go-home-symbolic", "Home (Ctrl+1)");
        let recents = button("view-grid-symbolic", "Recent apps (Ctrl+2)");
        let shot = button(
            "camera-photo-symbolic",
            "Screenshot (copied to this computer)",
        );
        let title = gtk::Label::builder()
            .label(&device.name)
            .ellipsize(gtk::pango::EllipsizeMode::End)
            .hexpand(true)
            .css_classes(["mirror-title"])
            .build();
        let bar = gtk::Box::builder()
            .spacing(2)
            .margin_bottom(8)
            .css_classes(["mirror-bar"])
            .build();
        bar.append(&close);
        bar.append(&title);
        bar.append(&back);
        bar.append(&home);
        bar.append(&recents);
        bar.append(&shot);
        let handle = gtk::WindowHandle::builder().child(&bar).build();

        let picture = gtk::Picture::builder()
            .content_fit(gtk::ContentFit::Fill)
            .can_shrink(true)
            .build();
        let spinner = gtk::Spinner::builder()
            .spinning(true)
            .width_request(28)
            .height_request(28)
            .build();
        let status_label = gtk::Label::builder()
            .label(format!("Connecting to {}…", device.name))
            .wrap(true)
            .justify(gtk::Justification::Center)
            .max_width_chars(24)
            .build();
        let status = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(14)
            .valign(gtk::Align::Center)
            .halign(gtk::Align::Center)
            .margin_start(24)
            .margin_end(24)
            .css_classes(["mirror-status"])
            .build();
        status.append(
            &gtk::Image::builder()
                .icon_name("phone-symbolic")
                .pixel_size(48)
                .build(),
        );
        status.append(&spinner);
        status.append(&status_label);
        let hint = gtk::Label::builder()
            .wrap(true)
            .justify(gtk::Justification::Center)
            .valign(gtk::Align::End)
            .margin_bottom(28)
            .margin_start(16)
            .margin_end(16)
            .visible(false)
            .css_classes(["mirror-hint"])
            .build();
        let screen = gtk::Overlay::builder()
            .child(&picture)
            .overflow(gtk::Overflow::Hidden)
            .css_classes(["mirror-screen", "waiting"])
            .focusable(true)
            .build();
        screen.add_overlay(&status);
        screen.add_overlay(&hint);
        let css_name = format!("mirror{}", NEXT_ID.with(|n| n.replace(n.get() + 1)));
        screen.set_widget_name(&css_name);

        let root = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .margin_start(MARGIN_SIDE)
            .margin_end(MARGIN_SIDE)
            .margin_bottom(MARGIN_BOTTOM)
            .margin_top(MARGIN_TOP)
            .build();
        root.append(&handle);
        root.append(&screen);
        // Resize grips over the invisible border: edges and corners.
        let stage = gtk::Overlay::builder().child(&root).build();
        let mut grips = Vec::new();
        for edge in [
            Edge::Left,
            Edge::Right,
            Edge::Bottom,
            Edge::BottomLeft,
            Edge::BottomRight,
        ] {
            let g = edge.widget();
            stage.add_overlay(&g);
            grips.push((edge, g));
        }
        window.set_child(Some(&stage));

        let css = gtk::CssProvider::new();
        if let Some(display) = gtk::gdk::Display::default() {
            gtk::style_context_add_provider_for_display(
                &display,
                &css,
                gtk::STYLE_PROVIDER_PRIORITY_APPLICATION + 1,
            );
        }
        let monitor_h: i32 = gtk::gdk::Display::default()
            .and_then(|d| d.monitors().item(0))
            .and_downcast::<gtk::gdk::Monitor>()
            .map_or(1000, |m| m.geometry().height());
        let m = Rc::new(Self {
            device,
            proxy,
            window,
            screen,
            picture,
            bar,
            status,
            status_label,
            spinner,
            hint,
            css,
            css_name,
            max_height: ((monitor_h as f64 * 0.95) as i32).max(400),
            phone: Cell::new((1080, 2400)),
            height: Cell::new(((monitor_h as f64 * 0.74) as i32).clamp(420, 1100)),
            pipeline: RefCell::new(None),
            socket: RefCell::new(None),
            _bus: RefCell::new(None),
            outbox: Outbox {
                queue: RefCell::new(VecDeque::new()),
                busy: Cell::new(false),
            },
            pointer: Cell::new((0.5, 0.5)),
            last_move: Cell::new(Instant::now()),
            finger: Cell::new(None),
            scroll: Cell::new((0.0, 0.0)),
            scroll_due: Cell::new(false),
            control: Cell::new(true),
            closed: Cell::new(false),
            live: Cell::new(false),
        });
        m.resize();
        for (edge, g) in grips {
            m.connect_grip(edge, &g);
        }
        // Maximised or fullscreen would stretch the phone: undo it at once.
        {
            let w = Rc::downgrade(&m);
            m.window.connect_maximized_notify(move |win| {
                if win.is_maximized() {
                    win.unmaximize();
                    if let Some(m) = w.upgrade() {
                        m.resize();
                    }
                }
            });
            m.window.connect_fullscreened_notify(|win| {
                if win.is_fullscreen() {
                    win.unfullscreen();
                }
            });
        }

        let key = |m: &Rc<Self>, k: &'static str| {
            let w = Rc::downgrade(m);
            move |_: &gtk::Button| {
                if let Some(m) = w.upgrade() {
                    m.send(json!({ "t": "key", "k": k }));
                }
            }
        };
        back.connect_clicked(key(&m, "back"));
        home.connect_clicked(key(&m, "home"));
        recents.connect_clicked(key(&m, "recents"));
        shot.connect_clicked(key(&m, "screenshot"));
        {
            let w = m.window.downgrade();
            close.connect_clicked(move |_| {
                if let Some(w) = w.upgrade() {
                    w.close();
                }
            });
        }
        m.connect_input();
        {
            let w = Rc::downgrade(&m);
            m.window.connect_close_request(move |_| {
                if let Some(m) = w.upgrade() {
                    m.shut_down();
                }
                glib::Propagation::Proceed
            });
        }
        m
    }

    /// Sizes the screen to the phone's shape and rounds its corners to match.
    fn resize(&self) {
        let (pw, ph) = self.phone.get();
        let (w, h) = screen_size(pw, ph, self.height.get());
        self.screen.set_size_request(w, h);
        self.bar.set_size_request(w, -1);
        // The window is the screen plus the bar above it and the margins for the shadow.
        let bar_h = self.bar.measure(gtk::Orientation::Vertical, w).1;
        if std::env::var_os("CONTINUUM_MIRROR_DEBUG").is_some() {
            tracing_log(&format!(
                "resize screen {w}x{h} bar {bar_h} window {}x{}",
                self.window.width(),
                self.window.height()
            ));
        }
        self.window
            .set_default_size(w + 2 * MARGIN_SIDE, h + bar_h + MARGIN_TOP + MARGIN_BOTTOM);
        let radius = (w.min(h) as f64 * CORNER).round();
        self.css.load_from_string(&format!(
            "#{} {{ border-radius: {radius}px; }} .mirror-bar {{ border-radius: 14px; }}",
            self.css_name
        ));
    }

    fn zoom(&self, by: f64) {
        let h = if by == 0.0 {
            1000.min(self.height.get().max(700))
        } else {
            (self.height.get() as f64 * by).round() as i32
        };
        self.height.set(h.clamp(MIN_HEIGHT, self.max_height));
        self.resize();
    }

    /// Dragging a grip scales the phone (its shape never changes).
    fn connect_grip(self: &Rc<Self>, edge: Edge, grip: &gtk::Box) {
        let drag = gtk::GestureDrag::new();
        let start = Rc::new(Cell::new(0));
        {
            let start = start.clone();
            let w = Rc::downgrade(self);
            drag.connect_drag_begin(move |_, _, _| {
                if let Some(m) = w.upgrade() {
                    start.set(m.height.get());
                }
            });
        }
        let w = Rc::downgrade(self);
        drag.connect_drag_update(move |_, dx, dy| {
            let Some(m) = w.upgrade() else { return };
            let (pw, ph) = m.phone.get();
            let per_x = ph.max(1) as f64 / pw.max(1) as f64; // height per pixel of width
            let grow = match edge {
                Edge::Right => dx * per_x,
                Edge::Left => -dx * per_x,
                Edge::Bottom => dy,
                Edge::BottomRight => (dx * per_x).max(dy),
                Edge::BottomLeft => (-dx * per_x).max(dy),
            };
            let h = (start.get() as f64 + grow).round() as i32;
            let h = h.clamp(MIN_HEIGHT, m.max_height);
            if h != m.height.get() {
                m.height.set(h);
                m.resize();
            }
        });
        grip.add_controller(drag);
    }

    fn show_status(&self, text: Option<&str>, busy: bool) {
        match text {
            Some(t) => {
                self.screen.add_css_class("waiting");
                self.status_label.set_label(t);
                self.spinner.set_visible(busy);
                self.status.set_visible(true);
            }
            None => {
                self.screen.remove_css_class("waiting");
                self.status.set_visible(false);
            }
        }
    }

    /// Queues one `mirror.input` for the phone. Ignored until the picture is live.
    fn send(self: &Rc<Self>, input: serde_json::Value) {
        // A key-frame request isn't an input: it works without remote control.
        let sync = input["k"] == "sync";
        if (self.status.is_visible() && !sync) || self.closed.get() {
            return;
        }
        if !self.control.get() && !sync {
            self.flash_hint();
            return;
        }
        if std::env::var_os("CONTINUUM_MIRROR_DEBUG").is_some() {
            tracing_log(&format!("input {input}"));
        }
        {
            // A newer position replaces a move still waiting to go out.
            let mut q = self.outbox.queue.borrow_mut();
            if input["t"] == "move" && q.back().is_some_and(|b| b.contains(r#""t":"move""#)) {
                q.pop_back();
            }
            q.push_back(input.to_string());
        }
        if self.outbox.busy.replace(true) {
            return;
        }
        let m = self.clone();
        glib::spawn_future_local(async move {
            loop {
                let next = m.outbox.queue.borrow_mut().pop_front();
                let Some(body) = next else { break };
                if let Err(e) = m.proxy.command(&m.device.id, "mirror.input", &body).await {
                    tracing_log(&format!("input not sent: {e}"));
                    m.outbox.queue.borrow_mut().clear();
                    break;
                }
            }
            m.outbox.busy.set(false);
        });
    }

    fn flash_hint(self: &Rc<Self>) {
        self.hint.set_label(&format!(
            "To use {} from here, turn on “Continuum remote control” in its Accessibility settings",
            self.device.name
        ));
        self.hint.set_visible(true);
        let w = Rc::downgrade(self);
        glib::timeout_add_local_once(Duration::from_secs(6), move || {
            if let Some(m) = w.upgrade() {
                m.hint.set_visible(false);
            }
        });
    }

    fn connect_input(self: &Rc<Self>) {
        // The bar fades in only when the pointer goes above the phone (over the bar), like on
        // a Mac, so it never gets in the way while you use the phone.
        let motion = gtk::EventControllerMotion::new();
        {
            let w = Rc::downgrade(self);
            let near_top = move |y: f64| {
                let Some(m) = w.upgrade() else { return };
                let top = m
                    .screen
                    .compute_bounds(&m.window)
                    .map_or(0.0, |b| b.y() as f64);
                if y < top + TOP_ZONE {
                    m.bar.add_css_class("shown");
                } else {
                    m.bar.remove_css_class("shown");
                }
            };
            let enter = near_top.clone();
            motion.connect_enter(move |_, _, y| enter(y));
            motion.connect_motion(move |_, _, y| near_top(y));
            let w = Rc::downgrade(self);
            motion.connect_leave(move |_| {
                if let Some(m) = w.upgrade() {
                    m.bar.remove_css_class("shown");
                }
            });
        }
        self.window.add_controller(motion);
        let over_screen = gtk::EventControllerMotion::new();
        {
            let w = Rc::downgrade(self);
            over_screen.connect_motion(move |_, x, y| {
                if let Some(m) = w.upgrade() {
                    m.pointer.set(m.norm(x, y));
                }
            });
        }
        self.screen.add_controller(over_screen);

        // Touch with the left button, live: the phone's finger goes down, follows the mouse and
        // lifts with it, like on iPhone Mirroring.
        let drag = gtk::GestureDrag::builder().button(1).build();
        {
            let w = Rc::downgrade(self);
            drag.connect_drag_begin(move |_, x, y| {
                let Some(m) = w.upgrade() else { return };
                m.screen.grab_focus();
                let (nx, ny) = m.norm(x, y);
                m.last_move.set(Instant::now());
                m.send(json!({ "t": "down", "x": round4(nx), "y": round4(ny) }));
            });
            let w = Rc::downgrade(self);
            drag.connect_drag_update(move |g, dx, dy| {
                let Some(m) = w.upgrade() else { return };
                let Some((x0, y0)) = g.start_point() else {
                    return;
                };
                // About one point per frame is plenty.
                if m.last_move.get().elapsed() < Duration::from_millis(12) {
                    return;
                }
                m.last_move.set(Instant::now());
                let (nx, ny) = m.norm(x0 + dx, y0 + dy);
                m.send(json!({ "t": "move", "x": round4(nx), "y": round4(ny) }));
            });
            let w = Rc::downgrade(self);
            drag.connect_drag_end(move |g, dx, dy| {
                let Some(m) = w.upgrade() else { return };
                let Some((x0, y0)) = g.start_point() else {
                    return;
                };
                let (nx, ny) = m.norm(x0 + dx, y0 + dy);
                m.send(json!({ "t": "up", "x": round4(nx), "y": round4(ny) }));
            });
        }
        self.screen.add_controller(drag);

        // Right-click: Back. Middle-click: Home.
        let click = gtk::GestureClick::builder().button(0).build();
        {
            let w = Rc::downgrade(self);
            click.connect_pressed(move |g, _, _, _| {
                let Some(m) = w.upgrade() else { return };
                match g.current_button() {
                    3 => m.send(json!({ "t": "key", "k": "back" })),
                    2 => m.send(json!({ "t": "key", "k": "home" })),
                    _ => {}
                }
            });
        }
        self.screen.add_controller(click);

        // Touchpad: two fingers drag the phone's screen live (it flings on its own when you
        // let go). Mouse wheel: each burst of notches becomes one swipe.
        let scroll = gtk::EventControllerScroll::new(gtk::EventControllerScrollFlags::BOTH_AXES);
        {
            let w = Rc::downgrade(self);
            scroll.connect_scroll_begin(move |_| {
                let Some(m) = w.upgrade() else { return };
                let (x, y) = m.pointer.get();
                m.finger.set(Some((x, y)));
                m.last_move.set(Instant::now());
                m.send(json!({ "t": "down", "x": round4(x), "y": round4(y) }));
            });
            let w = Rc::downgrade(self);
            scroll.connect_scroll_end(move |_| {
                let Some(m) = w.upgrade() else { return };
                if let Some((x, y)) = m.finger.take() {
                    m.send(json!({ "t": "up", "x": round4(x), "y": round4(y) }));
                }
            });
            let w = Rc::downgrade(self);
            scroll.connect_scroll(move |c, dx, dy| {
                let Some(m) = w.upgrade() else { return glib::Propagation::Proceed };
                if let Some((x, y)) = m.finger.get() {
                    let (w, h) = (m.screen.width().max(1) as f64, m.screen.height().max(1) as f64);
                    let (x, y) = ((x - dx / w).clamp(0.0, 1.0), (y - dy / h).clamp(0.0, 1.0));
                    m.finger.set(Some((x, y)));
                    if m.last_move.get().elapsed() >= Duration::from_millis(12) {
                        m.last_move.set(Instant::now());
                        m.send(json!({ "t": "move", "x": round4(x), "y": round4(y) }));
                    }
                    return glib::Propagation::Stop;
                }
                let (sx, sy) = if c.unit() == gtk::gdk::ScrollUnit::Surface {
                    let (w, h) = (m.screen.width().max(1) as f64, m.screen.height().max(1) as f64);
                    (dx / w, dy / h)
                } else {
                    (dx * 0.12, dy * 0.12)
                };
                let (ax, ay) = m.scroll.get();
                m.scroll.set((ax + sx, ay + sy));
                if !m.scroll_due.replace(true) {
                    let w = Rc::downgrade(&m);
                    glib::timeout_add_local_once(Duration::from_millis(70), move || {
                        let Some(m) = w.upgrade() else { return };
                        m.scroll_due.set(false);
                        let (dx, dy) = m.scroll.replace((0.0, 0.0));
                        let (x, y) = m.pointer.get();
                        if dx.abs() + dy.abs() > 0.002 {
                            m.send(json!({ "t": "scroll", "x": round4(x), "y": round4(y),
                                "dx": round4(dx.clamp(-1.0, 1.0)), "dy": round4(dy.clamp(-1.0, 1.0)) }));
                        }
                    });
                }
                glib::Propagation::Stop
            });
        }
        self.screen.add_controller(scroll);

        // Keyboard: text through an input method (accents, other layouts), keys by name.
        let keys = gtk::EventControllerKey::new();
        let im = gtk::IMMulticontext::new();
        im.set_client_widget(Some(&self.screen));
        {
            let w = Rc::downgrade(self);
            im.connect_commit(move |_, text| {
                if let Some(m) = w.upgrade()
                    && !text.is_empty()
                {
                    m.send(json!({ "t": "text", "s": text }));
                }
            });
        }
        keys.set_im_context(Some(&im));
        {
            let w = Rc::downgrade(self);
            keys.connect_key_pressed(move |_, key, _, state| {
                use gtk::gdk::Key;
                let Some(m) = w.upgrade() else {
                    return glib::Propagation::Proceed;
                };
                let ctrl = state.contains(gtk::gdk::ModifierType::CONTROL_MASK);
                let k = match key {
                    Key::w | Key::q if ctrl => {
                        m.window.close();
                        return glib::Propagation::Stop;
                    }
                    Key::plus | Key::equal | Key::KP_Add if ctrl => {
                        m.zoom(1.1);
                        return glib::Propagation::Stop;
                    }
                    Key::minus | Key::KP_Subtract if ctrl => {
                        m.zoom(1.0 / 1.1);
                        return glib::Propagation::Stop;
                    }
                    Key::_0 if ctrl => {
                        m.zoom(0.0);
                        return glib::Propagation::Stop;
                    }
                    Key::v if ctrl => "paste",
                    Key::_1 if ctrl => "home",
                    Key::_2 if ctrl => "recents",
                    Key::Escape => "back",
                    Key::BackSpace => "backspace",
                    Key::Delete | Key::KP_Delete => "delete",
                    Key::Return | Key::KP_Enter | Key::ISO_Enter => "enter",
                    Key::Tab => {
                        m.send(json!({ "t": "text", "s": "\t" }));
                        return glib::Propagation::Stop;
                    }
                    _ => return glib::Propagation::Proceed,
                };
                m.send(json!({ "t": "key", "k": k }));
                glib::Propagation::Stop
            });
        }
        self.window.add_controller(keys);
        let focus = gtk::EventControllerFocus::new();
        {
            let im2 = im.clone();
            focus.connect_enter(move |_| im2.focus_in());
            focus.connect_leave(move |_| im.focus_out());
        }
        self.window.add_controller(focus);
    }

    fn norm(&self, x: f64, y: f64) -> (f64, f64) {
        norm(
            x,
            y,
            self.screen.width() as f64,
            self.screen.height() as f64,
        )
    }

    /// Asks the daemon for the stream and starts decoding it.
    fn start(self: Rc<Self>) {
        let m = self.clone();
        glib::spawn_future_local(async move {
            m.watch_state().await;
            // Development aid: CONTINUUM_MIRROR_MAX=960 asks for a smaller video.
            let max = std::env::var("CONTINUUM_MIRROR_MAX")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(MAX_SIDE);
            let fd = match m.proxy.start_mirror(&m.device.id, max).await {
                Ok(fd) => fd,
                Err(e) => {
                    m.show_status(Some(&dbus_error(e).text()), false);
                    return;
                }
            };
            let fd: std::os::fd::OwnedFd = fd.into();
            let socket = UnixStream::from(fd);
            if let Err(e) = m.play(socket) {
                m.show_status(Some(&e), false);
            }
        });
    }

    async fn watch_state(self: &Rc<Self>) {
        let Ok(mut events) = self.proxy.receive_phone_event().await else {
            return;
        };
        let w = Rc::downgrade(self);
        glib::spawn_future_local(async move {
            while let Some(signal) = events.next().await {
                let Some(m) = w.upgrade() else { break };
                if m.closed.get() {
                    break;
                }
                let Ok(args) = signal.args() else { continue };
                if args.kind != "mirror.state" || args.id != m.device.id.as_str() {
                    continue;
                }
                let body: serde_json::Value = serde_json::from_str(args.body).unwrap_or_default();
                let state = body["state"].as_str().unwrap_or("");
                m.control.set(body["control"].as_bool().unwrap_or(false));
                // Status travels apart from the video and can arrive after the picture is up:
                // once it's live, only an ending (stopped, denied, error) may cover it.
                if m.live.get() && matches!(state, "consent" | "started" | "control") {
                    continue;
                }
                if state == "started" {
                    m.show_status(Some(&format!("Starting {}…", m.device.name)), true);
                    if !m.control.get() {
                        m.flash_hint();
                    }
                } else if let Some(text) = state_text(state, &m.device.name) {
                    m.show_status(Some(&text), state == "consent");
                }
            }
        });
    }

    fn play(self: &Rc<Self>, socket: UnixStream) -> Result<(), String> {
        gst::init().map_err(|e| e.to_string())?;
        let src = gst::ElementFactory::make("appsrc")
            .property("is-live", true)
            .property("do-timestamp", true)
            .property("max-bytes", 0u64)
            .property(
                "caps",
                gst::Caps::builder("video/x-h264")
                    .field("stream-format", "byte-stream")
                    .field("alignment", "au")
                    .build(),
            )
            .property_from_str("format", "time")
            .build()
            .map_err(|e| e.to_string())?;
        let dec = DECODERS
            .iter()
            .find_map(|d| gst::ElementFactory::make(d).build().ok())
            .ok_or("To show the phone's screen, install the GStreamer H.264 decoder (gstreamer1.0-libav on Ubuntu and Debian, gstreamer1-plugin-openh264 on Fedora, gst-libav on Arch)")?;
        if dec.find_property("max-threads").is_some() {
            // Frame threads add a frame of delay each; slices don't.
            dec.set_property("max-threads", 2i32);
            dec.set_property_from_str("thread-type", "slice");
        }
        let sink = gst::ElementFactory::make("gtk4paintablesink")
            .build()
            .map_err(|_| {
                "To show the phone's screen, install the GStreamer GTK 4 plugin (gstreamer1.0-gtk4)"
                    .to_owned()
            })?;
        sink.set_property("sync", false);
        let paintable = sink.property::<gtk::gdk::Paintable>("paintable");
        self.picture.set_paintable(Some(&paintable));
        {
            let w = Rc::downgrade(self);
            paintable.connect_invalidate_size(move |p| {
                let Some(m) = w.upgrade() else { return };
                let (pw, ph) = (p.intrinsic_width(), p.intrinsic_height());
                if pw > 0 && ph > 0 && m.phone.get() != (pw, ph) {
                    m.phone.set((pw, ph));
                    m.resize();
                }
            });
        }
        let pipeline = gst::Pipeline::new();
        pipeline
            .add_many([&src, &dec, &sink])
            .map_err(|e| e.to_string())?;
        gst::Element::link_many([&src, &dec, &sink]).map_err(|e| e.to_string())?;
        let bus = pipeline.bus().ok_or("no bus")?;
        let w = Rc::downgrade(self);
        let guard = bus
            .add_watch_local(move |_, msg| {
                if let gst::MessageView::Error(e) = msg.view()
                    && let Some(m) = w.upgrade()
                {
                    m.show_status(
                        Some(&format!("Couldn't show the phone's screen: {}", e.error())),
                        false,
                    );
                }
                glib::ControlFlow::Continue
            })
            .map_err(|e| e.to_string())?;
        *self._bus.borrow_mut() = Some(guard);
        pipeline
            .set_state(gst::State::Playing)
            .map_err(|e| e.to_string())?;

        let stats = Arc::new(Feed::default());
        let reader = socket.try_clone().map_err(|e| e.to_string())?;
        {
            let stats = stats.clone();
            std::thread::spawn(move || feed(reader, &src, &stats));
        }
        let shown = Rc::new(Cell::new(0u64));
        {
            let shown = shown.clone();
            paintable.connect_invalidate_contents(move |_| shown.set(shown.get() + 1));
        }
        let debug = std::env::var_os("CONTINUUM_MIRROR_DEBUG").is_some();
        let mut last = (Instant::now(), 0u64, 0u64, 0u64);
        *self.socket.borrow_mut() = Some(socket);
        *self.pipeline.borrow_mut() = Some(pipeline);

        // The first picture hides the status; the stream ending shows why.
        let w = Rc::downgrade(self);
        glib::timeout_add_local(Duration::from_millis(150), move || {
            let Some(m) = w.upgrade() else {
                return glib::ControlFlow::Break;
            };
            if m.closed.get() {
                return glib::ControlFlow::Break;
            }
            if stats.want_sync.swap(false, Ordering::Relaxed) {
                m.send(json!({ "t": "key", "k": "sync" }));
            }
            if debug && last.0.elapsed() >= Duration::from_secs(2) {
                // Development aid: the phone's latest frame as a picture, even when the window
                // is hidden (CONTINUUM_SNAPSHOT=/x.png writes /x.phone.png).
                if let Some(path) = std::env::var_os("CONTINUUM_SNAPSHOT")
                    && let Some(p) = m.picture.paintable()
                {
                    let (w, h) = (p.intrinsic_width().max(1), p.intrinsic_height().max(1));
                    let snap = gtk::Snapshot::new();
                    p.snapshot(&snap, w as f64, h as f64);
                    let renderer = gtk::gsk::CairoRenderer::new();
                    if let (Some(node), Ok(())) = (snap.to_node(), renderer.realize(None)) {
                        let path = std::path::PathBuf::from(path).with_extension("phone.png");
                        let tmp = path.with_extension("part");
                        if renderer.render_texture(&node, None).save_to_png(&tmp).is_ok() {
                            let _ = std::fs::rename(&tmp, &path);
                        }
                        renderer.unrealize();
                    }
                }
                let (frames, bytes, shown_now) = (
                    stats.frames.load(Ordering::Relaxed),
                    stats.bytes.load(Ordering::Relaxed),
                    shown.get(),
                );
                let secs = last.0.elapsed().as_secs_f64();
                tracing_log(&format!(
                    "{:.0} fps in, {:.0} fps shown, {:.1} Mbit/s, {} skipped",
                    (frames - last.1) as f64 / secs,
                    (shown_now - last.3) as f64 / secs,
                    (bytes - last.2) as f64 * 8.0 / secs / 1e6,
                    stats.dropped.load(Ordering::Relaxed)
                ));
                last = (Instant::now(), frames, bytes, shown_now);
            }
            if stats.ended.load(Ordering::Relaxed) {
                m.show_status(Some("Mirroring ended"), false);
                return glib::ControlFlow::Break;
            }
            let p = m.picture.paintable();
            let n = stats.frames.load(Ordering::Relaxed);
            if n > 0 && p.is_some_and(|p| p.intrinsic_width() > 0) && m.status.is_visible() {
                m.show_status(None, false);
                m.live.set(true);
                m.screen.grab_focus();
            }
            glib::ControlFlow::Continue
        });
        Ok(())
    }

    fn shut_down(&self) {
        if self.closed.replace(true) {
            return;
        }
        OPEN.with(|o| o.borrow_mut().remove(&self.device.id));
        if let Some(s) = self.socket.borrow_mut().take() {
            let _ = s.shutdown(std::net::Shutdown::Both);
        }
        if let Some(p) = self.pipeline.borrow_mut().take() {
            let _ = p.set_state(gst::State::Null);
        }
        if let Some(display) = gtk::gdk::Display::default() {
            gtk::style_context_remove_provider_for_display(&display, &self.css);
        }
        let (proxy, id) = (self.proxy.clone(), self.device.id.clone());
        glib::spawn_future_local(async move {
            let _ = proxy.stop_mirror(&id).await;
        });
    }
}

fn round4(v: f64) -> f64 {
    (v * 10_000.0).round() / 10_000.0
}

fn tracing_log(msg: &str) {
    eprintln!("continuum-ui: mirror: {msg}");
}

/// What the reader thread reports to the window.
#[derive(Default)]
struct Feed {
    frames: AtomicU64,
    bytes: AtomicU64,
    dropped: AtomicU64,
    /// The decoder fell behind and frames were skipped: ask the phone for a key frame.
    want_sync: AtomicBool,
    ended: AtomicBool,
}

/// Frames waiting for the decoder before we skip ahead (each one is ~16 ms of lag).
const MAX_BACKLOG: u64 = 3;

/// True if the access unit holds an IDR picture or SPS (a fresh start for the decoder).
fn is_key_frame(au: &[u8]) -> bool {
    let mut i = 0;
    while i + 3 < au.len() {
        if au[i] == 0 && au[i + 1] == 0 && au[i + 2] == 1 {
            let t = au[i + 3] & 0x1f;
            if t == 5 || t == 7 {
                return true;
            }
            // A non-IDR slice: the rest is picture data, no need to scan it.
            if t == 1 {
                return false;
            }
            i += 3;
        } else {
            i += 1;
        }
    }
    false
}

/// Reads access units from the daemon's socket into GStreamer until it closes. When the
/// decoder can't keep up, frames are skipped up to the next key frame instead of piling up as lag.
fn feed(mut socket: UnixStream, src: &gst::Element, feed: &Feed) {
    let mut len = [0u8; 4];
    let mut skipping = false;
    loop {
        if socket.read_exact(&mut len).is_err() {
            break;
        }
        let n = u32::from_be_bytes(len) as usize;
        // A zero-length frame is the phone saying it's still there (its screen is still).
        if n == 0 {
            continue;
        }
        if n > MAX_FRAME {
            tracing_log(&format!("bad frame length {n}"));
            break;
        }
        let mut data = vec![0u8; n];
        if socket.read_exact(&mut data).is_err() {
            break;
        }
        feed.bytes.fetch_add(n as u64 + 4, Ordering::Relaxed);
        let key = is_key_frame(&data);
        let backlog = src.property::<u64>("current-level-buffers");
        if !skipping && backlog >= MAX_BACKLOG {
            skipping = true;
            feed.want_sync.store(true, Ordering::Relaxed);
        }
        if skipping {
            if !key {
                feed.dropped.fetch_add(1, Ordering::Relaxed);
                continue;
            }
            skipping = false;
        }
        let buffer = gst::Buffer::from_mut_slice(data);
        if src.emit_by_name::<gst::FlowReturn>("push-buffer", &[&buffer]) != gst::FlowReturn::Ok {
            break;
        }
        feed.frames.fetch_add(1, Ordering::Relaxed);
    }
    feed.ended.store(true, Ordering::Relaxed);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn positions_are_clamped_to_the_screen() {
        assert_eq!(norm(50.0, 100.0, 100.0, 200.0), (0.5, 0.5));
        assert_eq!(norm(-5.0, 300.0, 100.0, 200.0), (0.0, 1.0));
        assert_eq!(norm(1.0, 1.0, 0.0, 0.0), (0.0, 0.0));
    }

    #[test]
    fn finds_key_frames() {
        let idr = [
            0, 0, 0, 1, 0x67, 1, 2, 0, 0, 1, 0x68, 3, 0, 0, 1, 0x65, 9, 9,
        ];
        assert!(is_key_frame(&idr));
        let p = [0, 0, 0, 1, 0x41, 9, 9, 9];
        assert!(!is_key_frame(&p));
        assert!(!is_key_frame(&[]));
        assert!(!is_key_frame(&[0, 0, 1]));
    }

    #[test]
    fn screen_keeps_the_phone_shape() {
        assert_eq!(screen_size(1080, 2400, 800), (360, 800));
        // Landscape keeps about the same area.
        let (w, h) = screen_size(2400, 1080, 800);
        assert!((w as f64 / h as f64 - 2400.0 / 1080.0).abs() < 0.01);
        assert!(h < 800 && w > 800);
    }

    #[test]
    fn known_states_have_text() {
        for s in ["consent", "denied", "stopped", "error", "busy"] {
            assert!(state_text(s, "Pixel 8").is_some());
        }
        assert!(state_text("started", "Pixel 8").is_none());
    }
}

/// Where a resize grip sits.
#[derive(Clone, Copy)]
enum Edge {
    Left,
    Right,
    Bottom,
    BottomLeft,
    BottomRight,
}

impl Edge {
    fn widget(self) -> gtk::Box {
        let (h, v, cursor) = match self {
            Edge::Left => (gtk::Align::Start, gtk::Align::Fill, "ew-resize"),
            Edge::Right => (gtk::Align::End, gtk::Align::Fill, "ew-resize"),
            Edge::Bottom => (gtk::Align::Fill, gtk::Align::End, "ns-resize"),
            Edge::BottomLeft => (gtk::Align::Start, gtk::Align::End, "nesw-resize"),
            Edge::BottomRight => (gtk::Align::End, gtk::Align::End, "nwse-resize"),
        };
        let corner = matches!(self, Edge::BottomLeft | Edge::BottomRight);
        let size = if corner { GRIP * 3 } else { GRIP };
        let b = gtk::Box::builder().halign(h).valign(v).build();
        match self {
            Edge::Left | Edge::Right => b.set_size_request(GRIP, -1),
            Edge::Bottom => b.set_size_request(-1, GRIP),
            _ => b.set_size_request(size, size),
        }
        // Edges stop below the bar, so it stays easy to grab.
        if !corner && matches!(self, Edge::Left | Edge::Right) {
            b.set_margin_top(48);
        }
        b.set_cursor_from_name(Some(cursor));
        b
    }
}
