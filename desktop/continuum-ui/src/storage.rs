//! Phone Storage page: browse the phone's files and folders, open or save any file.

use std::cell::RefCell;
use std::rc::Rc;

use adw::prelude::*;
use serde::Deserialize;

use crate::ctx::{self, Ctx, ReqError};

#[derive(Debug, Clone, Deserialize)]
struct Entry {
    name: String,
    #[serde(default)]
    dir: bool,
    #[serde(default)]
    size: u64,
    #[serde(default)]
    date: i64,
    #[serde(default)]
    mime: String,
}

#[derive(Deserialize)]
struct ListReply {
    entries: Vec<Entry>,
    #[serde(default)]
    truncated: bool,
}

pub struct Storage {
    ctx: Rc<Ctx>,
    pub widget: gtk::Stack,
    list: gtk::ListBox,
    /// Path bar above the list: one button per folder level.
    crumbs: gtk::Box,
    path: RefCell<String>,
    note: gtk::Label,
}

pub fn build(ctx: &Rc<Ctx>) -> Rc<Storage> {
    let widget = gtk::Stack::builder()
        .transition_type(gtk::StackTransitionType::Crossfade)
        .build();
    let list = gtk::ListBox::builder()
        .selection_mode(gtk::SelectionMode::None)
        .css_classes(["boxed-list"])
        .valign(gtk::Align::Start)
        .build();
    let crumbs = gtk::Box::builder()
        .spacing(2)
        .css_classes(["linked"])
        .build();
    let note = gtk::Label::builder()
        .css_classes(["dim-label", "caption"])
        .visible(false)
        .build();
    let content = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(12)
        .margin_top(18)
        .margin_bottom(18)
        .margin_start(18)
        .margin_end(18)
        .build();
    content.append(
        &gtk::ScrolledWindow::builder()
            .child(&crumbs)
            .vscrollbar_policy(gtk::PolicyType::Never)
            .build(),
    );
    content.append(&list);
    content.append(&note);
    let clamp = adw::Clamp::builder()
        .maximum_size(900)
        .child(&content)
        .build();
    widget.add_named(
        &gtk::ScrolledWindow::builder()
            .child(&clamp)
            .hscrollbar_policy(gtk::PolicyType::Never)
            .vexpand(true)
            .build(),
        Some("list"),
    );
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
    Rc::new(Storage {
        ctx: ctx.clone(),
        widget,
        list,
        crumbs,
        path: RefCell::default(),
        note,
    })
}

/// Shows the folder again (Refresh, reconnect), or the top for a new phone.
pub fn reload(page: &Rc<Storage>, from_top: bool) {
    let path = if from_top {
        String::new()
    } else {
        page.path.borrow().clone()
    };
    open_folder(page, path);
}

/// The parent of a `/`-separated path ("" is the top).
fn parent(path: &str) -> String {
    path.rsplit_once('/')
        .map(|(p, _)| p.to_owned())
        .unwrap_or_default()
}

fn join(path: &str, name: &str) -> String {
    if path.is_empty() {
        name.to_owned()
    } else {
        format!("{path}/{name}")
    }
}

fn open_folder(page: &Rc<Storage>, path: String) {
    page.path.replace(path.clone());
    fill_crumbs(page, &path);
    page.widget.set_visible_child_name("loading");
    let page = page.clone();
    glib::spawn_future_local(async move {
        let result = page
            .ctx
            .request("files.list", serde_json::json!({ "path": path }))
            .await
            .and_then(|v| {
                serde_json::from_value::<ListReply>(v).map_err(|e| ReqError::Other(e.to_string()))
            });
        if *page.path.borrow() != path {
            return; // the user moved on
        }
        match result {
            Ok(r) => {
                page.list.remove_all();
                if !path.is_empty() {
                    page.list.append(&up_row(&page, &path));
                }
                for e in &r.entries {
                    page.list.append(&entry_row(&page, &path, e));
                }
                let note = if r.truncated {
                    "This folder is too big to show everything; some items are left out.".to_owned()
                } else if r.entries.is_empty() {
                    "This folder is empty.".to_owned()
                } else {
                    String::new()
                };
                page.note.set_label(&note);
                page.note.set_visible(!note.is_empty());
                page.widget.set_visible_child_name("list");
            }
            Err(e) => {
                let status = ctx::error_page(&e, "Files");
                if let Some(old) = page.widget.child_by_name("status") {
                    page.widget.remove(&old);
                }
                page.widget.add_named(&status, Some("status"));
                page.widget.set_visible_child_name("status");
            }
        }
    });
}

