//! A drawn battery like the iPhone's: the fill follows the level, red when low, green with a
//! bolt while charging (two bolts on a fast charger).

use std::cell::Cell;
use std::f64::consts::PI;
use std::rc::Rc;

use gtk::prelude::*;

const W: i32 = 32;
const H: i32 = 15;

/// Level 0–100 (or -1 for unknown) and charging.
#[derive(Clone)]
pub struct Battery {
    pub widget: gtk::DrawingArea,
    state: Rc<Cell<(i32, bool)>>,
    fast: Rc<Cell<bool>>,
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
        let fast = Rc::new(Cell::new(false));
        let (s, f) = (state.clone(), fast.clone());
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
                let (cx, cy) = (body_w / 2.0, h / 2.0);
                if f.get() {
                    bolt(cr, cx - 3.2, cy, h - 3.0);
                    bolt(cr, cx + 3.2, cy, h - 3.0);
                } else {
                    bolt(cr, cx, cy, h - 2.0);
                }
            }
        });
        Self {
            widget,
            state,
            fast,
        }
    }

    pub fn set(&self, level: i32, charging: bool) {
        self.state.set((level, charging));
        if !charging {
            self.fast.set(false);
        }
        self.widget.set_visible(level >= 0);
        self.widget.queue_draw();
        self.widget
            .set_tooltip_text(Some(&tooltip(level, charging, 0, false, -1)));
    }

    /// Charging power, charger speed and time to full, when the phone reports them.
    pub fn set_details(&self, watts: u16, fast: bool, full_in: i32) {
        let (level, charging) = self.state.get();
        self.fast.set(charging && fast);
        self.widget.queue_draw();
        self.widget
            .set_tooltip_text(Some(&tooltip(level, charging, watts, fast, full_in)));
    }
}

/// A classic lightning bolt, centred on (cx, cy) and `h` tall, white with a soft edge.
fn bolt(cr: &gtk::cairo::Context, cx: f64, cy: f64, h: f64) {
    const SHAPE: [(f64, f64); 6] = [
        (0.16, -0.5),
        (-0.26, 0.08),
        (-0.02, 0.08),
        (-0.14, 0.5),
        (0.26, -0.08),
        (0.02, -0.08),
    ];
    for (i, (x, y)) in SHAPE.iter().enumerate() {
        let (x, y) = (cx + x * h, cy + y * h);
        if i == 0 {
            cr.move_to(x, y);
        } else {
            cr.line_to(x, y);
        }
    }
    cr.close_path();
    cr.set_source_rgb(1.0, 1.0, 1.0);
    let _ = cr.fill_preserve();
    cr.set_source_rgba(0.0, 0.0, 0.0, 0.3);
    cr.set_line_width(0.5);
    let _ = cr.stroke();
}

/// "25% · Fast charging · 18 W · Full in 1 h 5 min".
fn tooltip(level: i32, charging: bool, watts: u16, fast: bool, full_in: i32) -> String {
    if level < 0 {
        return String::new();
    }
    let mut parts = vec![format!("{level}%")];
    if charging {
        parts.push(if level >= 100 {
            "Charged".to_owned()
        } else if fast {
            "Fast charging".to_owned()
        } else {
            "Charging".to_owned()
        });
        if watts > 0 {
            parts.push(format!("{watts} W"));
        }
        match full_in {
            m if m > 0 && level < 100 => parts.push(format!("Full in {}", duration(m))),
            _ => {}
        }
    }
    parts.join(" · ")
}

fn duration(minutes: i32) -> String {
    match (minutes / 60, minutes % 60) {
        (0, m) => format!("{m} min"),
        (h, 0) => format!("{h} h"),
        (h, m) => format!("{h} h {m} min"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tooltips() {
        assert_eq!(tooltip(-1, false, 0, false, -1), "");
        assert_eq!(tooltip(40, false, 0, false, -1), "40%");
        assert_eq!(tooltip(25, true, 0, false, -1), "25% · Charging");
        assert_eq!(
            tooltip(25, true, 18, true, 65),
            "25% · Fast charging · 18 W · Full in 1 h 5 min"
        );
        assert_eq!(
            tooltip(90, true, 7, false, 20),
            "90% · Charging · 7 W · Full in 20 min"
        );
        assert_eq!(tooltip(100, true, 2, false, 0), "100% · Charged · 2 W");
    }
}
