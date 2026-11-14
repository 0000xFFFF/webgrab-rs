use crate::filter::{TYPES_CODEC, TYPES_IMAGE, TYPES_OTHER, TYPES_VIDEO};
use clap::Parser;
use std::ffi::OsString;
use std::path::PathBuf;

const EXAMPLES: &str = "\
Flags can be written with two dashes (--threads 4) or one dash (-threads 4).
Hyphens and underscores are interchangeable (--skip-all = --skip_all).
Short flags (-t, -d, -R ...) take their value after a space and can't be combined.

SKIP / DUPLICATE RULES (what each one checks):
  --unique     the same URL is queued only once
  --nodupes    the same file CONTENT (sha256) is kept only once
  --skip       the same file NAME already exists in --dir
               (only the name is compared! if two different links give different files
               with the same name, --skip and --replace can treat them as one file)
  --live-csv   remembers every URL/file/hash it handled, so re-running the same command resumes

EXAMPLES:
  webgrab https://example.com/gallery --grab --threads 8
  webgrab https://example.com/gallery --list --image --unique
  webgrab https://example.com/gallery --crawl-depth 2 --image --unique -t 8 -d pics
  webgrab https://example.com/gallery -D 3 --media -u --crawl-sleep 1500 --crawl-retry-429
  webgrab https://example.com/gallery --crawl-depth 1 --list --media --unique
  webgrab page.html --list --base https://example.com/ --video
  webgrab --live-csv data.csv --nodupes --unique download_links.html
  webgrab --read urls.txt --media -t 4 -d downloads --live-csv log.csv --hash-existing -R
  webgrab \"https://www.youtube.com/watch?v=XXXXXXXXXXX\" --valid --image --unique --must maxres";

#[derive(Parser, Debug)]
#[command(
    name = "webgrab",
    version,
    about = "Web page file scanner / downloader",
    arg_required_else_help = true,
    after_help = EXAMPLES
)]
pub struct Cli {
    /// Page URL, local .html file, or a direct link to a file
    #[arg(value_name = "URL|FILE")]
    pub input: Option<String>,

    // ---------------------------------------------------------------- modes
    /// Only print the links found (no downloading). Great for reading an .html file
    #[arg(short = 'l', long, help_heading = "Modes")]
    pub list: bool,

    /// No downloading, just print counts (size, title, total/accepted/rejected)
    #[arg(short = 'T', long, help_heading = "Modes")]
    pub test: bool,

    /// Read a text file and treat every line as a URL (empty lines and '#' lines ignored)
    #[arg(short = 'r', long, value_name = "FILE", help_heading = "Modes")]
    pub read: Option<PathBuf>,

    /// Download URLs as they are copied to the clipboard
    #[arg(short = 'w', long, help_heading = "Modes")]
    pub watch: bool,

    // ------------------------------------------------------------- download
    /// How many files to download at the same time
    #[arg(short = 't', long, default_value_t = 1, value_name = "NUM", help_heading = "Download")]
    pub threads: usize,

    /// Save files into this directory (created if missing)
    #[arg(short = 'd', long, value_name = "DIR", help_heading = "Download")]
    pub dir: Option<PathBuf>,

    /// Search subdirectories too (for --skip and --hash-existing)
    #[arg(short = 'R', long, help_heading = "Download")]
    pub recursive: bool,

    /// Only keep valid http(s) URLs
    #[arg(long, help_heading = "Download")]
    pub valid: bool,

    /// Only queue every URL once
    #[arg(short = 'u', long, help_heading = "Download")]
    pub unique: bool,

    /// Keep file content unique: hash every download, delete it if the same content was already saved
    #[arg(short = 'n', long, help_heading = "Download")]
    pub nodupes: bool,

    /// Hash the files already in --dir (and in subdirectories with -R) before downloading
    ///
    /// The hashes feed --nodupes; with --live-csv they are also written to the CSV
    /// (blank url, is_dupe=0).
    #[arg(short = 'H', long, help_heading = "Download")]
    pub hash_existing: bool,

