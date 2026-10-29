//! Universal clipboard on the desktop (PROTOCOL.md, "Clipboard").
//!
//! GNOME doesn't let background programs watch the Wayland clipboard, but every desktop keeps
//! the X11 clipboard (XWayland) in sync with it, and X11 clients may watch that. So a small
//! invisible X11 window here gets told about every copy (XFixes), reads texts and images, and
//! owns the clipboard when a phone sends something. Works the same on GNOME, KDE and X11 sessions.

use std::collections::VecDeque;
use std::os::fd::AsFd;
use std::os::unix::net::UnixStream;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use continuum_proto::clipboard::{Content, IMAGE_TYPES, MAX_BYTES, TEXT, split};
use continuum_proto::message::{CAP_CLIPBOARD, CLIPBOARD, Message};
use rustix::event::{PollFd, PollFlags, Timespec, poll};
use tracing::{debug, info};
use x11rb::connection::{Connection, RequestConnection};
use x11rb::protocol::Event;
use x11rb::protocol::xfixes::{ConnectionExt as _, SelectionEventMask};
use x11rb::protocol::xproto::{
    Atom, AtomEnum, ConnectionExt as _, CreateWindowAux, EventMask, PropMode, Property,
    SELECTION_NOTIFY_EVENT, SelectionNotifyEvent, SelectionRequestEvent, Window, WindowClass,
};
use x11rb::rust_connection::{DefaultStream, RustConnection};
use x11rb::wrapper::ConnectionExt as _;
use x11rb::{CURRENT_TIME, NONE};

use crate::state::Daemon;

const STEP_TIMEOUT: Duration = Duration::from_secs(2);
const RETRY: Duration = Duration::from_secs(10);

/// Remembers the last clipboard content seen in either direction, so nothing echoes back.
#[derive(Default)]
pub struct ClipHub {
    last: Mutex<Option<[u8; 32]>>,
    local: Mutex<Option<(Sender<Content>, UnixStream)>>,
}

impl ClipHub {
    /// Content copied on a phone: put it on the desktop clipboard.
    pub fn from_device(&self, c: Content) {
        *self.last.lock().unwrap_or_else(|e| e.into_inner()) = Some(c.digest());
        if let Some((tx, wake)) = &*self.local.lock().unwrap_or_else(|e| e.into_inner()) {
            let _ = tx.send(c);
            let _ = std::io::Write::write(&mut &*wake, &[1]);
        }
    }

    /// True if a desktop copy is new (not what a phone just sent us).
    fn is_new(&self, c: &Content) -> bool {
        let d = c.digest();
        let mut last = self.last.lock().unwrap_or_else(|e| e.into_inner());
        if *last == Some(d) {
            return false;
        }
        *last = Some(d);
        true
    }
}

/// Starts watching the desktop clipboard on a background thread.
pub fn start(daemon: Arc<Daemon>) {
    let (tx, rx) = channel();
    let Ok((wake_rd, wake_wr)) = UnixStream::pair() else {
        return;
    };
    let _ = wake_rd.set_nonblocking(true);
    *daemon.clip.local.lock().unwrap_or_else(|e| e.into_inner()) = Some((tx, wake_wr));
    let spawned = std::thread::Builder::new()
        .name("clipboard".into())
        .spawn(move || {
            loop {
                match X11::connect() {
                    Ok(x) => {
                        info!("watching the clipboard");
                        if let Err(e) = x.run(&daemon, &rx, &wake_rd) {
                            debug!("clipboard connection ended: {e}");
                        }
                    }
                    Err(e) => debug!("clipboard unavailable: {e}"),
                }
                std::thread::sleep(RETRY);
            }
        });
    if let Err(e) = spawned {
        tracing::warn!("clipboard thread: {e}");
    }
}

/// Sends a desktop copy to every connected phone that syncs the clipboard.
fn broadcast(daemon: &Daemon, c: &Content) {
    let Some(parts) = split(c) else { return };
    for tx in daemon.live_with(CAP_CLIPBOARD) {
        for p in &parts {
            let Ok(msg) = Message::with_body(CLIPBOARD, p) else {
                return;
            };
            if tx.blocking_send(msg).is_err() {
                break;
            }
        }
    }
    debug!("sent {} clipboard ({} bytes)", c.mime, c.data.len());
}

