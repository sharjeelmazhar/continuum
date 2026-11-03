//! The transfers button in the header: shows while files move, with each one's progress,
//! speed and time left.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::time::Duration;

use adw::prelude::*;
use serde::Deserialize;

use crate::ctx::Ctx;

#[derive(Debug, Deserialize)]
struct Row {
    name: String,
    up: bool,
    total: u64,
    done: u64,
    elapsed_ms: u64,
}

/// Speed per transfer, smoothed between polls: (bytes done, smoothed bytes per second).
type Speeds = HashMap<String, (u64, f64)>;

const POLL: Duration = Duration::from_millis(700);

pub fn button(ctx: &Rc<Ctx>) -> gtk::MenuButton {
    let label = gtk::Label::builder().css_classes(["numeric"]).build();
    let content = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    content.append(&gtk::Image::from_icon_name("folder-download-symbolic"));
    content.append(&label);
    let list = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(14)
        .margin_top(12)
        .margin_bottom(12)
        .margin_start(12)
        .margin_end(12)
        .width_request(340)
        .build();
    let button = gtk::MenuButton::builder()
        .child(&content)
        .tooltip_text("Transfers")
        .popover(&gtk::Popover::builder().child(&list).build())
        .visible(false)
        .build();

    let speeds: Rc<RefCell<Speeds>> = Rc::default();
    let (ctx, weak) = (ctx.clone(), button.downgrade());
    glib::timeout_add_local(POLL, move || {
        let Some(button) = weak.upgrade() else {
            return glib::ControlFlow::Break;
        };
        let Some(proxy) = ctx.proxy() else {
            button.set_visible(false);
            return glib::ControlFlow::Continue;
        };
        let (label, list, speeds) = (label.clone(), list.clone(), speeds.clone());
        glib::spawn_future_local(async move {
            let rows: Vec<Row> = proxy
                .transfers()
                .await
                .ok()
                .and_then(|j| serde_json::from_str(&j).ok())
                .unwrap_or_default();
            show(&button, &label, &list, &rows, &mut speeds.borrow_mut());
        });
        glib::ControlFlow::Continue
    });
    button
}

fn show(
    button: &gtk::MenuButton,
    label: &gtk::Label,
    list: &gtk::Box,
    rows: &[Row],
    speeds: &mut Speeds,
) {
    button.set_visible(!rows.is_empty());
    if rows.is_empty() {
        speeds.clear();
        if let Some(p) = button.popover() {
            p.popdown();
        }
        return;
    }
    let (done, total) = rows
        .iter()
        .fold((0, 0), |(d, t), r| (d + r.done, t + r.total));
    label.set_label(&format!("{}%", percent(done, total)));

    crate::ctx::clear_box(list);
    let mut seen = Vec::new();
    for r in rows {
        let key = format!("{}{}", r.up, r.name);
        let rate = {
            let entry = speeds.entry(key.clone()).or_insert((r.done, 0.0));
            let now = (r.done.saturating_sub(entry.0)) as f64 / POLL.as_secs_f64();
            // First sample: the average so far; then smooth so the number doesn't jump.
            entry.1 = if entry.1 == 0.0 {
                r.done as f64 / (r.elapsed_ms.max(1) as f64 / 1000.0)
            } else {
                entry.1 * 0.7 + now * 0.3
            };
            entry.0 = r.done;
            entry.1
        };
        seen.push(key);
        list.append(&row(r, rate));
    }
    speeds.retain(|k, _| seen.contains(k));
}

fn row(r: &Row, rate: f64) -> gtk::Widget {
    let icon = if r.up {
        "send-to-symbolic"
    } else {
        "folder-download-symbolic"
    };
    let title = gtk::Label::builder()
        .label(&r.name)
        .xalign(0.0)
        .hexpand(true)
        .ellipsize(gtk::pango::EllipsizeMode::Middle)
        .css_classes(["heading"])
        .build();
    let top = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    top.append(&gtk::Image::from_icon_name(icon));
    top.append(&title);
    let bar = gtk::ProgressBar::builder()
        .fraction(if r.total == 0 {
            0.0
        } else {
            r.done as f64 / r.total as f64
        })
        .build();
    let detail = gtk::Label::builder()
        .label(detail(r, rate))
        .xalign(0.0)
        .css_classes(["dim-label", "caption", "numeric"])
        .build();
    let b = gtk::Box::new(gtk::Orientation::Vertical, 6);
    b.append(&top);
    b.append(&bar);
    b.append(&detail);
    b.upcast()
}

fn percent(done: u64, total: u64) -> u64 {
    (done * 100).checked_div(total).unwrap_or(0).min(100)
}

/// "To phone · 1.2 GB of 2.1 GB · 57.3 MB/s · 16 s left".
fn detail(r: &Row, rate: f64) -> String {
    let way = if r.up { "To phone" } else { "From phone" };
    let mut s = format!(
        "{way} · {} of {}",
        glib::format_size(r.done),
        glib::format_size(r.total)
    );
    if rate > 1.0 {
        s.push_str(&format!(" · {}/s", glib::format_size(rate as u64)));
        let left = r.total.saturating_sub(r.done) as f64 / rate;
        s.push_str(&format!(" · {} left", time_left(left as u64)));
    }
    s
}

fn time_left(secs: u64) -> String {
    match secs {
        0..=59 => format!("{secs} s"),
        60..=3599 => format!("{} min {} s", secs / 60, secs % 60),
        _ => format!("{} h {} min", secs / 3600, secs / 60 % 60),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers() {
        assert_eq!(percent(50, 200), 25);
        assert_eq!(percent(1, 0), 0);
        assert_eq!(time_left(16), "16 s");
        assert_eq!(time_left(125), "2 min 5 s");
        assert_eq!(time_left(3725), "1 h 2 min");
    }
}