    /// Workspace/state file (CSV: url,filename,hash,is_dupe), updated live after every file
    ///
    /// If the file exists it is loaded first and only ever appended to. URLs already in it
    /// are skipped and its hashes are used by --nodupes, so re-running an interrupted
    /// command continues where it stopped. is_dupe=1 means the file had the same content as
    /// an earlier one and was deleted.
    #[arg(short = 'L', long, value_name = "FILE.csv", help_heading = "Download")]
    pub live_csv: Option<PathBuf>,

    /// If the file exists, replace it (default: save as "name [1].ext")
    ///
    /// Only the file NAME is compared: if two different links give different files with
    /// the same name, --replace will overwrite one with the other.
    #[arg(short = 'p', long, help_heading = "Download")]
    pub replace: bool,

    /// Skip a URL if a file with exactly the same NAME already exists in --dir
    ///
    /// Only the file name is checked, not the content. If two different links point to
    /// different files that happen to share a name, --skip (and --replace) can be
    /// problematic: the second one is skipped/overwritten. Use --nodupes with --live-csv
    /// to track real content. Add -R to also look in subdirectories.
    #[arg(short = 's', long, help_heading = "Download")]
    pub skip: bool,

    /// Base URL used to resolve relative links when the input is a local .html file
    #[arg(short = 'b', long, value_name = "URL", help_heading = "Download")]
    pub base: Option<String>,

    /// How deep to crawl: 0 = only the given page, 1 = also the pages it links to, 2 = and theirs ...
    ///
    /// Links are split into pages (crawled: <a href>, frames) and files (downloaded: everything
    /// else that passes your filters). Pages are followed only on the same host unless
    /// --external is given, each page once. Use --unique so a file shown on many pages is
    /// downloaded once. --ignore also applies to pages (e.g. --ignore logout). Pages are not
    /// saved unless you add --codec. Applies to a page URL / .html file, not to --read.
    #[arg(short = 'D', long, default_value_t = 0, value_name = "NUM", help_heading = "Download")]
    pub crawl_depth: usize,

    /// With --crawl-depth: wait this many milliseconds before every page request (avoids HTTP 429)
    #[arg(long, default_value_t = 0, value_name = "MS", help_heading = "Download")]
    pub crawl_sleep: u64,

    /// With --crawl-depth: when a page answers HTTP 429, wait and try again
    ///
    /// The wait doubles every time (1s, 2s, 4s ... max 60s), a Retry-After header from the
    /// server is respected if it asks for longer, and a random offset is added to every wait
    /// so the timing is never exactly the same.
    #[arg(long, help_heading = "Download")]
    pub crawl_retry_429: bool,

    /// With --crawl-retry-429: how many times to retry one page before giving up
    #[arg(long, default_value_t = 6, value_name = "NUM", help_heading = "Download")]
    pub crawl_retries: u32,

    /// With --crawl-depth: also follow links to other hosts (default: stay on the start page's host)
    #[arg(long, help_heading = "Download")]
    pub external: bool,

    /// Seconds without data before a request is abandoned
    #[arg(long, default_value_t = 60, value_name = "SECS", help_heading = "Download")]
    pub timeout: u64,

    // ---------------------------------------------------------- user agent
    /// Send "User-Agent: webgrab"
    #[arg(long, help_heading = "User-Agent")]
    pub agent: bool,
    /// Pretend to be Firefox
    #[arg(long, help_heading = "User-Agent")]
    pub agent_firefox: bool,
    /// Pretend to be Chromium (Linux)
    #[arg(long, help_heading = "User-Agent")]
    pub agent_chromium: bool,
    /// Pretend to be Google Chrome
    #[arg(long, help_heading = "User-Agent")]
    pub agent_chrome: bool,
    /// Pretend to be Brave (Chrome UA + Brave client hints)
    #[arg(long, help_heading = "User-Agent")]
    pub agent_brave: bool,
    /// Send this exact User-Agent (wins over the presets)
    #[arg(long, value_name = "STRING", help_heading = "User-Agent")]
    pub user_agent: Option<String>,

    // -------------------------------------------------------------- filters
    /// Keep only links containing ANY of these (comma separated, repeatable)
    #[arg(short = 'a', long, value_delimiter = ',', value_name = "KEYWORD", help_heading = "Filters")]
    pub any: Vec<String>,

