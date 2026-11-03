//! Phone-style typing help for reply boxes: a capital at the start of each sentence, "i" → "I",
//! and missing apostrophes ("im" → "I'm", "dont" → "don't") fixed when the word ends.

use std::cell::Cell;
use std::rc::Rc;

use gtk::prelude::*;

/// Words fixed when followed by a space or punctuation. Only ones that are never real words
/// without the apostrophe, so nothing the user meant gets changed.
const FIXES: &[(&str, &str)] = &[
    ("i", "I"),
    ("im", "I'm"),
    ("ive", "I've"),
    ("i'm", "I'm"),
    ("i've", "I've"),
    ("i'll", "I'll"),
    ("i'd", "I'd"),
    ("dont", "don't"),
    ("cant", "can't"),
    ("wont", "won't"),
    ("isnt", "isn't"),
    ("arent", "aren't"),
    ("wasnt", "wasn't"),
    ("werent", "weren't"),
    ("doesnt", "doesn't"),
    ("didnt", "didn't"),
    ("hasnt", "hasn't"),
    ("havent", "haven't"),
    ("hadnt", "hadn't"),
    ("couldnt", "couldn't"),
    ("shouldnt", "shouldn't"),
    ("wouldnt", "wouldn't"),
    ("mustnt", "mustn't"),
    ("youre", "you're"),
    ("youve", "you've"),
    ("youll", "you'll"),
    ("theyre", "they're"),
    ("theyve", "they've"),
    ("thats", "that's"),
    ("whats", "what's"),
    ("wheres", "where's"),
    ("theres", "there's"),
    ("heres", "here's"),
    ("whos", "who's"),
    ("lets", "let's"),
];

/// Ends a word and triggers the fixes.
fn is_boundary(c: char) -> bool {
    c.is_whitespace() || ".,!?;:".contains(c)
}

/// True where a new sentence starts: the very beginning, or after ". ", "! ", "? " or a line break.
fn sentence_start(before: &str) -> bool {
    let t = before.trim_end_matches(' ');
    t.is_empty() || t.ends_with('\n') || t.ends_with(['.', '!', '?']) && t.len() < before.len()
}

/// The corrected text after the user typed the last character of `text` (cursor at the end),
/// or `None` if nothing changes.
pub fn correct(text: &str) -> Option<String> {
    let last = text.chars().last()?;
    let body = &text[..text.len() - last.len_utf8()];
    // A first letter typed at the start of a sentence becomes a capital right away.
    if last.is_lowercase() && sentence_start(body) {
        let mut out = body.to_owned();
        out.extend(last.to_uppercase());
        return Some(out);
    }
    if !is_boundary(last) {
        return None;
    }
    let start = body
        .char_indices()
        .rev()
        .find(|(_, c)| !(c.is_alphabetic() || *c == '\''))
        .map_or(0, |(i, c)| i + c.len_utf8());
    let word = &body[start..];
    let lower = word.to_lowercase();
    let (_, fixed) = FIXES.iter().find(|(w, _)| *w == lower)?;
    // Keep a capital the user typed ("Dont" → "Don't").
    let mut fixed = (*fixed).to_owned();
    if word.starts_with(char::is_uppercase) {
        let mut c = fixed.chars();
        fixed = c
            .next()
            .map(|f| f.to_uppercase().chain(c).collect())
            .unwrap_or_default();
    }
    if fixed == word {
        return None;
    }
    Some(format!("{}{fixed}{last}", &body[..start]))
}

/// Applies [`correct`] as the user types into `entry` (only while typing at the end).
pub fn attach(entry: &gtk::Entry) {
    let busy = Rc::new(Cell::new(false));
    let len = Rc::new(Cell::new(0usize));
    entry.connect_changed(move |e| {
        if busy.get() {
            return;
        }
        let text = e.text().to_string();
        let n = text.chars().count();
        // Only one new character at the end: typing, not pasting, deleting or editing mid-text.
        let typed = n == len.get() + 1 && e.position() as usize == n;
        len.set(n);
        if !typed {
            return;
        }
        if let Some(fixed) = correct(&text) {
            busy.set(true);
            e.set_text(&fixed);
            e.set_position(-1);
            len.set(fixed.chars().count());
            busy.set(false);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Types `s` one character at a time, like a user.
    fn typed(s: &str) -> String {
        let mut text = String::new();
        for c in s.chars() {
            text.push(c);
            if let Some(f) = correct(&text) {
                text = f;
            }
        }
        text
    }

    #[test]
    fn capitals_and_apostrophes() {
        assert_eq!(
            typed("i am fine. how are you? "),
            "I am fine. How are you? "
        );
        assert_eq!(typed("im ok, dont worry "), "I'm ok, don't worry ");
        assert_eq!(typed("yes i think so."), "Yes I think so.");
        assert_eq!(typed("Dont go "), "Don't go ");
        assert_eq!(typed("its 5 pm "), "Its 5 pm ");
        assert_eq!(typed("ok\nsee you "), "Ok\nSee you ");
    }

    #[test]
    fn leaves_other_words_alone() {
        assert_eq!(typed("well, the mail is in "), "Well, the mail is in ");
        assert_eq!(typed("e.g. this "), "E.g. This ");
        assert_eq!(typed("www.site.com "), "Www.site.com ");
    }
}
