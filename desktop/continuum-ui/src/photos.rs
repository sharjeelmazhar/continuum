//! Photos page: the phone's gallery as a grid of thumbnails (loaded as they scroll into
//! view, cached on disk), and a viewer to look at, copy, save or open a photo.

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use adw::prelude::*;
use base64::Engine;
use base64::engine::general_purpose::STANDARD as B64;
use gtk::gdk;
use serde::Deserialize;

use crate::ctx::{self, Ctx, ReqError};

const PAGE: usize = 120;
/// Big enough to stay sharp on HiDPI screens once cropped to a square tile.
const THUMB_PX: u32 = 480;
const THUMBS_PER_REQUEST: usize = 24;

#[derive(Debug, Clone, Deserialize)]
pub struct Photo {
    pub id: i64,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub mime: String,
    #[serde(default)]
    pub date: i64,
    #[serde(default)]
    pub size: u64,
    #[serde(default)]
    pub dur: i64,
}

#[derive(Deserialize)]
struct ListReply {
    items: Vec<Photo>,
    #[serde(default)]
    total: u64,
}

#[derive(Deserialize)]
struct Thumb {
    id: i64,
    data: String,
}

#[derive(Deserialize)]
struct ThumbsReply {
    items: Vec<Thumb>,
}

pub struct Photos {
    ctx: Rc<Ctx>,
    pub widget: gtk::Stack,
    pub subtitle: gtk::Label,
    /// Square tiles (cropped, like Apple Photos) or each photo's own shape.
    pub fit: gtk::ToggleButton,
    store: gtk::gio::ListStore,
    /// Ctrl/Shift-click or right-click to select several; Copy puts them on the clipboard.
    selection: gtk::MultiSelection,
    /// Where a Shift-click range starts.
    anchor: Cell<u32>,
    /// Bumped on every reload so answers for an old list are dropped.
    generation: Cell<u64>,
    loading_more: Cell<bool>,
    done: Cell<bool>,
    textures: RefCell<HashMap<i64, gdk::Texture>>,
    /// Pictures currently showing a photo, to fill in when its thumbnail arrives.
    bound: RefCell<HashMap<i64, glib::WeakRef<gtk::Picture>>>,
    queue: RefCell<Vec<i64>>,
    requested: RefCell<HashSet<i64>>,
    fetching: Cell<bool>,
    device: RefCell<String>,
}

