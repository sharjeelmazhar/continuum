//! A drawn battery like the iPhone's: the fill follows the level, red when low, green with a
//! bolt while charging.

use std::cell::Cell;
use std::f64::consts::PI;
use std::rc::Rc;

use gtk::prelude::*;

const W: i32 = 34;
/// Taller than the battery itself, so the bolt can stand out above and below it.
const H: i32 = 19;
/// The battery body's height inside that.
const BODY_H: f64 = 15.0;

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
            let fg = (fg.red() as f64, fg.green() as f64, fg.blue() as f64);
            paint(cr, w, h, level, charging, fg);
        });
        Self { widget, state }
    }

    pub fn set(&self, level: i32, charging: bool) {
        self.state.set((level, charging));
        self.widget.set_visible(level >= 0);
        self.widget.queue_draw();
        self.widget
            .set_tooltip_text(Some(&tooltip(level, charging, 0, false, -1)));
    }

    /// Charging power, charger speed and time to full, when the phone reports them.
    pub fn set_details(&self, watts: u16, fast: bool, full_in: i32) {
        let (level, charging) = self.state.get();
        self.widget
            .set_tooltip_text(Some(&tooltip(level, charging, watts, fast, full_in)));
    }
}

/// Draws the battery into a `w` × `h` area in the text colour `fg`.
fn paint(
    cr: &gtk::cairo::Context,
    w: i32,
    h: i32,
    level: i32,
    charging: bool,
    fg: (f64, f64, f64),
) {
    let (fr, fgc, fb) = fg;
    let body_w = w as f64 - 3.0;
    let full_h = h as f64;
    let top = (full_h - BODY_H) / 2.0;
    let h = BODY_H;
    // Outline and the nub on the right.
    cr.set_source_rgba(fr, fgc, fb, 0.45);
    cr.set_line_width(1.2);
    rounded(cr, 0.6, top + 0.6, body_w - 1.2, h - 1.2, 3.5);
    let _ = cr.stroke();
    rounded(cr, body_w + 0.5, top + h * 0.32, 2.0, h * 0.36, 1.0);
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
    rounded(cr, 2.2, top + 2.2, width, h - 4.4, 2.0);
    let _ = cr.fill();
    if charging {
        // A bolt that pokes just past the battery's top and bottom edges.
        bolt(cr, body_w / 2.0, full_h / 2.0, full_h - 1.0, (fr, fgc, fb));
    }
}

/// A classic lightning bolt, centred on (cx, cy) and `h` tall: white, edged in the text colour
/// so it reads on the green fill and on the window around the battery alike.
fn bolt(cr: &gtk::cairo::Context, cx: f64, cy: f64, h: f64, edge: (f64, f64, f64)) {
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
    cr.set_source_rgba(edge.0, edge.1, edge.2, 0.85);
    cr.set_line_width(1.1);
    cr.set_line_join(gtk::cairo::LineJoin::Round);
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

    /// Draws every state, big, on light and dark backgrounds, so a change can be looked at:
    /// CONTINUUM_BATTERY_PREVIEW=out.ppm cargo test -p continuum-ui battery_preview
    #[test]
    fn battery_preview() {
        let Ok(out) = std::env::var("CONTINUUM_BATTERY_PREVIEW") else {
            return;
        };
        const S: i32 = 8;
        let states = [
            (60, false, false),
            (60, true, false),
            (60, true, true),
            (15, false, false),
        ];
        let (cw, ch) = (W * S + 40, H * S + 40);
        let (iw, ih) = (cw * states.len() as i32, ch * 2);
        let mut img = gtk::cairo::ImageSurface::create(gtk::cairo::Format::Rgb24, iw, ih).unwrap();
        {
            let cr = gtk::cairo::Context::new(&img).unwrap();
            for (row, (bg, fg)) in [(0.98, 0.15), (0.14, 0.95)].into_iter().enumerate() {
                cr.set_source_rgb(bg, bg, bg);
                cr.rectangle(0.0, (row as i32 * ch) as f64, iw as f64, ch as f64);
                let _ = cr.fill();
                for (i, (level, charging, _)) in states.into_iter().enumerate() {
                    cr.save().unwrap();
                    cr.translate((i as i32 * cw + 20) as f64, (row as i32 * ch + 20) as f64);
                    cr.scale(S as f64, S as f64);
                    paint(&cr, W, H, level, charging, (fg, fg, fg));
                    cr.restore().unwrap();
                }
            }
        }
        let stride = img.stride() as usize;
        let data = img.data().unwrap();
        let mut ppm = format!("P6 {iw} {ih} 255\n").into_bytes();
        for y in 0..ih as usize {
            for x in 0..iw as usize {
                let p = &data[y * stride + x * 4..][..4];
                ppm.extend([p[2], p[1], p[0]]);
            }
        }
        std::fs::write(out, ppm).unwrap();
    }

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
