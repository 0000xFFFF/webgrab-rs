use regex::Regex;
use std::sync::OnceLock;
use url::Url;

fn re_dq() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r#""([^"]*)""#).unwrap())
}
fn re_sq() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r#"'([^']*)'"#).unwrap())
}
fn re_css() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r#"(?i)url\(\s*['"]?([^'")\s]+)['"]?\s*\)"#).unwrap())
}
fn re_href() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| {
        Regex::new(r#"(?i)<(?:a|area)\b[^>]*?\bhref\s*=\s*(?:"([^"]*)"|'([^']*)'|([^\s"'>]+))"#).unwrap()
    })
}
fn re_frame() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| {
        Regex::new(r#"(?i)<(?:iframe|frame)\b[^>]*?\bsrc\s*=\s*(?:"([^"]*)"|'([^']*)'|([^\s"'>]+))"#).unwrap()
    })
}
fn re_comma_ws() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r",\s+").unwrap())
}

/// <title> of the page (whitespace collapsed, entities decoded).
pub fn title(html: &str) -> String {
    // ASCII lowercasing keeps byte offsets identical to the original string.
    let lower = html.to_ascii_lowercase();
    let Some(start) = lower.find("<title") else { return String::new() };
    let Some(gt) = lower[start..].find('>') else { return String::new() };
    let open_end = start + gt + 1;
    let Some(end) = lower[open_end..].find("</title>") else { return String::new() };
    let raw = html_escape::decode_html_entities(&html[open_end..open_end + end]).to_string();
    raw.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Every "quoted" / 'quoted' string and css url(...) in the page, in document order,
/// resolved against `base` when it is relative. Also understands srcset lists.
pub fn extract(base: Option<&Url>, html: &str) -> Vec<String> {
    let mut found: Vec<(usize, String)> = Vec::new();
    for re in [re_dq(), re_sq(), re_css()] {
        for c in re.captures_iter(html) {
            let m = c.get(1).unwrap();
            found.push((m.start(), m.as_str().to_string()));
        }
    }
    found.sort_by_key(|(pos, _)| *pos);

    let mut out = Vec::new();
    for (_, raw) in found {
        for cand in candidates(&raw) {
            if let Some(u) = resolve(base, &cand) {
                out.push(u);
            }
        }
    }
    out
}

fn plausible(s: &str) -> bool {
    !s.starts_with('#')
        && !s.chars().any(|c| "<>{}\"'\\".contains(c) || c.is_whitespace())
        && (s.contains('/') || s.contains('.'))
}

fn is_descriptor(t: &str) -> bool {
    match t.chars().last() {
        Some('w') | Some('x') | Some('h') => t[..t.len() - 1].parse::<f32>().is_ok(),
        _ => false,
    }
}

/// One quoted string -> zero or more URL-ish candidates.
fn candidates(raw: &str) -> Vec<String> {
    if raw.len() > 2048 {
        return vec![];
    }
    let mut s = html_escape::decode_html_entities(raw).to_string();
    if s.contains('\\') {
        s = s
            .replace("\\/", "/")
            .replace("\\u0026", "&")
            .replace("\\u002F", "/")
            .replace("\\u002f", "/");
    }
    let s = s.trim();
    if s.is_empty() {
        return vec![];
    }
    if !s.chars().any(char::is_whitespace) {
        return if plausible(s) { vec![s.to_string()] } else { vec![] };
    }

    // srcset="a.jpg 480w, b.jpg 800w"
    let mut urls: Vec<&str> = Vec::new();
    let mut has_desc = false;
    for part in re_comma_ws().split(s) {
        let toks: Vec<&str> = part.split_whitespace().collect();
        match toks.len() {
            1 => urls.push(toks[0]),
            2 if is_descriptor(toks[1]) => {
                has_desc = true;
                urls.push(toks[0]);
            }
            _ => return vec![],
        }
    }
    if has_desc {
        urls.into_iter().filter(|u| plausible(u)).map(String::from).collect()
    } else {
        vec![]
    }
}

fn resolve(base: Option<&Url>, s: &str) -> Option<String> {
    if let Ok(u) = Url::parse(s) {
        // "scheme://..." only; things like "background:url(x)" / "mailto:" / "data:" are not links
        return if s.contains("://") { Some(u.to_string()) } else { None };
    }
    match base {
        Some(b) => b.join(s).ok().map(|u| u.to_string()),
        None => Some(s.to_string()), // local file without --base: keep as written
    }
}

/// Links the crawler may follow: `<a href>`, `<area href>`, `<iframe src>`, `<frame src>`,
/// resolved against `base`, fragments removed, only things that look like web pages.
/// (Everything else found by `extract` is a download candidate.)
pub fn extract_pages(base: Option<&Url>, html: &str) -> Vec<String> {
    let mut found: Vec<(usize, String)> = Vec::new();
    for re in [re_href(), re_frame()] {
        for c in re.captures_iter(html) {
            let m = c.get(1).or_else(|| c.get(2)).or_else(|| c.get(3));
            if let Some(m) = m {
                found.push((m.start(), m.as_str().to_string()));
            }
        }
    }
    found.sort_by_key(|(pos, _)| *pos);

    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for (_, raw) in found {
        let dec = html_escape::decode_html_entities(&raw).to_string();
        let s = dec.trim();
        if s.is_empty() || s.starts_with('#') || s.len() > 2048 {
            continue;
        }
        let Some(u) = resolve(base, s) else { continue };
        let u = crate::util::strip_fragment(&u).to_string();
        if crate::filter::is_page_link(&u) && seen.insert(u.clone()) {
            out.push(u);
        }
    }
    out
}