pub fn build(ctx: &Rc<Ctx>) -> Rc<Photos> {
    let store = gtk::gio::ListStore::new::<glib::BoxedAnyObject>();
    let widget = gtk::Stack::builder()
        .transition_type(gtk::StackTransitionType::Crossfade)
        .build();
    let page = Rc::new(Photos {
        ctx: ctx.clone(),
        widget: widget.clone(),
        subtitle: gtk::Label::new(None),
        fit: gtk::ToggleButton::builder()
            .icon_name("zoom-fit-best-symbolic")
            .tooltip_text("Show Whole Photos")
            .build(),
        store: store.clone(),
        selection: gtk::MultiSelection::new(Some(store.clone())),
        anchor: Cell::new(0),
        generation: Cell::new(0),
        loading_more: Cell::new(false),
        done: Cell::new(false),
        textures: RefCell::default(),
        bound: RefCell::default(),
        queue: RefCell::default(),
        requested: RefCell::default(),
        fetching: Cell::new(false),
        device: RefCell::default(),
    });

    let factory = gtk::SignalListItemFactory::new();
    factory.connect_setup(move |_, item| {
        let item = item.downcast_ref::<gtk::ListItem>().expect("list item");
        let picture = gtk::Picture::builder()
            .content_fit(gtk::ContentFit::Cover)
            .width_request(150)
            .height_request(150)
            .build();
        picture.add_css_class("thumb");
        let badge = gtk::Label::builder()
            .halign(gtk::Align::End)
            .valign(gtk::Align::End)
            .margin_end(6)
            .margin_bottom(6)
            .visible(false)
            .css_classes(["video-badge"])
            .build();
        let check = gtk::Image::builder()
            .icon_name("object-select-symbolic")
            .halign(gtk::Align::End)
            .valign(gtk::Align::Start)
            .margin_end(6)
            .margin_top(6)
            .visible(false)
            .css_classes(["select-check"])
            .build();
        item.bind_property("selected", &check, "visible")
            .sync_create()
            .build();
        let overlay = gtk::Overlay::builder().child(&picture).build();
        overlay.add_overlay(&check);
        overlay.add_overlay(&badge);
        item.set_child(Some(&overlay));
    });
    {
        let page = Rc::downgrade(&page);
        factory.connect_bind(move |_, item| {
            let Some(page) = page.upgrade() else { return };
            let item = item.downcast_ref::<gtk::ListItem>().expect("list item");
            let (Some(obj), Some(overlay)) = (
                item.item().and_downcast::<glib::BoxedAnyObject>(),
                item.child().and_downcast::<gtk::Overlay>(),
            ) else {
                return;
            };
            let photo = obj.borrow::<Photo>().clone();
            let picture = overlay
                .child()
                .and_downcast::<gtk::Picture>()
                .expect("picture");
            let badge = overlay
                .last_child()
                .and_downcast::<gtk::Label>()
                .expect("badge");
            badge.set_visible(photo.dur > 0 || photo.mime.starts_with("video/"));
            badge.set_label(&format!("▶ {}", duration(photo.dur)));
            picture.set_tooltip_text(Some(&photo.name));
            // Lets a click on the grid find which item it hit.
            overlay.set_widget_name(&format!("tile:{}", item.position()));
            page.bind(photo.id, &picture);
        });
    }
    {
        let page = Rc::downgrade(&page);
        factory.connect_unbind(move |_, item| {
            let Some(page) = page.upgrade() else { return };
            let item = item.downcast_ref::<gtk::ListItem>().expect("list item");
            if let Some(obj) = item.item().and_downcast::<glib::BoxedAnyObject>() {
                page.bound.borrow_mut().remove(&obj.borrow::<Photo>().id);
            }
        });
    }

    let grid = gtk::GridView::builder()
        .model(&page.selection)
        .factory(&factory)
        .max_columns(12)
        .min_columns(2)
        .css_classes(["photo-grid"])
        .build();
    {
        // Clicks are ours, not the grid's: plain click opens, Ctrl/Shift-click select,
        // right-click offers Copy for everything selected.
        let click = gtk::GestureClick::builder()
            .button(0)
            .propagation_phase(gtk::PropagationPhase::Capture)
            .build();
        let weak = Rc::downgrade(&page);
        click.connect_pressed(move |g, _, x, y| {
            let (Some(page), Some(grid)) = (weak.upgrade(), g.widget()) else {
                return;
            };
            let Some(pos) = tile_at(&grid, x, y) else {
                return;
            };
            g.set_state(gtk::EventSequenceState::Claimed);
            grid.grab_focus();
            on_click(
                &page,
                &grid,
                pos,
                g.current_button(),
                g.current_event_state(),
                x,
                y,
            );
        });
        grid.add_controller(click);
    }
    {
        // Esc clears the selection; Ctrl+C copies it.
        let keys = gtk::EventControllerKey::new();
        let weak = Rc::downgrade(&page);
        keys.connect_key_pressed(move |_, key, _, mods| {
            let Some(page) = weak.upgrade() else {
                return glib::Propagation::Proceed;
            };
            if key == gdk::Key::Escape && !page.selection.selection().is_empty() {
                page.selection.unselect_all();
                return glib::Propagation::Stop;
            }
            if mods.contains(gdk::ModifierType::CONTROL_MASK) && key == gdk::Key::c {
                copy_selected(&page);
                return glib::Propagation::Stop;
            }
            if mods.contains(gdk::ModifierType::CONTROL_MASK) && key == gdk::Key::a {
                page.selection.select_all();
                return glib::Propagation::Stop;
            }
            glib::Propagation::Proceed
        });
        grid.add_controller(keys);
    }
    {
        let page = Rc::downgrade(&page);
        grid.connect_activate(move |grid, pos| {
            let (Some(page), Some(obj)) = (
                page.upgrade(),
                grid.model()
                    .and_then(|m| m.item(pos))
                    .and_downcast::<glib::BoxedAnyObject>(),
            ) else {
                return;
            };
            let photo = obj.borrow::<Photo>().clone();
            open_viewer(&page, photo);
        });
    }
    let scroller = gtk::ScrolledWindow::builder()
        .child(&grid)
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vexpand(true)
        .build();
    {
        let page = Rc::downgrade(&page);
        scroller.vadjustment().connect_value_changed(move |adj| {
            if adj.value() + adj.page_size() * 2.0 >= adj.upper()
                && let Some(page) = page.upgrade()
            {
                load_more(&page);
            }
        });
    }
    {
        let page = Rc::downgrade(&page);
        page.upgrade().expect("page").fit.connect_toggled(move |b| {
            let Some(page) = page.upgrade() else { return };
            b.set_tooltip_text(Some(if b.is_active() {
                "Show Square Tiles"
            } else {
                "Show Whole Photos"
            }));
            for p in page.bound.borrow().values().filter_map(|w| w.upgrade()) {
                p.set_content_fit(page.content_fit());
            }
        });
    }
    widget.add_named(&scroller, Some("grid"));
    widget.add_named(
        &gtk::Spinner::builder()
            .spinning(true)
            .width_request(32)
            .height_request(32)
            .halign(gtk::Align::Center)
            .valign(gtk::Align::Center)
            .build(),
        Some("loading"),
    );
    page
}