x11rb::atom_manager! {
    Atoms: AtomsCookie {
        CLIPBOARD,
        TARGETS,
        UTF8_STRING,
        TEXT,
        INCR,
        CONTINUUM_CLIP,
        TEXT_PLAIN_UTF8: b"text/plain;charset=utf-8",
        TEXT_PLAIN: b"text/plain",
        IMAGE_PNG: b"image/png",
        IMAGE_JPEG: b"image/jpeg",
        IMAGE_WEBP: b"image/webp",
        IMAGE_GIF: b"image/gif",
        PASSWORD_HINT: b"x-kde-passwordManagerHint",
        GNOME_FILES: b"x-special/gnome-copied-files",
    }
}

struct X11 {
    conn: RustConnection,
    win: Window,
    atoms: Atoms,
    /// Events that arrived while waiting for something else.
    queued: VecDeque<Event>,
}

impl X11 {
    fn connect() -> Result<Self, Box<dyn std::error::Error>> {
        let conn = open_display()?;
        let root = conn.setup().roots.first().ok_or("no screen")?.root;
        let win = conn.generate_id()?;
        conn.create_window(
            0,
            win,
            root,
            0,
            0,
            1,
            1,
            0,
            WindowClass::INPUT_ONLY,
            0,
            &CreateWindowAux::new().event_mask(EventMask::PROPERTY_CHANGE),
        )?;
        let atoms = Atoms::new(&conn)?.reply()?;
        conn.xfixes_query_version(5, 0)?.reply()?;
        conn.xfixes_select_selection_input(
            win,
            atoms.CLIPBOARD,
            SelectionEventMask::SET_SELECTION_OWNER,
        )?;
        conn.flush()?;
        Ok(Self {
            conn,
            win,
            atoms,
            queued: VecDeque::new(),
        })
    }

    fn run(
        mut self,
        daemon: &Daemon,
        incoming: &Receiver<Content>,
        wake: &UnixStream,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut owned: Option<Content> = None;
        // Anything the phone sent while we were disconnected is stale now.
        while incoming.try_recv().is_ok() {}
        loop {
            while let Some(ev) = self.next_event()? {
                match ev {
                    Event::XfixesSelectionNotify(e) if e.owner != self.win && e.owner != NONE => {
                        owned = None;
                        if let Some(c) = self.fetch()
                            && daemon.clip.is_new(&c)
                        {
                            broadcast(daemon, &c);
                        }
                    }
                    Event::SelectionRequest(e) => self.serve(owned.as_ref(), &e)?,
                    Event::SelectionClear(_) => owned = None,
                    _ => {}
                }
            }
            while let Ok(c) = incoming.try_recv() {
                self.conn
                    .set_selection_owner(self.win, self.atoms.CLIPBOARD, CURRENT_TIME)?;
                debug!("clipboard set from phone ({})", c.mime);
                owned = Some(c);
            }
            self.conn.flush()?;
            let mut buf = [0u8; 64];
            while matches!(std::io::Read::read(&mut &*wake, &mut buf), Ok(n) if n > 0) {}
            let x_fd = self.conn.stream().as_fd();
            let mut fds = [
                PollFd::new(&x_fd, PollFlags::IN),
                PollFd::new(wake, PollFlags::IN),
            ];
            poll(&mut fds, None)?;
            if fds[0].revents().intersects(PollFlags::HUP | PollFlags::ERR) {
                return Err("X server went away".into());
            }
        }
    }

    fn next_event(&mut self) -> Result<Option<Event>, x11rb::errors::ConnectionError> {
        if let Some(e) = self.queued.pop_front() {
            return Ok(Some(e));
        }
        self.conn.poll_for_event()
    }

    /// Waits up to `STEP_TIMEOUT` for an event matching `want`; others are queued.
    fn wait_for(&mut self, want: impl Fn(&Event) -> bool) -> Option<Event> {
        let deadline = Instant::now() + STEP_TIMEOUT;
        loop {
            match self.conn.poll_for_event().ok()? {
                Some(e) if want(&e) => return Some(e),
                Some(e) => self.queued.push_back(e),
                None => {
                    let left = deadline.checked_duration_since(Instant::now())?;
                    self.conn.flush().ok()?;
                    let ts = Timespec::try_from(left).ok()?;
                    let x_fd = self.conn.stream().as_fd();
                    let mut fds = [PollFd::new(&x_fd, PollFlags::IN)];
                    if poll(&mut fds, Some(&ts)).ok()? == 0 {
                        return None;
                    }
                }
            }
        }
    }

