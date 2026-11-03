//! Recognises popular phones from the model string they send ("samsung SM-S928B",
//! "Google Pixel 9 Pro") or their user-visible name ("Galaxy S24 Ultra"), and picks a matching
//! picture. The pictures are original drawings from scripts/render-phone-pictures.py.

/// Samsung model codes: the four characters after "SM-" (region suffixes are ignored).
#[rustfmt::skip]
const SAMSUNG: &[(&str, &str)] = &[
    ("G980", "Galaxy S20"), ("G981", "Galaxy S20"), ("G985", "Galaxy S20+"), ("G986", "Galaxy S20+"),
    ("G988", "Galaxy S20 Ultra"), ("G780", "Galaxy S20 FE"), ("G781", "Galaxy S20 FE"),
    ("G991", "Galaxy S21"), ("G996", "Galaxy S21+"), ("G998", "Galaxy S21 Ultra"), ("G990", "Galaxy S21 FE"),
    ("S901", "Galaxy S22"), ("S906", "Galaxy S22+"), ("S908", "Galaxy S22 Ultra"),
    ("S911", "Galaxy S23"), ("S916", "Galaxy S23+"), ("S918", "Galaxy S23 Ultra"), ("S711", "Galaxy S23 FE"),
    ("S921", "Galaxy S24"), ("S926", "Galaxy S24+"), ("S928", "Galaxy S24 Ultra"), ("S721", "Galaxy S24 FE"),
    ("S931", "Galaxy S25"), ("S936", "Galaxy S25+"), ("S938", "Galaxy S25 Ultra"), ("S937", "Galaxy S25 Edge"),
    ("S731", "Galaxy S25 FE"),
    ("N980", "Galaxy Note20"), ("N981", "Galaxy Note20"), ("N985", "Galaxy Note20 Ultra"),
    ("N986", "Galaxy Note20 Ultra"),
    ("F916", "Galaxy Z Fold2"), ("F926", "Galaxy Z Fold3"), ("F936", "Galaxy Z Fold4"),
    ("F946", "Galaxy Z Fold5"), ("F956", "Galaxy Z Fold6"), ("F966", "Galaxy Z Fold7"),
    ("F700", "Galaxy Z Flip"), ("F707", "Galaxy Z Flip"), ("F711", "Galaxy Z Flip3"),
    ("F721", "Galaxy Z Flip4"), ("F731", "Galaxy Z Flip5"), ("F741", "Galaxy Z Flip6"),
    ("F766", "Galaxy Z Flip7"), ("F761", "Galaxy Z Flip7 FE"),
    ("A515", "Galaxy A51"), ("A516", "Galaxy A51"), ("A525", "Galaxy A52"), ("A526", "Galaxy A52"),
    ("A528", "Galaxy A52s"), ("A536", "Galaxy A53"), ("A546", "Galaxy A54"), ("A556", "Galaxy A55"),
    ("A566", "Galaxy A56"), ("A315", "Galaxy A31"), ("A325", "Galaxy A32"), ("A326", "Galaxy A32"),
    ("A336", "Galaxy A33"), ("A346", "Galaxy A34"), ("A356", "Galaxy A35"), ("A366", "Galaxy A36"),
    ("A125", "Galaxy A12"), ("A135", "Galaxy A13"), ("A137", "Galaxy A13"), ("A145", "Galaxy A14"),
    ("A146", "Galaxy A14"), ("A155", "Galaxy A15"), ("A156", "Galaxy A15"), ("A165", "Galaxy A16"),
    ("A166", "Galaxy A16"), ("A715", "Galaxy A71"), ("A716", "Galaxy A71"), ("A725", "Galaxy A72"),
    ("A736", "Galaxy A73"), ("A217", "Galaxy A21s"), ("A225", "Galaxy A22"), ("A226", "Galaxy A22"),
    ("A235", "Galaxy A23"), ("A236", "Galaxy A23"), ("A245", "Galaxy A24"), ("A256", "Galaxy A25"),
];