impl Photos {
    fn bind(self: &Rc<Self>, id: i64, picture: &gtk::Picture) {
        picture.set_content_fit(self.content_fit());
        self.bound.borrow_mut().insert(id, picture.downgrade());
        if let Some(t) = self.textures.borrow().get(&id) {
            picture.set_paintable(Some(t));
            return;
        }
        picture.set_paintable(gdk::Paintable::NONE);
        // Thumbnails from an earlier visit load from disk, without asking the phone.
        let path = ctx::cache_dir("thumbs-480", &self.device.borrow()).join(format!("{id}.jpg"));
        if let Ok(t) = gdk::Texture::from_filename(&path) {
            self.textures.borrow_mut().insert(id, t.clone());
            picture.set_paintable(Some(&t));
            return;
        }
        if self.requested.borrow_mut().insert(id) {
            self.queue.borrow_mut().push(id);
            fetch_thumbs(self);
        }
    }

    fn show_thumb(&self, id: i64, jpeg: &[u8]) {
        let Ok(t) = gdk::Texture::from_bytes(&glib::Bytes::from(jpeg)) else {
            return;
        };
        let dir = ctx::cache_dir("thumbs-480", &self.device.borrow());
        let path = dir.join(format!("{id}.jpg"));
        let tmp = dir.join(format!("{id}.jpg.part"));
        let _ = std::fs::create_dir_all(&dir)
            .and_then(|_| std::fs::write(&tmp, jpeg))
            .and_then(|_| std::fs::rename(&tmp, &path));
        if let Some(p) = self.bound.borrow().get(&id).and_then(|w| w.upgrade()) {
            p.set_paintable(Some(&t));
        }
        self.textures.borrow_mut().insert(id, t);
    }
}

/// Reloads the gallery from the top (device changed, reconnected, or Refresh).
pub fn reload(page: &Rc<Photos>) {
    let device = page.ctx.device().map(|d| d.id).unwrap_or_default();
    if *page.device.borrow() != device {
        page.textures.borrow_mut().clear();
        *page.device.borrow_mut() = device;
    }
    page.generation.set(page.generation.get() + 1);
    page.subtitle.set_label("");
    page.store.remove_all();
    page.bound.borrow_mut().clear();
    page.queue.borrow_mut().clear();
    page.requested.borrow_mut().clear();
    page.done.set(false);
    page.loading_more.set(false);
    page.widget.set_visible_child_name("loading");
    load_more(page);
}

fn load_more(page: &Rc<Photos>) {
    if page.loading_more.get() || page.done.get() {
        return;
    }
    page.loading_more.set(true);
    let generation = page.generation.get();
    let offset = page.store.n_items();
    let page = page.clone();
    glib::spawn_future_local(async move {
        let body = serde_json::json!({ "offset": offset, "limit": PAGE });
        let result = page.ctx.request("photos.list", body).await;
        if page.generation.get() != generation {
            return;
        }
        page.loading_more.set(false);
        let reply = result.and_then(|v| {
            serde_json::from_value::<ListReply>(v).map_err(|e| ReqError::Other(e.to_string()))
        });
        match reply {
            Ok(r) => {
                page.done.set(r.items.len() < PAGE);
                let objs: Vec<glib::BoxedAnyObject> =
                    r.items.into_iter().map(glib::BoxedAnyObject::new).collect();
                page.store.extend_from_slice(&objs);
                page.subtitle.set_label(&match r.total {
                    0 => String::new(),
                    1 => "1 item".into(),
                    n => format!("{n} items"),
                });
                if page.store.n_items() == 0 {
                    show_status(
                        &page,
                        ctx::status_page(
                            "image-x-generic-symbolic",
                            "No Photos Yet",
                            "Photos and videos on your phone show up here.",
                        ),
                    );
                } else {
                    page.widget.set_visible_child_name("grid");
                }
            }
            Err(e) if offset == 0 => show_status(&page, ctx::error_page(&e, "Photos")),
            Err(e) => page.ctx.toast(&e.text()),
        }
    });
}

