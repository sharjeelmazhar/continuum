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
const THUMB_PX: u32 = 256;
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
    store: gtk::gio::ListStore,
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
        store: store.clone(),
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
    factory.connect_setup(|_, item| {
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
        let overlay = gtk::Overlay::builder().child(&picture).build();
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

    let selection = gtk::NoSelection::new(Some(store));
    let grid = gtk::GridView::builder()
        .model(&selection)
        .factory(&factory)
        .max_columns(12)
        .min_columns(2)
        .single_click_activate(true)
        .css_classes(["photo-grid"])
        .build();
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
        self.bound.borrow_mut().insert(id, picture.downgrade());
        if let Some(t) = self.textures.borrow().get(&id) {
            picture.set_paintable(Some(t));
            return;
        }
        picture.set_paintable(gdk::Paintable::NONE);
        // Thumbnails from an earlier visit load from disk, without asking the phone.
        let path = ctx::cache_dir("thumbs", &self.device.borrow()).join(format!("{id}.jpg"));
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
        let dir = ctx::cache_dir("thumbs", &self.device.borrow());
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

/// A photo full size, with Copy, Save and Open; videos are saved to the computer instead.
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
            &format!(
                "{} from your phone. It opens in your video player.",
                human_size(photo.size)
            ),
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
        let dialog = dialog.downgrade();
        glib::spawn_future_local(async move {
            let result = ctx.fetch_photo(id).await;
            let Some(d) = dialog.upgrade() else { return };
            match result {
                Ok(path) => {
                    d.close();
                    let file = gtk::gio::File::for_path(&path);
                    gtk::FileLauncher::new(Some(&file)).launch(
                        Some(&ctx.window),
                        gtk::gio::Cancellable::NONE,
                        |_| {},
                    );
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
        let Ok(texture) = gdk::Texture::from_filename(&path) else {
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