/// Other makers' model codes (`Build.MODEL`), matched as a whole word.
#[rustfmt::skip]
const CODES: &[(&str, &str)] = &[
    ("2201117TG", "Redmi Note 11"), ("2201117TY", "Redmi Note 11"), ("2201117TI", "Redmi Note 11"),
    ("2201116TG", "Redmi Note 11 Pro"), ("2201116SG", "Redmi Note 11 Pro 5G"),
    ("M2101K7AG", "Redmi Note 10"), ("M2101K6G", "Redmi Note 10 Pro"),
    ("M2007J3SY", "Mi 10T"), ("M2007J3SG", "Mi 10T Pro"), ("M2011K2G", "Mi 11"),
    ("2201122G", "Xiaomi 12"), ("2201123G", "Xiaomi 12 Pro"), ("2211133G", "Xiaomi 13"),
    ("2210132G", "Xiaomi 13 Pro"), ("2304FPN6DG", "Xiaomi 13 Ultra"), ("23127PN0CG", "Xiaomi 14"),
    ("24030PN60G", "Xiaomi 14 Ultra"), ("M2012K11AG", "POCO F3"), ("22021211RG", "POCO F4"),
    ("IN2010", "OnePlus 8"), ("IN2013", "OnePlus 8"), ("IN2015", "OnePlus 8"),
    ("IN2020", "OnePlus 8 Pro"), ("IN2023", "OnePlus 8 Pro"), ("IN2025", "OnePlus 8 Pro"),
    ("KB2001", "OnePlus 8T"), ("KB2003", "OnePlus 8T"), ("KB2005", "OnePlus 8T"),
    ("LE2110", "OnePlus 9"), ("LE2111", "OnePlus 9"), ("LE2113", "OnePlus 9"), ("LE2115", "OnePlus 9"),
    ("LE2120", "OnePlus 9 Pro"), ("LE2121", "OnePlus 9 Pro"), ("LE2123", "OnePlus 9 Pro"),
    ("LE2125", "OnePlus 9 Pro"), ("NE2210", "OnePlus 10 Pro"), ("NE2213", "OnePlus 10 Pro"),
    ("NE2215", "OnePlus 10 Pro"), ("CPH2447", "OnePlus 11"), ("CPH2449", "OnePlus 11"),
    ("CPH2581", "OnePlus 12"), ("CPH2653", "OnePlus 13"), ("AC2001", "OnePlus Nord"),
    ("AC2003", "OnePlus Nord"), ("DN2101", "OnePlus Nord 2"), ("DN2103", "OnePlus Nord 2"),
    ("CPH2173", "OPPO Find X3 Pro"), ("CPH2305", "OPPO Find X5 Pro"),
    ("A063", "Nothing Phone (1)"), ("A065", "Nothing Phone (2)"), ("A142", "Nothing Phone (2a)"),
];