fn show_status(page: &Photos, status: adw::StatusPage) {
    let retry = gtk::Button::builder()
        .label("Try Again")
        .halign(gtk::Align::Center)
        .css_classes(["pill"])
        .build();
    let weak = page.widget.downgrade();
    retry.connect_clicked(move |_| {
        if let Some(w) = weak.upgrade() {
            w.activate_action("win.reload", None).ok();
        }
    });
    status.set_child(Some(&retry));
    if let Some(old) = page.widget.child_by_name("status") {
        page.widget.remove(&old);
    }
    page.widget.add_named(&status, Some("status"));
    page.widget.set_visible_child_name("status");
}

/// Asks the phone for queued thumbnails, a batch at a time, newest-bound first.
fn fetch_thumbs(page: &Rc<Photos>) {
    if page.fetching.get() {
        return;
    }
    page.fetching.set(true);
    let page = page.clone();
    glib::spawn_future_local(async move {
        loop {
            let generation = page.generation.get();
            let ids: Vec<i64> = {
                let mut q = page.queue.borrow_mut();
                // Only what is still on screen; scrolled-past ones are asked for again later.
                let bound = page.bound.borrow();
                q.retain(|id| bound.contains_key(id));
                let take = q.len().min(THUMBS_PER_REQUEST);
                q.drain(..take).collect()
            };
            {
                let mut req = page.requested.borrow_mut();
                let bound = page.bound.borrow();
                req.retain(|id| bound.contains_key(id) || page.textures.borrow().contains_key(id));
            }
            if ids.is_empty() {
                break;
            }
            let body = serde_json::json!({ "ids": ids, "size": THUMB_PX });
            let reply = page.ctx.request("photos.thumbs", body).await;
            if page.generation.get() != generation {
                continue;
            }
            let Ok(reply) = reply.and_then(|v| {
                serde_json::from_value::<ThumbsReply>(v).map_err(|e| ReqError::Other(e.to_string()))
            }) else {
                for id in &ids {
                    page.requested.borrow_mut().remove(id);
                }
                break;
            };
            let got: HashSet<i64> = reply.items.iter().map(|t| t.id).collect();
            for t in reply.items {
                if let Ok(jpeg) = B64.decode(t.data) {
                    page.show_thumb(t.id, &jpeg);
                }
            }
            // Left out (over the size budget): ask again in the next batch.
            let mut q = page.queue.borrow_mut();
            for id in ids.into_iter().filter(|id| !got.contains(id)) {
                q.push(id);
            }
        }
        page.fetching.set(false);
    });
}

impl Photos {
    fn content_fit(&self) -> gtk::ContentFit {
        if self.fit.is_active() {
            gtk::ContentFit::Contain
        } else {
            gtk::ContentFit::Cover
        }
    }
}

/// Loads a photo the way it was taken: camera JPEGs are often stored sideways with an EXIF
/// rotation tag, which plain texture loading ignores.
fn load_upright(path: &std::path::Path) -> Option<gdk::Texture> {
    match gtk::gdk_pixbuf::Pixbuf::from_file(path) {
        Ok(p) => {
            let p = p.apply_embedded_orientation().unwrap_or(p);
            #[allow(deprecated)]
            Some(gdk::Texture::for_pixbuf(&p))
        }
        Err(_) => gdk::Texture::from_filename(path).ok(),
    }
}

