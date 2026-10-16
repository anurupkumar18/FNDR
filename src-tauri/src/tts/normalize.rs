//! Text as it is said: numbers, ordinals, money, percentages, dates, times,
//! units, common abbreviations, links and symbols become words before the
//! pronunciation step. The webview's `speakable()` (Notch Do) already drops
//! what should never be read aloud, such as secrets and long tokens; this
//! only rewrites what is read.

use once_cell::sync::Lazy;
use regex::{Captures, Regex};

const ONES: [&str; 20] = [
    "zero",
    "one",
    "two",
    "three",
    "four",
    "five",
    "six",
    "seven",
    "eight",
    "nine",
    "ten",
    "eleven",
    "twelve",
    "thirteen",
    "fourteen",
    "fifteen",
    "sixteen",
    "seventeen",
    "eighteen",
    "nineteen",
];
const TENS: [&str; 10] = [
    "", "", "twenty", "thirty", "forty", "fifty", "sixty", "seventy", "eighty", "ninety",
];
const SCALES: [&str; 5] = ["", "thousand", "million", "billion", "trillion"];
const MONTHS: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];

fn below_thousand(n: u64) -> String {
    let mut parts = Vec::new();
    let hundreds = n / 100;
    let rest = n % 100;
    if hundreds > 0 {
        parts.push(format!("{} hundred", ONES[hundreds as usize]));
    }
    if rest > 0 {
        if rest < 20 {
            parts.push(ONES[rest as usize].to_string());
        } else if rest % 10 == 0 {
            parts.push(TENS[(rest / 10) as usize].to_string());
        } else {
            parts.push(format!(
                "{} {}",
                TENS[(rest / 10) as usize],
                ONES[(rest % 10) as usize]
            ));
        }
    }
    parts.join(" ")
}

/// "1234" -> "one thousand two hundred thirty four". Digits beyond the
/// trillions, and any non-digit input, are read one digit at a time.
pub fn cardinal(n: u64) -> String {
    if n == 0 {
        return "zero".to_string();
    }
    let mut groups = Vec::new();
    let mut rest = n;
    while rest > 0 {
        groups.push(rest % 1000);
        rest /= 1000;
    }
    if groups.len() > SCALES.len() {
        return digits(&n.to_string());
    }
    let mut parts = Vec::new();
    for (scale, group) in groups.iter().enumerate().rev() {
        if *group == 0 {
            continue;
        }
        let words = below_thousand(*group);
        if SCALES[scale].is_empty() {
            parts.push(words);
        } else {
            parts.push(format!("{words} {}", SCALES[scale]));
        }
    }
    parts.join(" ")
}

pub fn cardinal_words(raw: &str) -> String {
    let clean: String = raw.chars().filter(|c| *c != ',').collect();
    if clean.is_empty() || !clean.chars().all(|c| c.is_ascii_digit()) {
        return digits(raw);
    }
    if clean.len() > 1 && clean.starts_with('0') {
        return digits(&clean);
    }
    match clean.parse::<u64>() {
        Ok(n) if clean.len() <= 15 => cardinal(n),
        _ => digits(&clean),
    }
}

fn digits(raw: &str) -> String {
    raw.chars()
        .filter_map(|c| c.to_digit(10))
        .map(|d| ONES[d as usize])
        .collect::<Vec<_>>()
        .join(" ")
}

fn ordinal_of(words: &str) -> String {
    let (head, last) = match words.rsplit_once(' ') {
        Some((head, last)) => (format!("{head} "), last),
        None => (String::new(), words),
    };
    let last = match last {
        "one" => "first".to_string(),
        "two" => "second".to_string(),
        "three" => "third".to_string(),
        "five" => "fifth".to_string(),
        "eight" => "eighth".to_string(),
        "nine" => "ninth".to_string(),
        "twelve" => "twelfth".to_string(),
        word if word.ends_with('y') => format!("{}ieth", &word[..word.len() - 1]),
        word => format!("{word}th"),
    };
    format!("{head}{last}")
}

pub fn ordinal(n: u64) -> String {
    ordinal_of(&cardinal(n))
}