/// (words to find, marketing name, picture). Words are compared after `normalize`, so
/// "S24+" also matches "Galaxy S24 Plus" and "Pixel 9" doesn't match "Pixel 9a".
#[rustfmt::skip]
const NAMES: &[(&str, &str, &str)] = &[
    ("S20", "Galaxy S20", "galaxy-s20"), ("S20+", "Galaxy S20+", "galaxy-s20"),
    ("S20 FE", "Galaxy S20 FE", "galaxy-s20"), ("S20 Ultra", "Galaxy S20 Ultra", "galaxy-s20-ultra"),
    ("S21", "Galaxy S21", "galaxy-s21"), ("S21+", "Galaxy S21+", "galaxy-s21"),
    ("S21 FE", "Galaxy S21 FE", "galaxy-s21"), ("S21 Ultra", "Galaxy S21 Ultra", "galaxy-s21-ultra"),
    ("S22", "Galaxy S22", "galaxy-s22"), ("S22+", "Galaxy S22+", "galaxy-s22"),
    ("S22 Ultra", "Galaxy S22 Ultra", "galaxy-s22-ultra"),
    ("S23", "Galaxy S23", "galaxy-s23"), ("S23+", "Galaxy S23+", "galaxy-s23"),
    ("S23 FE", "Galaxy S23 FE", "galaxy-s23"), ("S23 Ultra", "Galaxy S23 Ultra", "galaxy-s23-ultra"),
    ("S24", "Galaxy S24", "galaxy-s24"), ("S24+", "Galaxy S24+", "galaxy-s24"),
    ("S24 FE", "Galaxy S24 FE", "galaxy-s24"), ("S24 Ultra", "Galaxy S24 Ultra", "galaxy-s24-ultra"),
    ("S25", "Galaxy S25", "galaxy-s25"), ("S25+", "Galaxy S25+", "galaxy-s25"),
    ("S25 FE", "Galaxy S25 FE", "galaxy-s25"), ("S25 Edge", "Galaxy S25 Edge", "galaxy-s25"),
    ("S25 Ultra", "Galaxy S25 Ultra", "galaxy-s25-ultra"),
    ("Note20", "Galaxy Note20", "galaxy-note20"), ("Note20 Ultra", "Galaxy Note20 Ultra", "galaxy-note20"),
    ("Z Fold", "Galaxy Z Fold", "galaxy-z-fold"), ("Z Fold2", "Galaxy Z Fold2", "galaxy-z-fold"),
    ("Z Fold3", "Galaxy Z Fold3", "galaxy-z-fold"), ("Z Fold4", "Galaxy Z Fold4", "galaxy-z-fold"),
    ("Z Fold5", "Galaxy Z Fold5", "galaxy-z-fold"), ("Z Fold6", "Galaxy Z Fold6", "galaxy-z-fold"),
    ("Z Fold7", "Galaxy Z Fold7", "galaxy-z-fold"),
    ("Z Flip", "Galaxy Z Flip", "galaxy-z-flip-classic"), ("Z Flip3", "Galaxy Z Flip3", "galaxy-z-flip-classic"),
    ("Z Flip4", "Galaxy Z Flip4", "galaxy-z-flip-classic"), ("Z Flip5", "Galaxy Z Flip5", "galaxy-z-flip"),
    ("Z Flip6", "Galaxy Z Flip6", "galaxy-z-flip"), ("Z Flip7", "Galaxy Z Flip7", "galaxy-z-flip"),
    ("Z Flip7 FE", "Galaxy Z Flip7 FE", "galaxy-z-flip"),
    ("Galaxy A12", "Galaxy A12", "galaxy-a-classic"), ("Galaxy A13", "Galaxy A13", "galaxy-a-classic"),
    ("Galaxy A21s", "Galaxy A21s", "galaxy-a-classic"), ("Galaxy A22", "Galaxy A22", "galaxy-a-classic"),
    ("Galaxy A23", "Galaxy A23", "galaxy-a-classic"), ("Galaxy A31", "Galaxy A31", "galaxy-a-classic"),
    ("Galaxy A32", "Galaxy A32", "galaxy-a-classic"), ("Galaxy A33", "Galaxy A33", "galaxy-a-classic"),
    ("Galaxy A51", "Galaxy A51", "galaxy-a-classic"), ("Galaxy A52", "Galaxy A52", "galaxy-a-classic"),
    ("Galaxy A52s", "Galaxy A52s", "galaxy-a-classic"), ("Galaxy A53", "Galaxy A53", "galaxy-a-classic"),
    ("Galaxy A71", "Galaxy A71", "galaxy-a-classic"), ("Galaxy A72", "Galaxy A72", "galaxy-a-classic"),
    ("Galaxy A73", "Galaxy A73", "galaxy-a-classic"),
    ("Galaxy A14", "Galaxy A14", "galaxy-a"), ("Galaxy A15", "Galaxy A15", "galaxy-a"),
    ("Galaxy A16", "Galaxy A16", "galaxy-a"), ("Galaxy A24", "Galaxy A24", "galaxy-a"),
    ("Galaxy A25", "Galaxy A25", "galaxy-a"), ("Galaxy A34", "Galaxy A34", "galaxy-a"),
    ("Galaxy A35", "Galaxy A35", "galaxy-a"), ("Galaxy A36", "Galaxy A36", "galaxy-a"),
    ("Galaxy A54", "Galaxy A54", "galaxy-a"), ("Galaxy A55", "Galaxy A55", "galaxy-a"),
    ("Galaxy A56", "Galaxy A56", "galaxy-a"),
    ("Mi 10", "Mi 10", "xiaomi-classic"), ("Mi 10 Pro", "Mi 10 Pro", "xiaomi-classic"),
    ("Mi 10T", "Mi 10T", "xiaomi-classic"), ("Mi 10T Pro", "Mi 10T Pro", "xiaomi-classic"),
    ("Mi 11", "Mi 11", "xiaomi-classic"), ("Xiaomi 11T", "Xiaomi 11T", "xiaomi-classic"),
    ("Xiaomi 11T Pro", "Xiaomi 11T Pro", "xiaomi-classic"), ("Xiaomi 12", "Xiaomi 12", "xiaomi-classic"),
    ("Xiaomi 12 Pro", "Xiaomi 12 Pro", "xiaomi-classic"), ("Xiaomi 12T", "Xiaomi 12T", "xiaomi-classic"),
    ("Xiaomi 12T Pro", "Xiaomi 12T Pro", "xiaomi-classic"),
    ("Xiaomi 13", "Xiaomi 13", "xiaomi-square"), ("Xiaomi 13 Pro", "Xiaomi 13 Pro", "xiaomi-square"),
    ("Xiaomi 13T", "Xiaomi 13T", "xiaomi-square"), ("Xiaomi 13T Pro", "Xiaomi 13T Pro", "xiaomi-square"),
    ("Xiaomi 14", "Xiaomi 14", "xiaomi-square"), ("Xiaomi 14 Pro", "Xiaomi 14 Pro", "xiaomi-square"),
    ("Xiaomi 14T", "Xiaomi 14T", "xiaomi-square"), ("Xiaomi 14T Pro", "Xiaomi 14T Pro", "xiaomi-square"),
    ("Xiaomi 15", "Xiaomi 15", "xiaomi-square"), ("Xiaomi 15 Pro", "Xiaomi 15 Pro", "xiaomi-square"),
    ("Mi 11 Ultra", "Mi 11 Ultra", "xiaomi-ultra"), ("Xiaomi 12S Ultra", "Xiaomi 12S Ultra", "xiaomi-ultra"),
    ("Xiaomi 13 Ultra", "Xiaomi 13 Ultra", "xiaomi-ultra"), ("Xiaomi 14 Ultra", "Xiaomi 14 Ultra", "xiaomi-ultra"),
    ("Xiaomi 15 Ultra", "Xiaomi 15 Ultra", "xiaomi-ultra"),
    ("Redmi Note 9", "Redmi Note 9", "redmi-note-11"), ("Redmi Note 10", "Redmi Note 10", "redmi-note-11"),
    ("Redmi Note 10 Pro", "Redmi Note 10 Pro", "redmi-note-11"),
    ("Redmi Note 11", "Redmi Note 11", "redmi-note-11"),
    ("Redmi Note 11 Pro", "Redmi Note 11 Pro", "redmi-note-11"),
    ("Redmi Note 11 Pro 5G", "Redmi Note 11 Pro 5G", "redmi-note-11"),
    ("Redmi Note 12", "Redmi Note 12", "redmi-note-13"), ("Redmi Note 12 Pro", "Redmi Note 12 Pro", "redmi-note-13"),
    ("Redmi Note 13", "Redmi Note 13", "redmi-note-13"), ("Redmi Note 13 Pro", "Redmi Note 13 Pro", "redmi-note-13"),
    ("Redmi Note 14", "Redmi Note 14", "redmi-note-14"), ("Redmi Note 14 Pro", "Redmi Note 14 Pro", "redmi-note-14"),
    ("Redmi 9", "Redmi 9", "redmi"), ("Redmi 10", "Redmi 10", "redmi"), ("Redmi 12", "Redmi 12", "redmi"),
    ("Redmi 13", "Redmi 13", "redmi"), ("Redmi 14", "Redmi 14", "redmi"),
    ("POCO F3", "POCO F3", "poco-f"), ("POCO F4", "POCO F4", "poco-f"), ("POCO F5", "POCO F5", "poco-f"),
    ("POCO F6", "POCO F6", "poco-f"), ("POCO F7", "POCO F7", "poco-f"),
    ("POCO X3", "POCO X3", "poco-x"), ("POCO X4", "POCO X4", "poco-x"), ("POCO X5", "POCO X5", "poco-x"),
    ("POCO X6", "POCO X6", "poco-x"), ("POCO X7", "POCO X7", "poco-x"), ("POCO M3", "POCO M3", "poco-x"),
    ("POCO M4", "POCO M4", "poco-x"), ("POCO M5", "POCO M5", "poco-x"), ("POCO M6", "POCO M6", "poco-x"),
    ("Find X2", "OPPO Find X2", "oppo-find-x-classic"), ("Find X3", "OPPO Find X3", "oppo-find-x-classic"),
    ("Find X3 Pro", "OPPO Find X3 Pro", "oppo-find-x-classic"), ("Find X5", "OPPO Find X5", "oppo-find-x-classic"),
    ("Find X5 Pro", "OPPO Find X5 Pro", "oppo-find-x-classic"),
    ("Find X6", "OPPO Find X6", "oppo-find-x"), ("Find X7", "OPPO Find X7", "oppo-find-x"),
    ("Find X8", "OPPO Find X8", "oppo-find-x"),
    ("OPPO Reno4", "OPPO Reno4", "oppo-reno"), ("OPPO Reno5", "OPPO Reno5", "oppo-reno"),
    ("OPPO Reno6", "OPPO Reno6", "oppo-reno"), ("OPPO Reno7", "OPPO Reno7", "oppo-reno"),
    ("OPPO Reno8", "OPPO Reno8", "oppo-reno"), ("OPPO Reno10", "OPPO Reno10", "oppo-reno"),
    ("OPPO Reno11", "OPPO Reno11", "oppo-reno"), ("OPPO Reno12", "OPPO Reno12", "oppo-reno"),
    ("OPPO Reno13", "OPPO Reno13", "oppo-reno"),
    ("vivo X50", "vivo X50", "vivo-x-classic"), ("vivo X60", "vivo X60", "vivo-x-classic"),
    ("vivo X70", "vivo X70", "vivo-x-classic"), ("vivo X80", "vivo X80", "vivo-x"),
    ("vivo X90", "vivo X90", "vivo-x"), ("vivo X100", "vivo X100", "vivo-x"), ("vivo X200", "vivo X200", "vivo-x"),
    ("vivo V20", "vivo V20", "vivo-v"), ("vivo V21", "vivo V21", "vivo-v"), ("vivo V23", "vivo V23", "vivo-v"),
    ("vivo V25", "vivo V25", "vivo-v"), ("vivo V27", "vivo V27", "vivo-v"), ("vivo V29", "vivo V29", "vivo-v"),
    ("vivo V30", "vivo V30", "vivo-v"), ("vivo V40", "vivo V40", "vivo-v"), ("vivo V50", "vivo V50", "vivo-v"),
    ("OnePlus 8", "OnePlus 8", "oneplus-classic"), ("OnePlus 8 Pro", "OnePlus 8 Pro", "oneplus-classic"),
    ("OnePlus 8T", "OnePlus 8T", "oneplus-classic"), ("OnePlus 9", "OnePlus 9", "oneplus-classic"),
    ("OnePlus 9 Pro", "OnePlus 9 Pro", "oneplus-classic"), ("OnePlus 10 Pro", "OnePlus 10 Pro", "oneplus-classic"),
    ("OnePlus 10T", "OnePlus 10T", "oneplus-classic"), ("OnePlus 11", "OnePlus 11", "oneplus"),
    ("OnePlus 12", "OnePlus 12", "oneplus"), ("OnePlus 12R", "OnePlus 12R", "oneplus"),
    ("OnePlus 13", "OnePlus 13", "oneplus"), ("OnePlus Nord", "OnePlus Nord", "oneplus-nord"),
    ("OnePlus Nord 2", "OnePlus Nord 2", "oneplus-nord"),
    ("Pixel 4a", "Pixel 4a", "pixel-5"), ("Pixel 5", "Pixel 5", "pixel-5"), ("Pixel 5a", "Pixel 5a", "pixel-5"),
    ("Pixel 6", "Pixel 6", "pixel-6"), ("Pixel 6 Pro", "Pixel 6 Pro", "pixel-6"), ("Pixel 6a", "Pixel 6a", "pixel-6"),
    ("Pixel 7", "Pixel 7", "pixel-7"), ("Pixel 7 Pro", "Pixel 7 Pro", "pixel-7"), ("Pixel 7a", "Pixel 7a", "pixel-7"),
    ("Pixel 8", "Pixel 8", "pixel-7"), ("Pixel 8 Pro", "Pixel 8 Pro", "pixel-7"), ("Pixel 8a", "Pixel 8a", "pixel-7"),
    ("Pixel 9", "Pixel 9", "pixel-9"), ("Pixel 9 Pro", "Pixel 9 Pro", "pixel-9"),
    ("Pixel 9 Pro XL", "Pixel 9 Pro XL", "pixel-9"), ("Pixel 9a", "Pixel 9a", "pixel-9a"),
    ("Pixel 10", "Pixel 10", "pixel-9"), ("Pixel 10 Pro", "Pixel 10 Pro", "pixel-9"),
    ("Pixel 10 Pro XL", "Pixel 10 Pro XL", "pixel-9"), ("Pixel 10a", "Pixel 10a", "pixel-9a"),
    ("Pixel Fold", "Pixel Fold", "pixel-fold"), ("Pixel 9 Pro Fold", "Pixel 9 Pro Fold", "pixel-fold"),
    ("Pixel 10 Pro Fold", "Pixel 10 Pro Fold", "pixel-fold"),
    ("realme GT", "realme GT", "realme-gt"), ("realme 8", "realme 8", "realme"), ("realme 9", "realme 9", "realme"),
    ("realme 10", "realme 10", "realme"), ("realme 11", "realme 11", "realme"), ("realme 12", "realme 12", "realme"),
    ("realme 13", "realme 13", "realme"), ("realme 14", "realme 14", "realme"),
    ("Honor Magic", "Honor Magic", "honor"), ("Honor 50", "Honor 50", "honor"), ("Honor 70", "Honor 70", "honor"),
    ("Honor 90", "Honor 90", "honor"), ("Honor 200", "Honor 200", "honor"), ("Honor 400", "Honor 400", "honor"),
    ("motorola edge", "motorola edge", "moto-edge"), ("moto edge", "motorola edge", "moto-edge"),
    ("razr", "motorola razr", "moto-razr"),
    ("Nothing Phone 1", "Nothing Phone (1)", "nothing"), ("Nothing Phone 2", "Nothing Phone (2)", "nothing"),
    ("Nothing Phone 2a", "Nothing Phone (2a)", "nothing-dark"), ("Nothing Phone 3", "Nothing Phone (3)", "nothing-dark"),
    ("Nothing Phone 3a", "Nothing Phone (3a)", "nothing-dark"),
];