    /// Asks the clipboard owner for `target`; returns the property type and bytes.
    fn convert(&mut self, target: Atom) -> Option<(Atom, Vec<u8>)> {
        let (win, clip, prop) = (self.win, self.atoms.CLIPBOARD, self.atoms.CONTINUUM_CLIP);
        self.conn
            .convert_selection(win, clip, target, prop, CURRENT_TIME)
            .ok()?;
        let Event::SelectionNotify(n) = self.wait_for(
            |e| matches!(e, Event::SelectionNotify(n) if n.requestor == win && n.selection == clip),
        )?
        else {
            return None;
        };
        if n.property == NONE {
            return None;
        }
        let reply = self
            .conn
            .get_property(true, win, prop, AtomEnum::ANY, 0, u32::MAX / 4)
            .ok()?
            .reply()
            .ok()?;
        if reply.type_ != self.atoms.INCR {
            return Some((reply.type_, reply.value));
        }
        // Large data comes in pieces: each time we delete the property, the owner writes the next.
        let mut data = Vec::new();
        loop {
            self.wait_for(|e| {
                matches!(e, Event::PropertyNotify(p)
                    if p.window == win && p.atom == prop && p.state == Property::NEW_VALUE)
            })?;
            let part = self
                .conn
                .get_property(true, win, prop, AtomEnum::ANY, 0, u32::MAX / 4)
                .ok()?
                .reply()
                .ok()?;
            if part.value.is_empty() {
                return Some((part.type_, data));
            }
            data.extend_from_slice(&part.value);
            if data.len() > MAX_BYTES {
                return None;
            }
        }
    }

    /// Reads the new clipboard content, if it's a text or image we sync.
    fn fetch(&mut self) -> Option<Content> {
        let (_, raw) = self.convert(self.atoms.TARGETS)?;
        let targets: Vec<Atom> = raw
            .as_chunks::<4>()
            .0
            .iter()
            .map(|b| u32::from_ne_bytes(*b))
            .collect();
        let a = &self.atoms;
        // Password managers mark secrets; copied files are paths, not content.
        if targets.contains(&a.PASSWORD_HINT) || targets.contains(&a.GNOME_FILES) {
            return None;
        }
        let images = [
            (a.IMAGE_PNG, IMAGE_TYPES[0]),
            (a.IMAGE_JPEG, IMAGE_TYPES[1]),
            (a.IMAGE_WEBP, IMAGE_TYPES[2]),
            (a.IMAGE_GIF, IMAGE_TYPES[3]),
        ];
        if let Some((atom, mime)) = images.into_iter().find(|(t, _)| targets.contains(t)) {
            let (_, data) = self.convert(atom)?;
            return (!data.is_empty() && data.len() <= MAX_BYTES).then(|| Content {
                mime: mime.into(),
                data,
            });
        }
        let text = [a.UTF8_STRING, a.TEXT_PLAIN_UTF8, AtomEnum::STRING.into()]
            .into_iter()
            .find(|t| targets.contains(t))?;
        let (_, data) = self.convert(text)?;
        let s = match String::from_utf8(data) {
            Ok(s) => s,
            // STRING is Latin-1.
            Err(e) => e.into_bytes().iter().map(|&b| b as char).collect(),
        };
        (!s.is_empty() && s.len() <= MAX_BYTES).then(|| Content::text(&s))
    }

    /// Answers another program that pastes what a phone sent.
    fn serve(
        &self,
        owned: Option<&Content>,
        e: &SelectionRequestEvent,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let a = &self.atoms;
        let prop = if e.property == NONE {
            e.target
        } else {
            e.property
        };
        let mut ok = false;
        if let Some(c) = owned
            && e.selection == a.CLIPBOARD
        {
            let formats: Vec<Atom> = if c.mime == TEXT {
                vec![
                    a.UTF8_STRING,
                    a.TEXT_PLAIN_UTF8,
                    a.TEXT_PLAIN,
                    AtomEnum::STRING.into(),
                    a.TEXT,
                ]
            } else {
                [
                    (a.IMAGE_PNG, IMAGE_TYPES[0]),
                    (a.IMAGE_JPEG, IMAGE_TYPES[1]),
                    (a.IMAGE_WEBP, IMAGE_TYPES[2]),
                    (a.IMAGE_GIF, IMAGE_TYPES[3]),
                ]
                .into_iter()
                .filter(|(_, m)| *m == c.mime)
                .map(|(t, _)| t)
                .collect()
            };
            if e.target == a.TARGETS {
                let mut list = vec![a.TARGETS];
                list.extend(&formats);
                self.conn.change_property32(
                    PropMode::REPLACE,
                    e.requestor,
                    prop,
                    AtomEnum::ATOM,
                    &list,
                )?;
                ok = true;
            } else if formats.contains(&e.target)
                && c.data.len() + 1024 < self.conn.maximum_request_bytes()
            {
                let ty = if e.target == a.TEXT {
                    a.UTF8_STRING
                } else {
                    e.target
                };
                self.conn
                    .change_property8(PropMode::REPLACE, e.requestor, prop, ty, &c.data)?;
                ok = true;
            }
        }
        let notify = SelectionNotifyEvent {
            response_type: SELECTION_NOTIFY_EVENT,
            sequence: 0,
            time: e.time,
            requestor: e.requestor,
            selection: e.selection,
            target: e.target,
            property: if ok { prop } else { NONE },
        };
        self.conn
            .send_event(false, e.requestor, EventMask::NO_EVENT, notify)?;
        Ok(())
    }
}