/// A year as it is said: 1999 "nineteen ninety nine", 2005 "two thousand
/// five", 2026 "twenty twenty six", 1905 "nineteen oh five".
pub fn year(n: u64) -> String {
    if !(1000..=9999).contains(&n) || (2000..2010).contains(&n) || n % 1000 == 0 {
        return cardinal(n);
    }
    let high = n / 100;
    let low = n % 100;
    let head = below_thousand(high);
    match low {
        0 => format!("{head} hundred"),
        1..=9 => format!("{head} oh {}", ONES[low as usize]),
        _ => format!("{head} {}", below_thousand(low)),
    }
}

fn decimal(int_part: &str, frac: &str) -> String {
    let whole = if int_part.is_empty() {
        "zero".to_string()
    } else {
        cardinal_words(int_part)
    };
    format!("{whole} point {}", digits(frac))
}

fn number_words(raw: &str) -> String {
    match raw.split_once('.') {
        Some((int_part, frac)) if !frac.is_empty() => decimal(int_part, frac),
        _ => cardinal_words(raw.trim_end_matches('.')),
    }
}

fn plural(count_is_one: bool, one: &str, many: &str) -> String {
    if count_is_one { one } else { many }.to_string()
}

fn money(symbol: &str, amount: &str, scale: Option<&str>) -> String {
    let (unit, units, sub, subs) = match symbol {
        "£" => ("pound", "pounds", "penny", "pence"),
        "€" => ("euro", "euros", "cent", "cents"),
        _ => ("dollar", "dollars", "cent", "cents"),
    };
    let clean: String = amount.chars().filter(|c| *c != ',').collect();
    if let Some(scale) = scale {
        return format!("{} {} {units}", number_words(&clean), scale_word(scale));
    }
    let (whole, cents) = match clean.split_once('.') {
        Some((w, c)) => (w.to_string(), c.to_string()),
        None => (clean.clone(), String::new()),
    };
    let whole_n: u64 = whole.parse().unwrap_or(0);
    let cents_n: u64 = if cents.is_empty() {
        0
    } else if cents.len() == 1 {
        cents.parse::<u64>().unwrap_or(0) * 10
    } else if cents.len() == 2 {
        cents.parse().unwrap_or(0)
    } else {
        return format!("{} {units}", decimal(&whole, &cents));
    };
    let dollars = format!(
        "{} {}",
        cardinal_words(&whole),
        plural(whole_n == 1, unit, units)
    );
    if cents_n == 0 {
        return dollars;
    }
    let change = format!("{} {}", cardinal(cents_n), plural(cents_n == 1, sub, subs));
    if whole_n == 0 {
        change
    } else {
        format!("{dollars} and {change}")
    }
}

fn scale_word(scale: &str) -> &'static str {
    match scale.to_ascii_lowercase().as_str() {
        "k" | "thousand" => "thousand",
        "m" | "mn" | "million" => "million",
        "b" | "bn" | "billion" => "billion",
        _ => "trillion",
    }
}

fn month_index(name: &str) -> Option<usize> {
    let lower = name.trim_end_matches('.').to_ascii_lowercase();
    if lower.len() < 3 {
        return None;
    }
    MONTHS.iter().position(|m| {
        let m = m.to_ascii_lowercase();
        m == lower
            || (lower.len() <= 4 && m.starts_with(&lower) && (lower.len() == 3 || lower == "sept"))
    })
}

fn date_words(month: usize, day: u64, year_n: Option<u64>) -> String {
    let mut out = format!("{} {}", MONTHS[month], ordinal(day));
    if let Some(y) = year_n {
        out.push_str(&format!(", {}", year(y)));
    }
    out
}

fn meridiem(raw: &str) -> &'static str {
    if raw.to_ascii_lowercase().starts_with('p') {
        "PM"
    } else {
        "AM"
    }
}

fn time_words(hour: u64, minute: u64, suffix: Option<&str>) -> String {
    let base = match minute {
        0 if suffix.is_some() => cardinal(hour),
        0 => format!("{} o'clock", cardinal(hour)),
        1..=9 => format!("{} oh {}", cardinal(hour), cardinal(minute)),
        _ => format!("{} {}", cardinal(hour), cardinal(minute)),
    };
    match suffix {
        Some(s) => format!("{base} {}", meridiem(s)),
        None => base,
    }
}

