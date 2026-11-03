//! Messages page: the phone's SMS conversations on the left, the open one as chat bubbles on
//! the right, and a box to reply or start a new conversation.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use adw::prelude::*;
use serde::Deserialize;

use crate::ctx::{self, Ctx, ReqError};

#[derive(Debug, Clone, Deserialize)]
struct Thread {
    thread: i64,
    #[serde(default)]
    address: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    snippet: String,
    #[serde(default)]
    date: i64,
    #[serde(default)]
    unread: u32,
    #[serde(default)]
    out: bool,
}

impl Thread {
    fn title(&self) -> &str {
        if self.name.is_empty() {
            &self.address
        } else {
            &self.name
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
struct Sms {
    #[serde(default)]
    body: String,
    #[serde(default)]
    date: i64,
    #[serde(default)]
    out: bool,
    #[serde(default)]
    status: String,
}

#[derive(Deserialize)]
struct ThreadsReply {
    threads: Vec<Thread>,
}

#[derive(Deserialize)]
struct MessagesReply {
    messages: Vec<Sms>,
}

/// The conversation on the right: an existing thread, or a new one to a number.
#[derive(Debug, Clone, PartialEq)]
struct Open {
    thread: Option<i64>,
    address: String,
    title: String,
}

pub struct Messages {
    ctx: Rc<Ctx>,
    pub widget: gtk::Stack,
    list: gtk::ListBox,
    threads: RefCell<Vec<Thread>>,
    open: RefCell<Option<Open>>,
    chat_title: adw::WindowTitle,
    bubbles: gtk::Box,
    chat_scroll: gtk::ScrolledWindow,
    chat_stack: gtk::Stack,
    entry: gtk::Entry,
    send: gtk::Button,
    generation: Cell<u64>,
    /// Ignore row selection while the list is being rebuilt.
    rebuilding: Cell<bool>,
}

pub fn build(ctx: &Rc<Ctx>) -> Rc<Messages> {
    let widget = gtk::Stack::builder()
        .transition_type(gtk::StackTransitionType::Crossfade)
        .build();
    let list = gtk::ListBox::builder()
        .selection_mode(gtk::SelectionMode::Single)
        .css_classes(["navigation-sidebar", "threads"])
        .build();
    let new_button = gtk::Button::builder()
        .label("New Message")
        .css_classes(["pill", "suggested-action"])
        .margin_top(8)
        .margin_bottom(8)
        .margin_start(12)
        .margin_end(12)
        .build();
    let left = gtk::Box::new(gtk::Orientation::Vertical, 0);
    left.append(&new_button);
    left.append(
        &gtk::ScrolledWindow::builder()
            .child(&list)
            .hscrollbar_policy(gtk::PolicyType::Never)
            .vexpand(true)
            .build(),
    );
    left.set_width_request(300);
    left.add_css_class("thread-pane");

    let chat_title = adw::WindowTitle::new("", "");
    let bubbles = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(4)
        .margin_top(12)
        .margin_bottom(12)
        .margin_start(16)
        .margin_end(16)
        .valign(gtk::Align::End)
        .build();
    let clamp = adw::Clamp::builder()
        .maximum_size(760)
        .child(&bubbles)
        .build();
    let chat_scroll = gtk::ScrolledWindow::builder()
        .child(&clamp)
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vexpand(true)
        .build();
    let entry = gtk::Entry::builder()
        .placeholder_text("Text message")
        .hexpand(true)
        .max_length(5000)
        .build();
    let send = gtk::Button::builder()
        .icon_name("mail-send-symbolic")
        .tooltip_text("Send")
        .css_classes(["circular", "suggested-action"])
        .sensitive(false)
        .build();
    let composer = gtk::Box::builder()
        .spacing(8)
        .margin_top(8)
        .margin_bottom(12)
        .margin_start(16)
        .margin_end(16)
        .build();
    composer.append(&entry);
    composer.append(&send);
    let title_bar = gtk::CenterBox::builder()
        .center_widget(&chat_title)
        .margin_top(8)
        .margin_bottom(8)
        .build();
    let chat = gtk::Box::new(gtk::Orientation::Vertical, 0);
    chat.append(&title_bar);
    chat.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
    chat.append(&chat_scroll);
    chat.append(&composer);
    let chat_stack = gtk::Stack::new();
    chat_stack.add_named(
        &ctx::status_page(
            ctx::icon(&["chat-bubble-text-symbolic", "mail-unread-symbolic"]),
            "Your Messages",
            "Pick a conversation, or start a new one.",
        ),
        Some("none"),
    );
    chat_stack.add_named(&chat, Some("chat"));
    chat_stack.set_hexpand(true);

    let split = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    split.append(&left);
    split.append(&gtk::Separator::new(gtk::Orientation::Vertical));
    split.append(&chat_stack);
    widget.add_named(&split, Some("main"));
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

    let page = Rc::new(Messages {
        ctx: ctx.clone(),
        widget,
        list: list.clone(),
        threads: RefCell::default(),
        open: RefCell::default(),
        chat_title,
        bubbles,
        chat_scroll,
        chat_stack,
        entry: entry.clone(),
        send: send.clone(),
        generation: Cell::new(0),
        rebuilding: Cell::new(false),
    });

    {
        let page = Rc::downgrade(&page);
        list.connect_row_selected(move |_, row| {
            let (Some(page), Some(row)) = (page.upgrade(), row) else {
                return;
            };
            if page.rebuilding.get() {
                return;
            }
            let t = page.threads.borrow().get(row.index() as usize).cloned();
            if let Some(t) = t {
                let open = Open {
                    thread: Some(t.thread),
                    address: t.address.clone(),
                    title: t.title().to_owned(),
                };
                show_conversation(&page, open);
            }
        });
    }
    {
        let send = send.clone();
        entry.connect_changed(move |e| send.set_sensitive(!e.text().trim().is_empty()));
    }
    for activate in [Some(&entry), None] {
        let page = Rc::downgrade(&page);
        let go = move || {
            if let Some(page) = page.upgrade() {
                send_message(&page);
            }
        };
        match activate {
            Some(e) => {
                e.connect_activate(move |_| go());
            }
            None => {
                send.connect_clicked(move |_| go());
            }
        }
    }
    {
        let page = Rc::downgrade(&page);
        new_button.connect_clicked(move |_| {
            if let Some(page) = page.upgrade() {
                new_conversation(&page);
            }
        });
    }
    page
}

/// Reloads the conversation list (and the open conversation).
pub fn reload(page: &Rc<Messages>, from_scratch: bool) {
    if from_scratch {
        page.open.replace(None);
        page.chat_stack.set_visible_child_name("none");
        page.widget.set_visible_child_name("loading");
    }
    page.generation.set(page.generation.get() + 1);
    let generation = page.generation.get();
    let page = page.clone();
    glib::spawn_future_local(async move {
        let result = page
            .ctx
            .request("sms.threads", serde_json::json!({ "limit": 100 }))
            .await
            .and_then(|v| {
                serde_json::from_value::<ThreadsReply>(v)
                    .map_err(|e| ReqError::Other(e.to_string()))
            });
        if page.generation.get() != generation {
            return;
        }
        match result {
            Ok(r) => {
                fill_threads(&page, r.threads);
                page.widget.set_visible_child_name("main");
                let open = page.open.borrow().clone();
                if let Some(open) = open {
                    load_conversation(&page, open, false);
                }
            }
            Err(e) => {
                let status = ctx::error_page(&e, "Messages");
                if let Some(old) = page.widget.child_by_name("status") {
                    page.widget.remove(&old);
                }
                page.widget.add_named(&status, Some("status"));
                page.widget.set_visible_child_name("status");
            }
        }
    });
}

fn fill_threads(page: &Rc<Messages>, threads: Vec<Thread>) {
    page.rebuilding.set(true);
    page.list.remove_all();
    let open = page.open.borrow().clone();
    // A new conversation gets its thread id once the first message is in the phone's database.
    let mut selected = None;
    for (i, t) in threads.iter().enumerate() {
        page.list.append(&thread_row(t));
        if let Some(o) = &open
            && (o.thread == Some(t.thread)
                || (o.thread.is_none() && same_number(&o.address, &t.address)))
        {
            selected = Some(i);
        }
    }
    if let (Some(i), Some(o)) = (selected, open) {
        if let Some(row) = page.list.row_at_index(i as i32) {
            page.list.select_row(Some(&row));
        }
        page.open.replace(Some(Open {
            thread: Some(threads[i].thread),
            ..o
        }));
    }
    let first = page.open.borrow().is_none() && !threads.is_empty();
    *page.threads.borrow_mut() = threads;
    page.rebuilding.set(false);
    // Like a chat app: open the newest conversation right away.
    if first && let Some(row) = page.list.row_at_index(0) {
        page.list.select_row(Some(&row));
    }
}

fn same_number(a: &str, b: &str) -> bool {
    let digits = |s: &str| -> String { s.chars().filter(char::is_ascii_digit).collect() };
    let (a, b) = (digits(a), digits(b));
    // Ignore country prefixes: compare the last 9 digits.
    !a.is_empty() && a.len().min(b.len()) >= 6 && {
        let n = a.len().min(b.len()).min(9);
        a[a.len() - n..] == b[b.len() - n..]
    }
}

fn thread_row(t: &Thread) -> gtk::ListBoxRow {
    let avatar = adw::Avatar::new(40, Some(t.title()), !t.name.is_empty());
    let name = gtk::Label::builder()
        .label(t.title())
        .xalign(0.0)
        .ellipsize(gtk::pango::EllipsizeMode::End)
        .hexpand(true)
        .css_classes(["heading"])
        .build();
    let time = gtk::Label::builder()
        .label(ctx::short_time(t.date))
        .css_classes(["dim-label", "caption"])
        .build();
    let snippet = gtk::Label::builder()
        .label(if t.out {
            format!("You: {}", t.snippet)
        } else {
            t.snippet.clone()
        })
        .xalign(0.0)
        .ellipsize(gtk::pango::EllipsizeMode::End)
        .single_line_mode(true)
        .css_classes(if t.unread > 0 {
            vec!["unread"]
        } else {
            vec!["dim-label"]
        })
        .build();
    let top = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    top.append(&name);
    top.append(&time);
    let bottom = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    bottom.append(&snippet);
    snippet.set_hexpand(true);
    if t.unread > 0 {
        bottom.append(
            &gtk::Label::builder()
                .label(t.unread.to_string())
                .valign(gtk::Align::Center)
                .halign(gtk::Align::End)
                .css_classes(["badge"])
                .build(),
        );
    }
    let text = gtk::Box::new(gtk::Orientation::Vertical, 2);
    text.append(&top);
    text.append(&bottom);
    let row = gtk::Box::builder()
        .spacing(12)
        .margin_top(8)
        .margin_bottom(8)
        .margin_start(4)
        .margin_end(4)
        .build();
    row.append(&avatar);
    row.append(&text);
    gtk::ListBoxRow::builder().child(&row).build()
}

fn show_conversation(page: &Rc<Messages>, open: Open) {
    page.chat_title.set_title(&open.title);
    page.chat_title.set_subtitle(if open.title == open.address {
        ""
    } else {
        &open.address
    });
    page.chat_stack.set_visible_child_name("chat");
    ctx::clear_box(&page.bubbles);
    page.open.replace(Some(open.clone()));
    page.entry.grab_focus();
    load_conversation(page, open, true);
}

fn load_conversation(page: &Rc<Messages>, open: Open, scroll: bool) {
    let Some(thread) = open.thread else { return };
    let page = page.clone();
    glib::spawn_future_local(async move {
        let body = serde_json::json!({ "thread": thread, "before": 0, "limit": 100 });
        let result = page.ctx.request("sms.messages", body).await.and_then(|v| {
            serde_json::from_value::<MessagesReply>(v).map_err(|e| ReqError::Other(e.to_string()))
        });
        if page.open.borrow().as_ref().and_then(|o| o.thread) != Some(thread) {
            return;
        }
        match result {
            Ok(r) => {
                ctx::clear_box(&page.bubbles);
                let mut last_date = 0;
                for m in &r.messages {
                    // A time line when more than an hour passed since the previous message.
                    if m.date - last_date > 3_600_000 {
                        page.bubbles.append(
                            &gtk::Label::builder()
                                .label(ctx::long_time(m.date))
                                .css_classes(["dim-label", "caption"])
                                .margin_top(10)
                                .margin_bottom(4)
                                .build(),
                        );
                    }
                    last_date = m.date;
                    page.bubbles.append(&bubble(m));
                }
                if scroll || at_bottom(&page.chat_scroll) {
                    scroll_to_end(&page.chat_scroll);
                }
            }
            Err(e) => page.ctx.toast(&e.text()),
        }
    });
}

fn bubble(m: &Sms) -> gtk::Widget {
    let label = gtk::Label::builder()
        .label(&m.body)
        .wrap(true)
        .wrap_mode(gtk::pango::WrapMode::WordChar)
        .xalign(0.0)
        .selectable(true)
        .max_width_chars(48)
        .css_classes(["bubble", if m.out { "out" } else { "in" }])
        .halign(if m.out {
            gtk::Align::End
        } else {
            gtk::Align::Start
        })
        .tooltip_text(ctx::long_time(m.date))
        .build();
    if !m.out || m.status == "sent" || m.status.is_empty() {
        return label.upcast();
    }
    let note = gtk::Label::builder()
        .label(match m.status.as_str() {
            "failed" => "Not sent",
            _ => "Sending…",
        })
        .halign(gtk::Align::End)
        .css_classes(if m.status == "failed" {
            vec!["error", "caption"]
        } else {
            vec!["dim-label", "caption"]
        })
        .build();
    let b = gtk::Box::new(gtk::Orientation::Vertical, 2);
    b.append(&label);
    b.append(&note);
    b.upcast()
}

fn at_bottom(s: &gtk::ScrolledWindow) -> bool {
    let a = s.vadjustment();
    a.value() + a.page_size() >= a.upper() - 40.0
}

fn scroll_to_end(s: &gtk::ScrolledWindow) {
    let s = s.clone();
    // After layout, so the new height is known.
    glib::timeout_add_local_once(Duration::from_millis(50), move || {
        let a = s.vadjustment();
        a.set_value(a.upper());
    });
}

fn send_message(page: &Rc<Messages>) {
    let text = page.entry.text().trim().to_owned();
    let Some(open) = page.open.borrow().clone() else {
        return;
    };
    if text.is_empty() {
        return;
    }
    page.send.set_sensitive(false);
    page.entry.set_sensitive(false);
    // Shown at once; the phone's copy replaces it when the list reloads.
    page.bubbles.append(&bubble(&Sms {
        body: text.clone(),
        date: glib::real_time() / 1000,
        out: true,
        status: "sending".into(),
    }));
    scroll_to_end(&page.chat_scroll);
    let page = page.clone();
    glib::spawn_future_local(async move {
        let body = serde_json::json!({ "address": open.address, "body": text });
        let result = page.ctx.request("sms.send", body).await;
        page.entry.set_sensitive(true);
        match result {
            Ok(_) => {
                page.entry.set_text("");
                page.entry.grab_focus();
                // sms.changed reloads too; this covers phones that report late.
                glib::timeout_future(Duration::from_millis(1500)).await;
                reload(&page, false);
            }
            Err(e) => {
                page.send.set_sensitive(true);
                page.ctx.toast(&format!("Couldn't send: {}", e.text()));
                load_conversation(&page, open, true);
            }
        }
    });
}

fn new_conversation(page: &Rc<Messages>) {
    let dialog = adw::AlertDialog::new(Some("New Message"), Some("Enter a phone number."));
    let entry = gtk::Entry::builder()
        .placeholder_text("Phone number")
        .input_purpose(gtk::InputPurpose::Phone)
        .activates_default(true)
        .build();
    dialog.set_extra_child(Some(&entry));
    dialog.add_responses(&[("cancel", "Cancel"), ("start", "Start")]);
    dialog.set_response_appearance("start", adw::ResponseAppearance::Suggested);
    dialog.set_default_response(Some("start"));
    dialog.set_close_response("cancel");
    let window = page.ctx.window.clone();
    let page = page.clone();
    dialog.connect_response(None, move |_, r| {
        let number: String = entry
            .text()
            .chars()
            .filter(|c| c.is_ascii_digit() || *c == '+')
            .collect();
        if r != "start" || number.len() < 3 {
            return;
        }
        let existing = page
            .threads
            .borrow()
            .iter()
            .position(|t| same_number(&t.address, &number));
        match existing.and_then(|i| page.list.row_at_index(i as i32)) {
            Some(row) => page.list.select_row(Some(&row)),
            None => {
                page.list.unselect_all();
                show_conversation(
                    &page,
                    Open {
                        thread: None,
                        address: number.clone(),
                        title: number,
                    },
                );
            }
        }
    });
    dialog.present(Some(&window));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_match_with_or_without_country_code() {
        assert!(same_number("+92 300 1234567", "03001234567"));
        assert!(same_number("5551234", "555-1234"));
        assert!(!same_number("5551234", "5551235"));
        assert!(!same_number("", ""));
        assert!(!same_number("123", "123"));
    }
}
