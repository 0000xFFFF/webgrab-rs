use crate::filter::{TYPES_CODEC, TYPES_IMAGE, TYPES_OTHER, TYPES_VIDEO};
use clap::Parser;
use std::ffi::OsString;
use std::path::PathBuf;

const EXAMPLES: &str = "\
Flags can be written with two dashes (--threads 4) or one dash (-threads 4).
Underscores work too (--skip_all).

EXAMPLES:
  webgrab https://example.com/gallery --grab --threads 8
  webgrab https://example.com/gallery --list --image --unique
  webgrab page.html --list --base https://example.com/ --video
  webgrab https://example.com --image --nodupes --export-hashes hashes.txt
  webgrab --read urls.txt --media --threads 4 --dir downloads
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
    #[arg(long, visible_aliases = ["out", "echo", "print", "output"], help_heading = "Modes")]
    pub list: bool,

    /// No downloading, just print counts (size, title, total/accepted/skipped/rejected)
    #[arg(long, visible_aliases = ["ping", "try"], help_heading = "Modes")]
    pub test: bool,

    /// Read a text file and treat every line as a URL (empty lines and '#' lines ignored)
    #[arg(long, visible_aliases = ["txt", "text"], value_name = "FILE", help_heading = "Modes")]
    pub read: Option<PathBuf>,

    /// Download URLs as they are copied to the clipboard
    #[arg(long, visible_alias = "listen", help_heading = "Modes")]
    pub watch: bool,

    // ------------------------------------------------------------- download
    /// How many files to download at the same time
    #[arg(short = 't', long, default_value_t = 1, value_name = "NUM", help_heading = "Download")]
    pub threads: usize,

    /// Save files into this directory (created if missing)
    #[arg(long, value_name = "DIR", help_heading = "Download")]
    pub dir: Option<PathBuf>,

    /// Only keep valid http(s) URLs
    #[arg(long, help_heading = "Download")]
    pub valid: bool,

    /// Only keep the first occurrence of every URL (de-duplicates the download list)
    #[arg(long, help_heading = "Download")]
    pub unique: bool,

    /// After each download hash the file; if the same content was already downloaded, delete the new file
    #[arg(long, help_heading = "Download")]
    pub nodupes: bool,

    /// Hash the files already in --dir first so --nodupes also catches those (implies --nodupes)
    #[arg(long, help_heading = "Download")]
    pub hash_existing: bool,

    /// Write "<sha256>  <filename>" for every saved file; updated live after each download
    #[arg(long, value_name = "FILE", help_heading = "Download")]
    pub export_hashes: Option<PathBuf>,

    /// Append to the --export-hashes file instead of overwriting it
    #[arg(long, help_heading = "Download")]
    pub export_append: bool,

    /// If the file exists, replace it (default: save as "name [1].ext")
    #[arg(long, help_heading = "Download")]
    pub replace: bool,

    /// If the file exists in --dir, skip the URL
    #[arg(long, help_heading = "Download")]
    pub skip: bool,
    /// Like --skip but also search subdirectories
    #[arg(long, help_heading = "Download")]
    pub skip_find: bool,
    /// Skip names listed in *.webgrab_skip files (one per line)
    #[arg(long, help_heading = "Download")]
    pub skip_file: bool,
    /// Ignore extensions when comparing file names
    #[arg(long, help_heading = "Download")]
    pub skip_ext: bool,
    /// Ignore case when comparing file names
    #[arg(long, help_heading = "Download")]
    pub skip_case: bool,
    /// Apply the skip rules before output too (useful with --list / --test)
    #[arg(long, help_heading = "Download")]
    pub skip_output: bool,
    /// --skip-find --skip-file --skip-ext --skip-case --skip-output
    #[arg(long, help_heading = "Download")]
    pub skip_all: bool,

    /// Send "User-Agent: webgrab" (fixes some 403 errors)
    #[arg(long, help_heading = "Download")]
    pub agent: bool,

    /// Send this User-Agent
    #[arg(long, value_name = "STRING", help_heading = "Download")]
    pub user_agent: Option<String>,

    /// Seconds without data before a request is abandoned
    #[arg(long, default_value_t = 60, value_name = "SECS", help_heading = "Download")]
    pub timeout: u64,

    /// Base URL used to resolve relative links when the input is a local .html file
    #[arg(long, value_name = "URL", help_heading = "Download")]
    pub base: Option<String>,

    // -------------------------------------------------------------- filters
    /// Keep only links containing one of these (comma separated, repeatable)
    #[arg(long, value_delimiter = ',', value_name = "KEYWORD", help_heading = "Filters")]
    pub only: Vec<String>,

    /// Keep only links containing ALL of these
    #[arg(long, value_delimiter = ',', value_name = "KEYWORD", help_heading = "Filters")]
    pub must: Vec<String>,

    /// Drop links containing any of these
    #[arg(long, value_delimiter = ',', value_name = "KEYWORD", help_heading = "Filters")]
    pub ignore: Vec<String>,

    /// --video --image --other
    #[arg(long, help_heading = "Filters")]
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
    #[arg(long, help_heading = "Display")]
    pub clean: bool,
    /// Number the output
    #[arg(long, help_heading = "Display")]
    pub count: bool,
    /// More detail (--count, DING lines, table output with --list, debug block at the end)
    #[arg(long, help_heading = "Display")]
    pub verbose: bool,
    /// With --list: print only the file names
    #[arg(long, help_heading = "Display")]
    pub filename: bool,
    /// Use colors
    #[arg(long, help_heading = "Display")]
    pub color: bool,
}

impl Cli {
    /// Expand the presets (--grab, --media, --skip-all, ...) into the plain options.
    pub fn resolve(&mut self) {
        if self.skip_all {
            self.skip_find = true;
            self.skip_file = true;
            self.skip_case = true;
            self.skip_ext = true;
            self.skip_output = true;
        }
        if self.verbose {
            self.count = true;
        }
        if self.hash_existing {
            self.nodupes = true;
        }
        if self.grab {
            self.clean = true;
            self.color = true;
            self.count = true;
            self.valid = true;
            self.media = true;
            self.ignore.push("thumb".into());
        }
        let mut add = |list: &[&str]| self.only.extend(list.iter().map(|s| s.to_string()));
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
        self.only.retain(|s| !s.is_empty());
        self.must.retain(|s| !s.is_empty());
        self.ignore.retain(|s| !s.is_empty());
        self.threads = self.threads.clamp(1, 256);
    }
}

/// Lets `-flag`, `--flag`, `--flag_name` and `/?` all work.
/// Single dash + more than one letter is treated as a long flag; real short flags (-t, -h, -V) are untouched.
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