fn unit_word(unit: &str, one: bool) -> Option<String> {
    let (single, many) = match unit {
        "KB" | "kb" => ("kilobyte", "kilobytes"),
        "MB" | "mb" => ("megabyte", "megabytes"),
        "GB" | "gb" => ("gigabyte", "gigabytes"),
        "TB" | "tb" => ("terabyte", "terabytes"),
        "ms" => ("millisecond", "milliseconds"),
        "sec" | "secs" => ("second", "seconds"),
        "min" | "mins" => ("minute", "minutes"),
        "hr" | "hrs" => ("hour", "hours"),
        "km" => ("kilometer", "kilometers"),
        "kg" => ("kilogram", "kilograms"),
        "mph" => ("mile per hour", "miles per hour"),
        "x" | "×" => ("times", "times"),
        _ => return None,
    };
    Some(plural(one, single, many))
}

macro_rules! re {
    ($name:ident, $pattern:expr) => {
        static $name: Lazy<Regex> = Lazy::new(|| Regex::new($pattern).expect("valid pattern"));
    };
}

re!(
    URL,
    r"(?i)\b(?:https?://|www\.)([a-z0-9-]+(?:\.[a-z0-9-]+)+)(?::\d+)?(?:/[^\s]*)?"
);
re!(
    EMAIL,
    r"(?i)\b([a-z0-9._%+-]+)@([a-z0-9-]+(?:\.[a-z0-9-]+)+)\b"
);
re!(ISO_DATE, r"\b(\d{4})-(\d{2})-(\d{2})\b");
re!(
    MONTH_DAY,
    r"(?i)\b(Jan|Feb|Mar|Apr|May|Jun|Jul|Aug|Sep|Sept|Oct|Nov|Dec|January|February|March|April|June|July|August|September|October|November|December)\.? (\d{1,2})(?:st|nd|rd|th)?(?:,? (\d{4}))?\b"
);
re!(
    DAY_MONTH,
    r"(?i)\b(\d{1,2})(?:st|nd|rd|th)? (January|February|March|April|May|June|July|August|September|October|November|December|Jan|Feb|Mar|Apr|Jun|Jul|Aug|Sep|Sept|Oct|Nov|Dec)\.?(?:,? (\d{4}))?\b"
);
re!(SLASH_DATE, r"\b(\d{1,2})/(\d{1,2})/(\d{4})\b");
re!(
    TIME,
    r"(?i)\b(\d{1,2}):(\d{2})(?:\s*([ap]\.?m\.?)(?:\s|$|[,;!?)]))?"
);
re!(
    HOUR_MERIDIEM,
    r"(?i)\b(\d{1,2})\s*([ap]\.?m\.?)(\s|$|[,;!?)])"
);
re!(
    MONEY,
    r"([$£€])\s?(\d{1,3}(?:,\d{3})+(?:\.\d+)?|\d+(?:\.\d+)?)(?:\s?((?i:k|m|mn|b|bn|thousand|million|billion|trillion))\b)?"
);
re!(PERCENT, r"(-?(?:\d{1,3}(?:,\d{3})+|\d+)(?:\.\d+)?)\s?%");
re!(ORDINAL, r"(?i)\b(\d+)(st|nd|rd|th)\b");
re!(RANGE, r"\b(\d+(?:\.\d+)?)\s?[-–]\s?(\d+(?:\.\d+)?)\b");
re!(
    UNIT,
    r"\b(\d+(?:\.\d+)?)\s?(KB|kb|MB|mb|GB|gb|TB|tb|ms|secs?|mins?|hrs?|km|kg|mph|x|×)\b"
);
re!(VERSION, r"\b[vV]?(\d+)\.(\d+)\.(\d+)\b");
re!(LETTER_TWO_LETTER, r"\b([A-Za-z])2([A-Za-z])\b");
re!(NEGATIVE, r"(^|[\s(])-(\d)");
re!(NUMBER, r"\d{1,3}(?:,\d{3})+(?:\.\d+)?|\d+(?:\.\d+)?");
re!(SPACES, r"[ \t]+");