/// The picture for a phone, from its model string or (failing that) its name.
pub fn picture_for(model: &str, name: &str) -> Option<&'static [u8]> {
    let art = match code_name(model) {
        Some(display) => best(display)?.2,
        None => {
            [best(model), best(name)]
                .into_iter()
                .flatten()
                .max_by_key(|e| normalize(e.0).len())?
                .2
        }
    };
    png(art)
}

/// The phone's marketing name ("Galaxy S24 Ultra") when the model string is recognised.
pub fn marketing_name(model: &str) -> Option<&'static str> {
    code_name(model).or_else(|| best(model).map(|e| e.1))
}

fn code_name(model: &str) -> Option<&'static str> {
    let upper = model.to_ascii_uppercase();
    if let Some(i) = upper.find("SM-") {
        let code = upper.get(i + 3..i + 7)?;
        return SAMSUNG.iter().find(|(c, _)| *c == code).map(|(_, d)| *d);
    }
    upper
        .split(|c: char| !c.is_ascii_alphanumeric())
        .find_map(|word| CODES.iter().find(|(c, _)| *c == word))
        .map(|(_, d)| *d)
}

/// The longest table entry whose words appear in `text`.
fn best(text: &str) -> Option<&'static (&'static str, &'static str, &'static str)> {
    let hay = format!(" {} ", normalize(text));
    NAMES
        .iter()
        .filter(|e| hay.contains(&format!(" {} ", normalize(e.0))))
        .max_by_key(|e| normalize(e.0).len())
}

