//! A drawn battery like the iPhone's: the fill follows the level, red when low, green with a
//! bolt while charging.

use std::cell::Cell;
use std::f64::consts::PI;
use std::rc::Rc;

use gtk::prelude::*;

const W: i32 = 30;
const H: i32 = 14;

/// Level 0–100 (or -1 for unknown) and charging.
#[derive(Clone)]
pub struct Battery {
    pub widget: gtk::DrawingArea,
    state: Rc<Cell<(i32, bool)>>,
}

fn rounded(cr: &gtk::cairo::Context, x: f64, y: f64, w: f64, h: f64, r: f64) {
    cr.new_sub_path();
    cr.arc(x + w - r, y + r, r, -PI / 2.0, 0.0);
    cr.arc(x + w - r, y + h - r, r, 0.0, PI / 2.0);
    cr.arc(x + r, y + h - r, r, PI / 2.0, PI);
    cr.arc(x + r, y + r, r, PI, 1.5 * PI);
    cr.close_path();
}

impl Battery {
    pub fn new() -> Self {
        let widget = gtk::DrawingArea::builder()
            .content_width(W)
            .content_height(H)
            .valign(gtk::Align::Center)
            .build();
        let state = Rc::new(Cell::new((-1, false)));
        let s = state.clone();
        widget.set_draw_func(move |area, cr, w, h| {
            let (level, charging) = s.get();
            let fg = area.color();
            let (fr, fgc, fb) = (fg.red() as f64, fg.green() as f64, fg.blue() as f64);
            let body_w = w as f64 - 3.0;
            let h = h as f64;
            // Outline and the nub on the right.
            cr.set_source_rgba(fr, fgc, fb, 0.45);
            cr.set_line_width(1.2);
            rounded(cr, 0.6, 0.6, body_w - 1.2, h - 1.2, 3.5);
            let _ = cr.stroke();
            rounded(cr, body_w + 0.5, h * 0.32, 2.0, h * 0.36, 1.0);
            let _ = cr.fill();
            if level < 0 {
                return;
            }
            // The fill, inset from the outline.
            let inner = body_w - 4.4;
            let width = (inner * f64::from(level.clamp(0, 100)) / 100.0).max(1.5);
            match (charging, level) {
                (true, _) => cr.set_source_rgb(0.20, 0.78, 0.35), // green
                (false, l) if l <= 20 => cr.set_source_rgb(1.0, 0.23, 0.19), // red
                _ => cr.set_source_rgba(fr, fgc, fb, 0.95),
            }
            rounded(cr, 2.2, 2.2, width, h - 4.4, 2.0);
            let _ = cr.fill();
            if charging {
                // A bolt across the middle, cut out with the window background around it.
                let (cx, cy) = (body_w / 2.0, h / 2.0);
                let bolt = [
                    (cx + 1.2, cy - 5.6),
                    (cx - 3.4, cy + 0.9),
                    (cx - 0.2, cy + 0.9),
                    (cx - 1.2, cy + 5.6),
                    (cx + 3.4, cy - 0.9),
                    (cx + 0.2, cy - 0.9),
                ];
                cr.move_to(bolt[0].0, bolt[0].1);
                for (x, y) in &bolt[1..] {
                    cr.line_to(*x, *y);
                }
                cr.close_path();
                cr.set_source_rgb(1.0, 1.0, 1.0);
                let _ = cr.fill_preserve();
                cr.set_source_rgba(0.0, 0.0, 0.0, 0.35);
                cr.set_line_width(0.6);
                let _ = cr.stroke();
            }
        });
        Self { widget, state }
    }

    pub fn set(&self, level: i32, charging: bool) {
        self.state.set((level, charging));
        self.widget.set_visible(level >= 0);
        self.widget.queue_draw();
        let tip = match (level, charging) {
            (l, _) if l < 0 => String::new(),
            (l, true) => format!("{l}%, charging"),
            (l, false) => format!("{l}%"),
        };
        self.widget.set_tooltip_text(Some(&tip));
    }
}