/// Abbreviations read as words. Matched as whole words, case-sensitive.
const ABBREVIATIONS: &[(&str, &str)] = &[
    ("Dr.", "Doctor"),
    ("Mr.", "Mister"),
    ("Mrs.", "Missus"),
    ("Ms.", "Miz"),
    ("Jr.", "Junior"),
    ("Sr.", "Senior"),
    ("Prof.", "Professor"),
    ("vs.", "versus"),
    ("vs", "versus"),
    ("etc.", "et cetera"),
    ("e.g.", "for example"),
    ("i.e.", "that is"),
    ("approx.", "approximately"),
    ("w/", "with"),
    ("w/o", "without"),
];

fn replace_abbreviations(text: &str) -> String {
    text.split(' ')
        .map(|word| {
            let core = word.trim_end_matches([',', ';', ':', '!', '?']);
            let tail = &word[core.len()..];
            match ABBREVIATIONS.iter().find(|(abbr, _)| *abbr == core) {
                Some((_, said)) => format!("{said}{tail}"),
                None => word.to_string(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn host_words(host: &str) -> String {
    host.trim_start_matches("www.")
        .split('.')
        .collect::<Vec<_>>()
        .join(" dot ")
}

fn symbols(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let chars: Vec<char> = text.chars().collect();
    for (i, c) in chars.iter().enumerate() {
        let prev_space = i == 0 || chars[i - 1].is_whitespace();
        let next_space = i + 1 == chars.len() || chars[i + 1].is_whitespace();
        let next_digit = chars.get(i + 1).is_some_and(|n| n.is_ascii_digit());
        match c {
            '&' => out.push_str(" and "),
            '+' if prev_space || next_space || next_digit => out.push_str(" plus "),
            '=' => out.push_str(" equals "),
            '@' => out.push_str(" at "),
            '#' if next_digit => out.push_str(" number "),
            '°' => out.push_str(" degrees "),
            '~' if next_digit => out.push_str(" about "),
            '→' | '⇒' => out.push_str(" to "),
            '×' => out.push_str(" times "),
            '*' | '_' | '`' | '#' | '|' | '<' | '>' | '^' | '\\' | '{' | '}' | '[' | ']' => {
                out.push(' ')
            }
            '‘' | '’' => out.push('\''),
            '–' if !(prev_space && next_space) => out.push('-'),
            '–' => out.push('—'),
            _ => out.push(*c),
        }
    }
    out
}

/// Rewrites text into words the pronunciation step can read.
pub fn normalize(text: &str) -> String {
    let text = text.replace("...", "…").replace(['\r', '\n'], " ");
    let text = URL.replace_all(&text, |c: &Captures| host_words(&c[1]));
    let text = EMAIL.replace_all(&text, |c: &Captures| {
        format!("{} at {}", c[1].replace('.', " dot "), host_words(&c[2]))
    });
    let text = ISO_DATE.replace_all(&text, |c: &Captures| {
        let (y, m, d) = (
            c[1].parse::<u64>().unwrap_or(0),
            c[2].parse::<usize>().unwrap_or(0),
            c[3].parse::<u64>().unwrap_or(0),
        );
        if (1..=12).contains(&m) && (1..=31).contains(&d) {
            date_words(m - 1, d, Some(y))
        } else {
            c[0].to_string()
        }
    });
    let text = MONTH_DAY.replace_all(&text, |c: &Captures| {
        let day = c[2].parse::<u64>().unwrap_or(0);
        match month_index(&c[1]) {
            Some(m) if (1..=31).contains(&day) => {
                date_words(m, day, c.get(3).and_then(|y| y.as_str().parse().ok()))
            }
            _ => c[0].to_string(),
        }
    });
    let text = DAY_MONTH.replace_all(&text, |c: &Captures| {
        let day = c[1].parse::<u64>().unwrap_or(0);
        match month_index(&c[2]) {
            Some(m) if (1..=31).contains(&day) => {
                let mut out = format!("the {} of {}", ordinal(day), MONTHS[m]);
                if let Some(y) = c.get(3).and_then(|y| y.as_str().parse().ok()) {
                    out.push_str(&format!(", {}", year(y)));
                }
                out
            }
            _ => c[0].to_string(),
        }
    });
    let text = SLASH_DATE.replace_all(&text, |c: &Captures| {
        let (a, b, y) = (
            c[1].parse::<u64>().unwrap_or(0),
            c[2].parse::<u64>().unwrap_or(0),
            c[3].parse::<u64>().unwrap_or(0),
        );
        let (month, day) = if a > 12 { (b, a) } else { (a, b) };
        if (1..=12).contains(&month) && (1..=31).contains(&day) {
            date_words(month as usize - 1, day, Some(y))
        } else {
            c[0].to_string()
        }
    });
    let len = text.len();
    let ends_sentence =
        |c: &Captures| c[0].ends_with('.') && c.get(0).is_some_and(|m| m.end() == len);
    let text = TIME.replace_all(&text, |c: &Captures| {
        let (h, m) = (
            c[1].parse::<u64>().unwrap_or(0),
            c[2].parse::<u64>().unwrap_or(0),
        );
        if h > 24 || m > 59 {
            return c[0].to_string();
        }
        let tail = if ends_sentence(c) {
            ".".to_string()
        } else if c.get(3).is_some() {
            c[0].chars()
                .last()
                .filter(|ch| !ch.is_alphanumeric() && *ch != '.')
                .map(String::from)
                .unwrap_or_default()
        } else {
            String::new()
        };
        format!("{}{tail}", time_words(h, m, c.get(3).map(|s| s.as_str())))
    });
    let len = text.len();
    let text = HOUR_MERIDIEM.replace_all(&text, |c: &Captures| {
        let h = c[1].parse::<u64>().unwrap_or(0);
        if !(1..=12).contains(&h) {
            return c[0].to_string();
        }
        let period = if c[2].ends_with('.') && c.get(0).is_some_and(|m| m.end() == len) {
            "."
        } else {
            ""
        };
        format!("{} {}{period}{}", cardinal(h), meridiem(&c[2]), &c[3])
    });
    let text = MONEY.replace_all(&text, |c: &Captures| {
        money(&c[1], &c[2], c.get(3).map(|s| s.as_str()))
    });
    let text = PERCENT.replace_all(&text, |c: &Captures| {
        let raw = c[1].to_string();
        match raw.strip_prefix('-') {
            Some(rest) => format!("minus {} percent", number_words(rest)),
            None => format!("{} percent", number_words(&raw)),
        }
    });
    let text = ORDINAL.replace_all(&text, |c: &Captures| match c[1].parse::<u64>() {
        Ok(n) => ordinal(n),
        Err(_) => c[0].to_string(),
    });
    let text = UNIT.replace_all(&text, |c: &Captures| {
        let one = &c[1] == "1";
        match unit_word(&c[2], one) {
            Some(unit) => format!("{} {unit}", number_words(&c[1])),
            None => c[0].to_string(),
        }
    });
    let text = RANGE.replace_all(&text, |c: &Captures| {
        format!("{} to {}", number_words(&c[1]), number_words(&c[2]))
    });
    let text = VERSION.replace_all(&text, |c: &Captures| {
        format!(
            "{} point {} point {}",
            cardinal_words(&c[1]),
            cardinal_words(&c[2]),
            cardinal_words(&c[3])
        )
    });
    let text = LETTER_TWO_LETTER.replace_all(&text, "$1 to $2");
    let text = NEGATIVE.replace_all(&text, "${1}minus $2");
    let text = replace_abbreviations(&text);
    let text = symbols(&text);
    let text = NUMBER.replace_all(&text, |c: &Captures| {
        let raw = c[0].to_string();
        let plain: String = raw.chars().filter(|ch| *ch != ',').collect();
        if !raw.contains(',') && !raw.contains('.') && plain.len() == 4 {
            if let Ok(n) = plain.parse::<u64>() {
                if (1100..=2099).contains(&n) {
                    return year(n);
                }
            }
        }
        if !raw.contains(',') && !raw.contains('.') && plain.len() >= 7 {
            return digits(&plain);
        }
        number_words(&raw)
    });
    SPACES.replace_all(text.trim(), " ").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cardinals_ordinals_and_years() {
        assert_eq!(cardinal(0), "zero");
        assert_eq!(cardinal(21), "twenty one");
        assert_eq!(cardinal(105), "one hundred five");
        assert_eq!(
            cardinal(1_234_567),
            "one million two hundred thirty four thousand five hundred sixty seven"
        );
        assert_eq!(ordinal(1), "first");
        assert_eq!(ordinal(12), "twelfth");
        assert_eq!(ordinal(20), "twentieth");
        assert_eq!(ordinal(23), "twenty third");
        assert_eq!(year(1999), "nineteen ninety nine");
        assert_eq!(year(2026), "twenty twenty six");
        assert_eq!(year(2005), "two thousand five");
        assert_eq!(year(1905), "nineteen oh five");
        assert_eq!(year(1900), "nineteen hundred");
    }

    #[test]
    fn numbers_in_text() {
        assert_eq!(normalize("I have 3 cats."), "I have three cats.");
        assert_eq!(
            normalize("It costs 1,250 credits."),
            "It costs one thousand two hundred fifty credits."
        );
        assert_eq!(normalize("pi is 3.14"), "pi is three point one four");
        assert_eq!(
            normalize("Call 5551234567"),
            "Call five five five one two three four five six seven"
        );
        assert_eq!(normalize("Room 007"), "Room zero zero seven");
        assert_eq!(normalize("It was -5 outside"), "It was minus five outside");
        assert_eq!(
            normalize("In 1999 we met"),
            "In nineteen ninety nine we met"
        );
        assert_eq!(normalize("3, 4, and 5."), "three, four, and five.");
    }

    #[test]
    fn money_and_percent() {
        assert_eq!(normalize("$5.20"), "five dollars and twenty cents");
        assert_eq!(normalize("$1"), "one dollar");
        assert_eq!(normalize("$0.50"), "fifty cents");
        assert_eq!(normalize("£3"), "three pounds");
        assert_eq!(normalize("€2.5M"), "two point five million euros");
        assert_eq!(normalize("45% done"), "forty five percent done");
        assert_eq!(normalize("up 2.5%"), "up two point five percent");
    }

    #[test]
    fn dates_and_times() {
        assert_eq!(
            normalize("on 2026-10-09"),
            "on October ninth, twenty twenty six"
        );
        assert_eq!(normalize("Oct 9, 2026"), "October ninth, twenty twenty six");
        assert_eq!(normalize("March 3rd"), "March third");
        assert_eq!(
            normalize("9 October 2026"),
            "the ninth of October, twenty twenty six"
        );
        assert_eq!(normalize("10/09/2026"), "October ninth, twenty twenty six");
        assert_eq!(normalize("at 10:30"), "at ten thirty");
        assert_eq!(normalize("at 9:05 am."), "at nine oh five AM.");
        assert_eq!(normalize("at 3pm, then"), "at three PM, then");
        assert_eq!(normalize("at 14:00"), "at fourteen o'clock");
    }

    #[test]
    fn ordinals_ranges_units_versions() {
        assert_eq!(normalize("the 21st time"), "the twenty first time");
        assert_eq!(normalize("pages 5-10"), "pages five to ten");
        assert_eq!(normalize("a 4 GB file"), "a four gigabytes file");
        assert_eq!(normalize("took 1 ms"), "took one millisecond");
        assert_eq!(
            normalize("version 2.1.3"),
            "version two point one point three"
        );
        assert_eq!(normalize("a G2P engine"), "a G to P engine");
    }

    #[test]
    fn abbreviations_symbols_and_links() {
        assert_eq!(
            normalize("Dr. Smith vs. Mr. Jones"),
            "Doctor Smith versus Mister Jones"
        );
        assert_eq!(normalize("apples, pears, etc."), "apples, pears, et cetera");
        assert_eq!(normalize("R&D"), "R and D");
        assert_eq!(
            normalize("see https://www.fndr.app/docs/start?x=1 now"),
            "see fndr dot app now"
        );
        assert_eq!(
            normalize("mail kunj.r@example.com"),
            "mail kunj dot r at example dot com"
        );
        assert_eq!(normalize("**bold** and `code`"), "bold and code");
        assert_eq!(normalize("wait..."), "wait…");
        assert_eq!(normalize("2 + 2 = 4"), "two plus two equals four");
        assert_eq!(normalize("issue #42"), "issue number forty two");
    }

    #[test]
    fn text_without_numbers_is_unchanged() {
        let line = "Your meeting with Priya starts soon.";
        assert_eq!(normalize(line), line);
    }
}