    /// Keep only links containing ALL of these
    #[arg(short = 'm', long, value_delimiter = ',', value_name = "KEYWORD", help_heading = "Filters")]
    pub must: Vec<String>,

    /// Drop links containing any of these
    #[arg(short = 'i', long, value_delimiter = ',', value_name = "KEYWORD", help_heading = "Filters")]
    pub ignore: Vec<String>,

    /// --video --image --other
    #[arg(short = 'M', long, help_heading = "Filters")]
    pub media: bool,
    /// Video/audio extensions (.mp4 .webm .mkv .mp3 ...)
    #[arg(long, help_heading = "Filters")]
    pub video: bool,
    /// Image extensions (.jpg .png .gif .svg ...)
    #[arg(long, help_heading = "Filters")]
    pub image: bool,
    /// Other extensions (.zip .rar .exe .txt ...)
    #[arg(long, help_heading = "Filters")]
    pub other: bool,
    /// Page/code extensions (.html .css .js .php ...)
    #[arg(long, help_heading = "Filters")]
    pub codec: bool,
    /// --clean --color --count --valid --media --ignore thumb
    #[arg(long, help_heading = "Filters")]
    pub grab: bool,

    // -------------------------------------------------------------- display
    /// Only print numbers (on a single line)
    #[arg(long, help_heading = "Display")]
    pub compact: bool,
    /// Don't print errors/warnings (only successful downloads)
    #[arg(short = 'q', long, help_heading = "Display")]
    pub clean: bool,
    /// Number the output
    #[arg(short = 'c', long, help_heading = "Display")]
    pub count: bool,
    /// More detail (--count, DING lines, table output with --list, debug block at the end)
    #[arg(short = 'v', long, help_heading = "Display")]
    pub verbose: bool,
    /// With --list: print only the file names
    #[arg(short = 'f', long, help_heading = "Display")]
    pub filename: bool,
    /// Use colors
    #[arg(long, help_heading = "Display")]
    pub color: bool,
}

impl Cli {
    /// Expand the presets (--grab, --media, ...) into the plain options.
    pub fn resolve(&mut self) {
        if self.verbose {
            self.count = true;
        }
        if self.grab {
            self.clean = true;
            self.color = true;
            self.count = true;
            self.valid = true;
            self.media = true;
            self.ignore.push("thumb".into());
        }
        let mut add = |list: &[&str]| self.any.extend(list.iter().map(|s| s.to_string()));
        let (media, video, image, other, codec) =
            (self.media, self.video, self.image, self.other, self.codec);
        if media || video {
            add(TYPES_VIDEO);
        }
        if media || image {
            add(TYPES_IMAGE);
        }
        if media || other {
            add(TYPES_OTHER);
        }
        if codec {
            add(TYPES_CODEC);
        }
        self.any.retain(|s| !s.is_empty());
        self.must.retain(|s| !s.is_empty());
        self.ignore.retain(|s| !s.is_empty());
        self.threads = self.threads.clamp(1, 256);
    }
}

/// Lets `-flag`, `--flag`, `--flag_name` and `/?` all work.
/// Single dash + more than one letter is treated as a long flag; real short flags (-t, -R, -h, -V) are untouched.
pub fn normalized_args() -> Vec<OsString> {
    std::env::args_os()
        .enumerate()
        .map(|(i, a)| {
            if i == 0 {
                return a;
            }
            let Some(s) = a.to_str() else { return a };
            if s == "/?" {
                return OsString::from("--help");
            }
            let body = if let Some(r) = s.strip_prefix("--") {
                r
            } else if let Some(r) = s.strip_prefix('-') {
                if r.chars().count() > 1 && r.chars().next().map_or(false, |c| c.is_ascii_alphabetic()) {
                    r
                } else {
                    return a;
                }
            } else {
                return a;
            };
            if body.is_empty() {
                return a; // a bare "--"
            }
            let (name, rest) = match body.split_once('=') {
                Some((n, v)) => (n, format!("={}", v)),
                None => (body, String::new()),
            };
            OsString::from(format!("--{}{}", name.replace('_', "-"), rest))
        })
        .collect()
}
