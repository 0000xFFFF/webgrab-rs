use crate::cli::Cli;
use url::Url;

pub const TYPES_VIDEO: &[&str] = &[
    ".mp4", ".webm", ".avi", ".mov", ".mkv", ".flv", ".mpeg", ".mpg", ".wmv", ".mp3", ".ogg",
];
pub const TYPES_IMAGE: &[&str] = &[
    ".jpg", ".jpeg", ".jpe", ".jiff", ".jfif", ".png", ".gif", ".ico", ".svg", ".bmp", ".dib",
    ".tif", ".tiff",
];
pub const TYPES_OTHER: &[&str] = &[".zip", ".rar", ".exe", ".swf", ".dll", ".txt"];
pub const TYPES_CODEC: &[&str] = &[
    ".asp", ".aspx", ".axd", ".asx", ".asmx", ".ashx", ".css", ".cfm", ".yaws", ".swf", ".html",
    ".htm", ".xhtml", ".jhtml", ".jsp", ".jspx", ".wss", ".do", ".action", ".js", ".pl", ".php",
    ".php4", ".php3", ".phtml", ".py", ".rb", ".rhtml", ".shtml", ".xml", ".rss", ".svg", ".cgi",
    ".dll",
];

/// Extensions that are files, never web pages (used to tell crawlable links from downloads).
const NON_PAGE_EXTS: &[&str] = &[
    ".css", ".js", ".mjs", ".json", ".xml", ".rss", ".atom", ".svg", ".ico", ".map", ".woff",
    ".woff2", ".ttf", ".otf", ".eot", ".pdf", ".doc", ".docx", ".xls", ".xlsx", ".ppt", ".pptx",
    ".csv", ".gz", ".tgz", ".tar", ".bz2", ".xz", ".7z", ".iso", ".apk", ".dmg", ".msi", ".bin",
    ".torrent",
];

/// Could this link be a web page we can crawl? (http(s), and not a media/archive/asset file).
/// No extension (`/gallery/`) and unknown extensions count as pages; the crawler double-checks
/// the Content-Type before reading a response.
pub fn is_page_link(link: &str) -> bool {
    if !is_http(link) {
        return false;
    }
    match crate::util::url_ext(link) {
        None => true,
        Some(e) => {
            let e = e.as_str();
            !(TYPES_VIDEO.contains(&e)
                || TYPES_IMAGE.contains(&e)
                || TYPES_OTHER.contains(&e)
                || NON_PAGE_EXTS.contains(&e))
        }
    }
}

pub fn is_http(link: &str) -> bool {
    Url::parse(link)
        .map(|u| matches!(u.scheme(), "http" | "https"))
        .unwrap_or(false)
}

/// What the HTML element around a link says it is (`<img>` -> Image, `<video>`/`<audio>` -> Video).
/// Lets --image/--video/--media accept links that have no file extension.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Hint {
    Image,
    Video,
}

/// Keyword rules, case-insensitive substring matching (same semantics as the C# version):
///   any    = at least one must be present
///   must   = all must be present
///   ignore = none may be present
pub struct Filters {
    only: Vec<String>,
    must: Vec<String>,
    ignore: Vec<String>,
    valid: bool,
    hint_image: bool,
    hint_video: bool,
}

impl Filters {
    pub fn new(cli: &Cli) -> Self {
        let lc = |v: &Vec<String>| v.iter().map(|s| s.to_lowercase()).collect::<Vec<_>>();
        Self {
            only: lc(&cli.any),
            must: lc(&cli.must),
            ignore: lc(&cli.ignore),
            valid: cli.valid,
            hint_image: cli.media || cli.image,
            hint_video: cli.media || cli.video,
        }
    }

    /// --ignore keywords only (also applied to links the crawler would follow).
    pub fn ignored(&self, link: &str) -> bool {
        let l = link.to_lowercase();
        self.ignore.iter().any(|k| l.contains(k.as_str()))
    }

    pub fn passes(&self, link: &str, hint: Option<Hint>) -> bool {
        if link.is_empty() {
            return false;
        }
        if self.valid && !is_http(link) {
            return false;
        }
        let l = link.to_lowercase();
        if !self.only.is_empty() && !self.only.iter().any(|k| l.contains(k.as_str())) {
            // no extension/keyword match: still accepted if the element says it's wanted media
            let by_hint = match hint {
                Some(Hint::Image) => self.hint_image,
                Some(Hint::Video) => self.hint_video,
                None => false,
            };
            if !by_hint {
                return false;
            }
        }
        if !self.must.iter().all(|k| l.contains(k.as_str())) {
            return false;
        }
        if self.ignore.iter().any(|k| l.contains(k.as_str())) {
            return false;
        }
        true
    }
}