fn duration(ms: i64) -> String {
    let s = ms / 1000;
    if s >= 3600 {
        format!("{}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60)
    } else {
        format!("{}:{:02}", s / 60, s % 60)
    }
}

fn human_size(bytes: u64) -> String {
    glib::format_size(bytes).to_string()
}

/// A photo full size, with Copy, Save and Open; videos play right here (Open uses the video
/// player).
fn open_viewer(page: &Rc<Photos>, photo: Photo) {
    let ctx = page.ctx.clone();
    let video = photo.dur > 0 || photo.mime.starts_with("video/");
    let header = adw::HeaderBar::new();
    let title = adw::WindowTitle::new(
        &photo.name,
        &format!(
            "{} · {}",
            ctx::long_time(photo.date),
            human_size(photo.size)
        ),
    );
    header.set_title_widget(Some(&title));
    let stack = gtk::Stack::new();
    let spinner = gtk::Spinner::builder()
        .spinning(true)
        .width_request(48)
        .height_request(48)
        .halign(gtk::Align::Center)
        .valign(gtk::Align::Center)
        .build();
    stack.add_named(&spinner, Some("loading"));
    let picture = gtk::Picture::builder()
        .content_fit(gtk::ContentFit::Contain)
        .hexpand(true)
        .vexpand(true)
        .build();
    stack.add_named(&picture, Some("photo"));
    // Show the thumbnail right away, sharpened when the full photo arrives.
    if let Some(t) = page.textures.borrow().get(&photo.id) {
        picture.set_paintable(Some(t));
        stack.set_visible_child_name("photo");
    }

    let copy = gtk::Button::builder()
        .icon_name("edit-copy-symbolic")
        .tooltip_text("Copy")
        .sensitive(false)
        .build();
    let save = gtk::Button::builder()
        .icon_name("folder-download-symbolic")
        .tooltip_text("Save to Downloads")
        .build();
    let open = gtk::Button::builder()
        .label("Open")
        .sensitive(false)
        .css_classes(["suggested-action"])
        .build();
    header.pack_end(&open);
    header.pack_end(&save);
    header.pack_end(&copy);

    let view = adw::ToolbarView::new();
    view.add_top_bar(&header);
    view.set_content(Some(&stack));
    let dialog = adw::Dialog::builder()
        .content_width(900)
        .content_height(680)
        .child(&view)
        .build();
    dialog.add_css_class("viewer");

    let id = photo.id;
    {
        let ctx = ctx.clone();
        let dialog = dialog.downgrade();
        save.connect_clicked(move |_| {
            let ctx = ctx.clone();
            let dialog = dialog.clone();
            glib::spawn_future_local(async move {
                let body = serde_json::json!({ "ids": [id] });
                match ctx.request("photos.send", body).await {
                    Ok(_) => {
                        ctx.toast("Saving to Downloads/Continuum…");
                        if let Some(d) = dialog.upgrade() {
                            d.close();
                        }
                    }
                    Err(e) => ctx.toast(&e.text()),
                }
            });
        });
    }
    dialog.present(Some(&ctx.window));
    if video {
        let status = ctx::status_page(
            "media-playback-start-symbolic",
            "Getting the Video…",
            &format!("{} from your phone.", human_size(photo.size)),
        );
        status.set_child(Some(
            &gtk::Spinner::builder()
                .spinning(true)
                .width_request(32)
                .height_request(32)
                .build(),
        ));
        stack.add_named(&status, Some("video"));
        stack.set_visible_child_name("video");
        let weak = dialog.downgrade();
        glib::spawn_future_local(async move {
            let result = ctx.fetch_photo(id).await;
            let Some(d) = weak.upgrade() else { return };
            match result {
                Ok(path) => {
                    let launch = {
                        let (ctx, path) = (ctx.clone(), path.clone());
                        move || {
                            let file = gtk::gio::File::for_path(&path);
                            gtk::FileLauncher::new(Some(&file)).launch(
                                Some(&ctx.window),
                                gtk::gio::Cancellable::NONE,
                                |_| {},
                            );
                        }
                    };
                    let Some(player) = crate::player::Player::new(std::path::Path::new(&path))
                    else {
                        // No GStreamer GTK sink on this computer: use the video player.
                        d.close();
                        launch();
                        return;
                    };
                    stack.add_named(&player.widget, Some("player"));
                    stack.set_visible_child_name("player");
                    open.set_sensitive(true);
                    copy.set_sensitive(true);
                    {
                        let (ctx, video) = (ctx.clone(), photo.clone());
                        copy.connect_clicked(move |_| {
                            let paths = clean_names(
                                std::slice::from_ref(&video),
                                std::slice::from_ref(&path),
                            );
                            let ctx = ctx.clone();
                            glib::spawn_future_local(async move {
                                let Some(p) = ctx.proxy() else { return };
                                let refs: Vec<&str> = paths.iter().map(String::as_str).collect();
                                match p.copy_files(&refs).await {
                                    Ok(()) => ctx.toast("Video copied. Paste it anywhere"),
                                    Err(e) => ctx.toast(&ctx::dbus_error(e).text()),
                                }
                            });
                        });
                    }
                    open.connect_clicked(move |_| launch());
                    // Stop playback when the viewer closes.
                    let keep = RefCell::new(Some(player));
                    d.connect_closed(move |_| {
                        keep.take();
                    });
                }
                Err(e) => {
                    status.set_title("Couldn't Get the Video");
                    status.set_description(Some(&e.text()));
                    status.set_child(gtk::Widget::NONE);
                }
            }
        });
        return;
    }

    glib::spawn_future_local(async move {
        let path = match ctx.fetch_photo(id).await {
            Ok(p) => p,
            Err(e) => {
                ctx.toast(&format!("Couldn't load the photo: {}", e.text()));
                return;
            }
        };
        let Some(texture) = load_upright(&path) else {
            ctx.toast("This photo's format can't be shown here");
            return;
        };
        picture.set_paintable(Some(&texture));
        stack.set_visible_child_name("photo");
        copy.set_sensitive(true);
        open.set_sensitive(true);
        let ctx2 = ctx.clone();
        copy.connect_clicked(move |_| {
            ctx2.window.clipboard().set_texture(&texture);
            ctx2.toast("Photo copied");
        });
        open.connect_clicked(move |_| {
            let file = gtk::gio::File::for_path(&path);
            let launcher = gtk::FileLauncher::new(Some(&file));
            launcher.launch(Some(&ctx.window), gtk::gio::Cancellable::NONE, |_| {});
        });
    });
}

/// The photos and videos currently selected, in grid order.
fn selected(page: &Photos) -> Vec<Photo> {
    let set = page.selection.selection();
    let mut out = Vec::new();
    if let Some((mut iter, first)) = gtk::BitsetIter::init_first(&set) {
        let mut pos = Some(first);
        while let Some(p) = pos {
            if let Some(obj) = page.store.item(p).and_downcast::<glib::BoxedAnyObject>() {
                out.push(obj.borrow::<Photo>().clone());
            }
            pos = iter.next();
        }
    }
    out
}

/// The grid position of the tile under (x, y), from the name set when it was bound.
fn tile_at(grid: &gtk::Widget, x: f64, y: f64) -> Option<u32> {
    let mut w = grid.pick(x, y, gtk::PickFlags::DEFAULT);
    while let Some(widget) = w {
        if let Some(pos) = widget.widget_name().strip_prefix("tile:") {
            return pos.parse().ok();
        }
        if &widget == grid {
            return None;
        }
        w = widget.parent();
    }
    None
}

fn on_click(
    page: &Rc<Photos>,
    grid: &gtk::Widget,
    pos: u32,
    button: u32,
    mods: gdk::ModifierType,
    x: f64,
    y: f64,
) {
    let sel = &page.selection;
    match button {
        3 => {
            if !sel.is_selected(pos) {
                sel.select_item(pos, true);
                page.anchor.set(pos);
            }
            context_menu(page, grid, x, y);
        }
        1 if mods.contains(gdk::ModifierType::SHIFT_MASK) => {
            let a = page.anchor.get();
            let (from, to) = (a.min(pos), a.max(pos));
            sel.select_range(from, to - from + 1, false);
        }
        1 if mods.contains(gdk::ModifierType::CONTROL_MASK) || !sel.selection().is_empty() => {
            if sel.is_selected(pos) {
                sel.unselect_item(pos);
            } else {
                sel.select_item(pos, false);
            }
            page.anchor.set(pos);
        }
        1 => {
            if let Some(obj) = page.store.item(pos).and_downcast::<glib::BoxedAnyObject>() {
                let photo = obj.borrow::<Photo>().clone();
                open_viewer(page, photo);
            }
        }
        _ => {}
    }
}

type MenuAction = Box<dyn Fn(&Rc<Photos>)>;

/// Right-click menu on the grid: Copy, Save to Downloads, Select All.
fn context_menu(page: &Rc<Photos>, at: &gtk::Widget, x: f64, y: f64) {
    let n = page.selection.selection().size();
    let what = if n == 1 {
        "1 Item".to_owned()
    } else {
        format!("{n} Items")
    };
    let menu = gtk::Box::new(gtk::Orientation::Vertical, 0);
    let popover = gtk::Popover::builder()
        .child(&menu)
        .has_arrow(false)
        .pointing_to(&gdk::Rectangle::new(x as i32, y as i32, 1, 1))
        .build();
    popover.add_css_class("menu");
    let item = |label: &str, run: MenuAction| {
        let b = gtk::Button::builder()
            .label(label)
            .css_classes(["flat"])
            .build();
        if let Some(l) = b.child().and_downcast::<gtk::Label>() {
            l.set_xalign(0.0);
        }
        let (page, popover) = (page.clone(), popover.downgrade());
        b.connect_clicked(move |_| {
            if let Some(p) = popover.upgrade() {
                p.popdown();
            }
            run(&page);
        });
        menu.append(&b);
    };
    item(&format!("Copy {what}"), Box::new(copy_selected));
    item(
        &format!("Save {what} to Downloads"),
        Box::new(save_selected),
    );
    item(
        "Select All",
        Box::new(|p: &Rc<Photos>| {
            p.selection.select_all();
        }),
    );
    item(
        "Clear Selection",
        Box::new(|p: &Rc<Photos>| {
            p.selection.unselect_all();
        }),
    );
    popover.set_parent(at);
    popover.connect_closed(|p| {
        let p = p.clone();
        glib::idle_add_local_once(move || p.unparent());
    });
    popover.popup();
}

/// Downloads the selected items (cached ones are instant) and puts them on the clipboard,
/// ready to paste in Files, chat apps or editors.
fn copy_selected(page: &Rc<Photos>) {
    let items = selected(page);
    if items.is_empty() {
        return;
    }
    let ctx = page.ctx.clone();
    let n = items.len();
    ctx.toast(&if n == 1 {
        "Copying…".to_owned()
    } else {
        format!("Copying {n} items…")
    });
    glib::spawn_future_local(async move {
        let mut paths = Vec::new();
        for p in &items {
            match ctx.fetch_photo(p.id).await {
                Ok(path) => paths.push(path),
                Err(e) => {
                    ctx.toast(&format!("Couldn't copy {}: {}", p.name, e.text()));
                    return;
                }
            }
        }
        let Some(proxy) = ctx.proxy() else { return };
        let paths = clean_names(&items, &paths);
        let refs: Vec<&str> = paths.iter().map(String::as_str).collect();
        match proxy.copy_files(&refs).await {
            Ok(()) => ctx.toast(&if n == 1 {
                "Copied. Paste it anywhere".to_owned()
            } else {
                format!("{n} items copied. Paste them anywhere")
            }),
            Err(e) => ctx.toast(&ctx::dbus_error(e).text()),
        }
    });
}

/// Cached downloads carry an id prefix; paste them under the phone's own names instead, via
/// hard links (or copies) in a fresh folder. Older folders are cleared out.
fn clean_names(items: &[Photo], cached: &[std::path::PathBuf]) -> Vec<String> {
    let root = ctx::cache_dir("clip", "");
    if let Ok(old) = std::fs::read_dir(&root) {
        for e in old.flatten() {
            let _ = std::fs::remove_dir_all(e.path());
        }
    }
    let dir = root.join(glib::monotonic_time().to_string());
    let mut out = Vec::new();
    for (p, from) in items.iter().zip(cached) {
        let name = std::path::Path::new(&p.name)
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .filter(|n| !n.is_empty() && !n.starts_with('.'))
            .unwrap_or_else(|| format!("photo-{}", p.id));
        let mut to = dir.join(&name);
        if to.exists() {
            to = dir.join(format!("{}-{name}", p.id));
        }
        let ok = std::fs::create_dir_all(&dir).is_ok()
            && (std::fs::hard_link(from, &to).is_ok() || std::fs::copy(from, &to).is_ok());
        out.push(
            if ok { to } else { from.clone() }
                .to_string_lossy()
                .into_owned(),
        );
    }
    out
}

/// Has the phone send the selected items to Downloads/Continuum.
fn save_selected(page: &Rc<Photos>) {
    let ids: Vec<i64> = selected(page).iter().map(|p| p.id).collect();
    if ids.is_empty() {
        return;
    }
    let ctx = page.ctx.clone();
    glib::spawn_future_local(async move {
        match ctx
            .request("photos.send", serde_json::json!({ "ids": ids }))
            .await
        {
            Ok(_) => ctx.toast("Saving to Downloads/Continuum…"),
            Err(e) => ctx.toast(&e.text()),
        }
    });
}