/// Lower-case words split at punctuation and between letters and digits ("S24+" -> "s 24 plus").
fn normalize(s: &str) -> String {
    let mut out = String::new();
    let mut prev = ' ';
    for c in s.chars().map(|c| c.to_ascii_lowercase()) {
        if c == '+' {
            out.push_str(" plus ");
            prev = ' ';
        } else if c.is_ascii_alphanumeric() {
            if prev != ' ' && prev.is_ascii_digit() != c.is_ascii_digit() {
                out.push(' ');
            }
            out.push(c);
            prev = c;
        } else {
            out.push(' ');
            prev = ' ';
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn png(art: &str) -> Option<&'static [u8]> {
    macro_rules! arts {
        ($($id:literal),* $(,)?) => {
            match art {
                $($id => Some(include_bytes!(concat!("../data/phones/", $id, ".png"))),)*
                _ => None,
            }
        };
    }
    arts!(
        "galaxy-s20",
        "galaxy-s20-ultra",
        "galaxy-s21",
        "galaxy-s21-ultra",
        "galaxy-s22",
        "galaxy-s22-ultra",
        "galaxy-s23",
        "galaxy-s23-ultra",
        "galaxy-s24",
        "galaxy-s24-ultra",
        "galaxy-s25",
        "galaxy-s25-ultra",
        "galaxy-note20",
        "galaxy-z-fold",
        "galaxy-z-flip",
        "galaxy-z-flip-classic",
        "galaxy-a",
        "galaxy-a-classic",
        "xiaomi-ultra",
        "xiaomi-square",
        "xiaomi-classic",
        "redmi-note-11",
        "redmi-note-13",
        "redmi-note-14",
        "redmi",
        "poco-x",
        "poco-f",
        "oppo-find-x",
        "oppo-find-x-classic",
        "oppo-reno",
        "vivo-x",
        "vivo-x-classic",
        "vivo-v",
        "oneplus",
        "oneplus-classic",
        "oneplus-nord",
        "pixel-5",
        "pixel-6",
        "pixel-7",
        "pixel-9",
        "pixel-9a",
        "pixel-fold",
        "realme",
        "realme-gt",
        "honor",
        "moto-edge",
        "moto-razr",
        "nothing",
        "nothing-dark",
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn art(model: &str, name: &str) -> Option<&'static str> {
        let png = picture_for(model, name)?;
        NAMES
            .iter()
            .map(|e| e.2)
            .find(|a| super::png(a) == Some(png))
    }

    #[test]
    fn samsung_codes() {
        assert_eq!(marketing_name("samsung SM-S928B"), Some("Galaxy S24 Ultra"));
        assert_eq!(marketing_name("samsung SM-S921U1"), Some("Galaxy S24"));
        assert_eq!(marketing_name("samsung SM-G998B"), Some("Galaxy S21 Ultra"));
        assert_eq!(marketing_name("samsung SM-F946B"), Some("Galaxy Z Fold5"));
        assert_eq!(marketing_name("samsung SM-A546E"), Some("Galaxy A54"));
        assert_eq!(art("samsung SM-S928B", "Phone"), Some("galaxy-s24-ultra"));
        assert_eq!(art("samsung SM-S711B", "x"), Some("galaxy-s23"));
        assert_eq!(art("samsung SM-F731B", "x"), Some("galaxy-z-flip"));
        assert_eq!(art("samsung SM-F711B", "x"), Some("galaxy-z-flip-classic"));
        assert_eq!(art("samsung SM-A525F", "x"), Some("galaxy-a-classic"));
        assert_eq!(art("samsung SM-N986B", "x"), Some("galaxy-note20"));
    }

    #[test]
    fn names_and_longest_match() {
        assert_eq!(
            art("samsung SM-X999", "My S24 Ultra"),
            Some("galaxy-s24-ultra")
        );
        assert_eq!(art("", "Galaxy S24"), Some("galaxy-s24"));
        assert_eq!(art("", "Galaxy S20 Ultra 5G"), Some("galaxy-s20-ultra"));
        assert_eq!(marketing_name("Galaxy S24+"), Some("Galaxy S24+"));
        assert_eq!(marketing_name("Galaxy S24 Plus"), Some("Galaxy S24+"));
        assert_eq!(art("Google Pixel 9 Pro", ""), Some("pixel-9"));
        assert_eq!(art("Google Pixel 9a", ""), Some("pixel-9a"));
        assert_eq!(art("Google Pixel 9 Pro Fold", ""), Some("pixel-fold"));
        assert_eq!(marketing_name("Google Pixel 8a"), Some("Pixel 8a"));
        assert_eq!(art("Google Pixel 6a", "Pixel"), Some("pixel-6"));
    }

    #[test]
    fn xiaomi_family() {
        assert_eq!(marketing_name("Xiaomi 2201117TG"), Some("Redmi Note 11"));
        assert_eq!(art("Xiaomi 2201117TY", ""), Some("redmi-note-11"));
        assert_eq!(art("Xiaomi Redmi Note 11", ""), Some("redmi-note-11"));
        assert_eq!(
            art("Xiaomi 23129RAA4G", "Redmi Note 13 Pro+ 5G"),
            Some("redmi-note-13")
        );
        assert_eq!(art("Xiaomi 24030PN60G", ""), Some("xiaomi-ultra"));
        assert_eq!(art("Xiaomi 2211133G", ""), Some("xiaomi-square"));
        assert_eq!(art("Xiaomi M2007J3SG", ""), Some("xiaomi-classic"));
        assert_eq!(art("Xiaomi Redmi 9A", ""), Some("redmi"));
        assert_eq!(art("Xiaomi 22101320G", "POCO X5 Pro 5G"), Some("poco-x"));
        assert_eq!(art("Xiaomi M2012K11AG", ""), Some("poco-f"));
        // "Mi 10" must not match inside "Redmi 10".
        assert_eq!(marketing_name("Xiaomi Redmi 10"), Some("Redmi 10"));
    }

    #[test]
    fn other_brands() {
        assert_eq!(marketing_name("OnePlus CPH2581"), Some("OnePlus 12"));
        assert_eq!(art("OnePlus LE2123", ""), Some("oneplus-classic"));
        assert_eq!(
            art("OnePlus CPH2609", "OnePlus Nord CE4"),
            Some("oneplus-nord")
        );
        assert_eq!(art("OPPO CPH2173", ""), Some("oppo-find-x-classic"));
        assert_eq!(art("OPPO CPH9999", "OPPO Find X8 Pro"), Some("oppo-find-x"));
        assert_eq!(art("OPPO CPH9999", "Reno12 Pro"), None);
        assert_eq!(art("OPPO CPH9999", "OPPO Reno12 Pro"), Some("oppo-reno"));
        assert_eq!(art("vivo V2309", "vivo X100 Pro"), Some("vivo-x"));
        assert_eq!(art("vivo V2050", "vivo X60"), Some("vivo-x-classic"));
        assert_eq!(art("motorola motorola edge 50 pro", ""), Some("moto-edge"));
        assert_eq!(
            art("motorola motorola razr 50 ultra", ""),
            Some("moto-razr")
        );
        assert_eq!(art("Nothing A063", ""), Some("nothing"));
        assert_eq!(art("Nothing A142", ""), Some("nothing-dark"));
        assert_eq!(art("realme RMX3800", "realme GT 6"), Some("realme-gt"));
        assert_eq!(art("HONOR BVL-N49", "HONOR Magic6 Pro"), Some("honor"));
    }

    #[test]
    fn unknown() {
        assert_eq!(picture_for("samsung SM-X710", "Galaxy Tab S9"), None);
        assert_eq!(picture_for("LENOVO TB-X606F", "Lenovo Tab"), None);
        assert_eq!(picture_for("", ""), None);
        assert_eq!(marketing_name("Xiaomi 12345XYZ"), None);
        assert_eq!(marketing_name("samsung SM-"), None);
    }

    #[test]
    fn every_entry_has_a_picture() {
        for (_, display, art) in NAMES {
            assert!(png(art).is_some(), "{art}");
            assert!(best(display).is_some(), "{display}");
        }
        for (_, display) in SAMSUNG.iter().chain(CODES) {
            assert!(best(display).is_some(), "{display}");
        }
    }
}