fn fill_crumbs(page: &Rc<Storage>, path: &str) {
    ctx::clear_box(&page.crumbs);
    let mut so_far = String::new();
    let mut levels = vec![(String::new(), "Phone".to_owned())];
    for part in path.split('/').filter(|p| !p.is_empty()) {
        so_far = join(&so_far, part);
        levels.push((so_far.clone(), part.to_owned()));
    }
    let last = levels.len() - 1;
    for (i, (target, label)) in levels.into_iter().enumerate() {
        let b = gtk::Button::builder().label(&label).build();
        if i == 0 {
            b.set_child(Some(
                &adw::ButtonContent::builder()
                    .icon_name("phone-symbolic")
                    .label(&label)
                    .build(),
            ));
        }
        if i == last {
            b.add_css_class("suggested-action");
        }
        let p = page.clone();
        b.connect_clicked(move |_| open_folder(&p, target.clone()));
        page.crumbs.append(&b);
    }
}

fn up_row(page: &Rc<Storage>, path: &str) -> adw::ActionRow {
    let row = adw::ActionRow::builder()
        .title("Back")
        .activatable(true)
        .build();
    row.add_prefix(&gtk::Image::from_icon_name("go-up-symbolic"));
    let (page, up) = (page.clone(), parent(path));
    row.connect_activated(move |_| open_folder(&page, up.clone()));
    row
}

fn icon_for(e: &Entry) -> gtk::Image {
    if e.dir {
        return gtk::Image::from_icon_name("folder");
    }
    let content_type = gtk::gio::content_type_from_mime_type(&e.mime)
        .unwrap_or_else(|| "application/octet-stream".into());
    gtk::Image::from_gicon(&gtk::gio::content_type_get_icon(&content_type))
}

fn entry_row(page: &Rc<Storage>, path: &str, e: &Entry) -> adw::ActionRow {
    let subtitle = if e.dir {
        ctx::short_time(e.date)
    } else {
        format!(
            "{} · {}",
            glib::format_size(e.size),
            ctx::short_time(e.date)
        )
    };
    let row = adw::ActionRow::builder()
        .title(glib::markup_escape_text(&e.name))
        .subtitle(subtitle)
        .activatable(true)
        .build();
    let icon = icon_for(e);
    icon.set_pixel_size(32);
    row.add_prefix(&icon);
    let full = join(path, &e.name);
    if e.dir {
        row.add_suffix(&gtk::Image::from_icon_name("go-next-symbolic"));
        let page = page.clone();
        row.connect_activated(move |_| open_folder(&page, full.clone()));
        return row;
    }
    let version = format!("{}:{}", e.size, e.date);
    let save = gtk::Button::builder()
        .icon_name("folder-download-symbolic")
        .tooltip_text("Save to Downloads")
        .valign(gtk::Align::Center)
        .css_classes(["flat"])
        .build();
    {
        let (page, full, version) = (page.clone(), full.clone(), version.clone());
        save.connect_clicked(move |b| download(&page, &full, &version, true, b));
    }
    row.add_suffix(&save);
    let page = page.clone();
    row.connect_activated(move |r| download(&page, &full, &version, false, r));
    row
}

/// Downloads a file; then opens it, or (with `save`) tells where it went.
fn download(
    page: &Rc<Storage>,
    path: &str,
    version: &str,
    save: bool,
    busy: &impl IsA<gtk::Widget>,
) {
    let (Some(p), Some(d)) = (page.ctx.proxy(), page.ctx.device()) else {
        return;
    };
    let busy = busy.clone().upcast::<gtk::Widget>();
    busy.set_sensitive(false);
    let name = path.rsplit('/').next().unwrap_or(path).to_owned();
    page.ctx.toast(&format!("Downloading {name}…"));
    let (ctx, path, version) = (page.ctx.clone(), path.to_owned(), version.to_owned());
    glib::spawn_future_local(async move {
        let result = p.fetch_file(&d.id, &path, &version, save).await;
        busy.set_sensitive(true);
        match result {
            Ok(local) if save => {
                let toast = adw::Toast::builder()
                    .title(format!("Saved {name} in Downloads/Continuum"))
                    .button_label("Show")
                    .build();
                toast.connect_button_clicked(move |_| {
                    let file = gtk::gio::File::for_path(&local);
                    gtk::FileLauncher::new(Some(&file)).open_containing_folder(
                        gtk::Window::NONE,
                        gtk::gio::Cancellable::NONE,
                        |_| {},
                    );
                });
                ctx.toasts.add_toast(toast);
            }
            Ok(local) => {
                let file = gtk::gio::File::for_path(&local);
                gtk::FileLauncher::new(Some(&file)).launch(
                    Some(&ctx.window),
                    gtk::gio::Cancellable::NONE,
                    |_| {},
                );
            }
            Err(e) => ctx.toast(&format!(
                "Couldn't download {name}: {}",
                ctx::dbus_error(e).text()
            )),
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths() {
        assert_eq!(parent("DCIM/Camera"), "DCIM");
        assert_eq!(parent("DCIM"), "");
        assert_eq!(join("", "DCIM"), "DCIM");
        assert_eq!(join("DCIM", "Camera"), "DCIM/Camera");
    }
}
