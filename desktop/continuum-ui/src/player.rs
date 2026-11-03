//! Plays a downloaded video inside the app, through GStreamer's GTK 4 sink (the one GNOME's
//! own video player uses). Returns `None` when that sink isn't installed, so the caller can
//! fall back to the user's video player.

use std::cell::Cell;
use std::path::Path;
use std::rc::Rc;

use adw::prelude::*;
use gstreamer as gst;
use gstreamer::prelude::*;

pub struct Player {
    pub widget: gtk::Box,
    playbin: gst::Element,
}

impl Drop for Player {
    fn drop(&mut self) {
        let _ = self.playbin.set_state(gst::State::Null);
    }
}

impl Player {
    pub fn new(path: &Path) -> Option<Rc<Self>> {
        gst::init().ok()?;
        let sink = gst::ElementFactory::make("gtk4paintablesink")
            .build()
            .ok()?;
        let paintable = sink.property::<gtk::gdk::Paintable>("paintable");
        let playbin = gst::ElementFactory::make("playbin")
            .property("uri", gtk::gio::File::for_path(path).uri().as_str())
            .property("video-sink", &sink)
            .build()
            .ok()?;

        let picture = gtk::Picture::builder()
            .paintable(&paintable)
            .content_fit(gtk::ContentFit::Contain)
            .hexpand(true)
            .vexpand(true)
            .build();
        let play = gtk::Button::builder()
            .icon_name("media-playback-pause-symbolic")
            .tooltip_text("Pause")
            .css_classes(["flat", "circular"])
            .build();
        let seek = gtk::Scale::builder()
            .orientation(gtk::Orientation::Horizontal)
            .hexpand(true)
            .adjustment(&gtk::Adjustment::new(0.0, 0.0, 1.0, 1.0, 5.0, 0.0))
            .build();
        let time = gtk::Label::builder()
            .label("0:00 / 0:00")
            .css_classes(["numeric", "dim-label"])
            .build();
        let controls = gtk::Box::builder()
            .spacing(8)
            .margin_start(12)
            .margin_end(12)
            .margin_top(6)
            .margin_bottom(10)
            .build();
        controls.append(&play);
        controls.append(&seek);
        controls.append(&time);
        let widget = gtk::Box::new(gtk::Orientation::Vertical, 0);
        widget.append(&picture);
        widget.append(&controls);

        let player = Rc::new(Self { widget, playbin });
        let playing = Rc::new(Cell::new(true));
        // Set while we move the slider ourselves, so it isn't taken as the user seeking.
        let updating = Rc::new(Cell::new(false));
        {
            let (weak, playing) = (Rc::downgrade(&player), playing.clone());
            play.connect_clicked(move |b| {
                let Some(p) = weak.upgrade() else { return };
                let now = !playing.get();
                playing.set(now);
                p.set_playing(now);
                b.set_icon_name(if now {
                    "media-playback-pause-symbolic"
                } else {
                    "media-playback-start-symbolic"
                });
                b.set_tooltip_text(Some(if now { "Pause" } else { "Play" }));
            });
        }
        {
            let (weak, updating) = (Rc::downgrade(&player), updating.clone());
            seek.connect_value_changed(move |s| {
                if updating.get() {
                    return;
                }
                if let Some(p) = weak.upgrade() {
                    let to = gst::ClockTime::from_mseconds((s.value() * 1000.0) as u64);
                    let _ = p
                        .playbin
                        .seek_simple(gst::SeekFlags::FLUSH | gst::SeekFlags::KEY_UNIT, to);
                }
            });
        }
        {
            let weak = Rc::downgrade(&player);
            glib::timeout_add_local(std::time::Duration::from_millis(250), move || {
                let Some(p) = weak.upgrade() else {
                    return glib::ControlFlow::Break;
                };
                let pos = p.playbin.query_position::<gst::ClockTime>();
                let dur = p.playbin.query_duration::<gst::ClockTime>();
                if let (Some(pos), Some(dur)) = (pos, dur) {
                    updating.set(true);
                    seek.adjustment().set_upper(dur.seconds_f64().max(1.0));
                    seek.set_value(pos.seconds_f64());
                    updating.set(false);
                    time.set_label(&format!("{} / {}", clock(pos), clock(dur)));
                }
                glib::ControlFlow::Continue
            });
        }
        // At the end, go back to the start, paused, like Photos apps do.
        if let Some(bus) = player.playbin.bus() {
            let weak = Rc::downgrade(&player);
            let watch = bus.add_watch_local(move |_, msg| {
                let Some(p) = weak.upgrade() else {
                    return glib::ControlFlow::Break;
                };
                if let gst::MessageView::Eos(_) = msg.view() {
                    playing.set(false);
                    p.set_playing(false);
                    let _ = p
                        .playbin
                        .seek_simple(gst::SeekFlags::FLUSH, gst::ClockTime::ZERO);
                    play.set_icon_name("media-playback-start-symbolic");
                    play.set_tooltip_text(Some("Play"));
                }
                glib::ControlFlow::Continue
            });
            // The watch lives as long as the widget.
            if let Ok(guard) = watch {
                let keep = std::cell::RefCell::new(Some(guard));
                player.widget.connect_destroy(move |_| {
                    keep.take();
                });
            }
        }
        player.set_playing(true);
        Some(player)
    }

    pub fn set_playing(&self, on: bool) {
        let _ = self.playbin.set_state(if on {
            gst::State::Playing
        } else {
            gst::State::Paused
        });
    }
}

fn clock(t: gst::ClockTime) -> String {
    let s = t.seconds();
    if s >= 3600 {
        format!("{}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60)
    } else {
        format!("{}:{:02}", s / 60, s % 60)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clock_format() {
        assert_eq!(clock(gst::ClockTime::from_seconds(4)), "0:04");
        assert_eq!(clock(gst::ClockTime::from_seconds(125)), "2:05");
        assert_eq!(clock(gst::ClockTime::from_seconds(3725)), "1:02:05");
    }
}