/// Connects to the X server (XWayland on Wayland). A service started at login may not have
/// `DISPLAY`/`XAUTHORITY` yet, so they are also read from the systemd user environment.
fn open_display() -> Result<RustConnection, Box<dyn std::error::Error>> {
    let display = env("DISPLAY").ok_or("no DISPLAY")?;
    let parsed = x11rb_protocol::parse_display::parse_display(Some(&display))?;
    let auth = env("XAUTHORITY")
        .or_else(|| {
            std::env::var("HOME")
                .ok()
                .map(|h| format!("{h}/.Xauthority"))
        })
        .and_then(|path| std::fs::read(path).ok())
        .and_then(|bytes| xauth_cookie(&bytes, parsed.display))
        .unwrap_or_default();
    let mut last: Box<dyn std::error::Error> = "no address".into();
    for addr in parsed.connect_instruction() {
        match DefaultStream::connect(&addr) {
            Ok((stream, _)) => {
                return Ok(RustConnection::connect_to_stream_with_auth_info(
                    stream,
                    parsed.screen.into(),
                    auth.0,
                    auth.1,
                )?);
            }
            Err(e) => last = e.into(),
        }
    }
    Err(last)
}

fn env(name: &str) -> Option<String> {
    if let Ok(v) = std::env::var(name)
        && !v.is_empty()
    {
        return Some(v);
    }
    let out = std::process::Command::new("systemctl")
        .args(["--user", "show-environment"])
        .output()
        .ok()?;
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .find_map(|l| l.strip_prefix(&format!("{name}=")).map(str::to_owned))
}

/// Picks the (name, data) cookie for a local display from an Xauthority file.
fn xauth_cookie(mut b: &[u8], display: u16) -> Option<(Vec<u8>, Vec<u8>)> {
    fn field<'a>(b: &mut &'a [u8]) -> Option<&'a [u8]> {
        let len = u16::from_be_bytes([*b.first()?, *b.get(1)?]) as usize;
        let v = b.get(2..2 + len)?;
        *b = &b[2 + len..];
        Some(v)
    }
    let number = display.to_string();
    let mut fallback = None;
    while b.len() >= 2 {
        b = &b[2..]; // family
        let (_addr, num, name, data) = (
            field(&mut b)?,
            field(&mut b)?,
            field(&mut b)?,
            field(&mut b)?,
        );
        let entry = (name.to_vec(), data.to_vec());
        if num == number.as_bytes() {
            return Some(entry);
        }
        if num.is_empty() {
            fallback = Some(entry);
        }
    }
    fallback
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn xauth_picks_matching_display() {
        let mut f = Vec::new();
        for (num, cookie) in [("1", [1u8; 4]), ("0", [2u8; 4])] {
            f.extend_from_slice(&256u16.to_be_bytes());
            for v in [
                b"host".as_slice(),
                num.as_bytes(),
                b"MIT-MAGIC-COOKIE-1",
                &cookie,
            ] {
                f.extend_from_slice(&(v.len() as u16).to_be_bytes());
                f.extend_from_slice(v);
            }
        }
        assert_eq!(xauth_cookie(&f, 0).unwrap().1, vec![2; 4]);
        assert_eq!(xauth_cookie(&f, 1).unwrap().0, b"MIT-MAGIC-COOKIE-1");
        assert!(xauth_cookie(&f, 5).is_none());
        assert!(xauth_cookie(&f[..7], 0).is_none());
    }

    #[test]
    fn echo_is_suppressed() {
        let hub = ClipHub::default();
        let c = Content::text("hi");
        hub.from_device(c.clone());
        assert!(!hub.is_new(&c));
        assert!(hub.is_new(&Content::text("other")));
    }
}
