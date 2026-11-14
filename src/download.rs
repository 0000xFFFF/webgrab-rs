use crate::cli::Cli;
use crate::csvlog::CsvLog;
use crate::csvlog::{MEDIA, PAGE};
use crate::filter::{Filters, Hint};
use crate::skip::Skipper;
use crate::stats::Stats;
use crate::ua;
use crate::ui::{Printer, K};
use crate::util::{file_name_from_url, human, split_name, walk_files};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering::Relaxed;
use std::sync::Mutex;
use std::time::Duration;

pub enum Outcome {
    Downloaded,
    Skipped,
    HashDupe,
}

#[derive(PartialEq)]
enum Clash {
    None,
    Renamed,
    Replaced,
}

/// Everything a download needs. Shared by all worker threads.
pub struct Ctx<'a> {
    pub cli: &'a Cli,
    pub ui: &'a Printer,
    pub stats: &'a Stats,
    pub filters: Filters,
    pub skipper: Skipper,
    agent: ureq::Agent,
    headers: Vec<(String, String)>,
    pub dir: PathBuf,
    /// file names handed out during this run (so two threads never write the same name)
    reserved: Mutex<HashSet<String>>,
    /// sha256 -> file name of the first file with that content
    hashes: Mutex<HashMap<String, String>>,
    /// --live-csv workspace
    csv: Option<CsvLog>,
}

fn hex_of_file(path: &Path) -> io::Result<String> {
    let mut f = File::open(path)?;
    let mut h = Sha256::new();
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Ok(format!("{:x}", h.finalize()))
}

pub fn sha256_hex(data: &[u8]) -> String {
    format!("{:x}", Sha256::digest(data))
}

/// File extension for a response Content-Type (used when the URL has none).
fn ext_for_mime(ct: &str) -> Option<&'static str> {
    let m = ct.split(';').next().unwrap_or("").trim().to_lowercase();
    Some(match m.as_str() {
        "image/jpeg" | "image/jpg" | "image/pjpeg" => ".jpg",
        "image/png" => ".png",
        "image/gif" => ".gif",
        "image/webp" => ".webp",
        "image/avif" => ".avif",
        "image/svg+xml" => ".svg",
        "image/bmp" => ".bmp",
        "image/tiff" => ".tif",
        "image/x-icon" | "image/vnd.microsoft.icon" => ".ico",
        "video/mp4" => ".mp4",
        "video/webm" => ".webm",
        "video/quicktime" => ".mov",
        "video/x-matroska" => ".mkv",
        "video/mpeg" => ".mpg",
        "video/x-flv" => ".flv",
        "video/ogg" => ".ogv",
        "video/x-msvideo" => ".avi",
        "audio/mpeg" | "audio/mp3" => ".mp3",
        "audio/ogg" => ".ogg",
        "audio/wav" | "audio/x-wav" => ".wav",
        "audio/webm" => ".weba",
        "audio/aac" | "audio/mp4" | "audio/x-m4a" => ".m4a",
        _ => return None,
    })
}

impl<'a> Ctx<'a> {
    pub fn new(cli: &'a Cli, ui: &'a Printer, stats: &'a Stats) -> Result<Self, String> {
        let dir = cli.dir.clone().unwrap_or_else(|| PathBuf::from("."));
        let downloading = !cli.list && !cli.test;

        if downloading {
            fs::create_dir_all(&dir).map_err(|e| format!("can't create {}: {}", dir.display(), e))?;
        }

        let agent = ureq::AgentBuilder::new()
            .timeout_connect(Duration::from_secs(15))
            .timeout_read(Duration::from_secs(cli.timeout.max(1)))
            .build();

        let csv = match (&cli.live_csv, downloading) {
            (Some(p), true) => Some(CsvLog::open(p)?),
            _ => None,
        };

        let mut hashes = HashMap::new();
        if let Some(c) = &csv {
            for (h, name) in c.loaded_hashes() {
                hashes.entry(h.clone()).or_insert_with(|| name.clone());
            }
            if c.migrated {
                ui.say(K::War, "WAR[live-csv] old 4-column csv converted to type,url,name,hash,is_dupe (copy kept as .legacy.bak)");
            }
            ui.say(
                K::Inf,
                &format!(
                    "loaded {} row(s) from {}{}",
                    c.loaded_rows,
                    cli.live_csv.as_ref().unwrap().display(),
                    if c.bad_rows > 0 { format!(" ({} unreadable row(s) ignored)", c.bad_rows) } else { String::new() }
                ),
            );
        }

        let ctx = Ctx {
            cli,
            ui,
            stats,
            filters: Filters::new(cli),
            skipper: Skipper::new(cli, &dir),
            agent,
            headers: ua::headers(cli),
            dir,
            reserved: Mutex::new(HashSet::new()),
            hashes: Mutex::new(hashes),
            csv,
        };

        if cli.hash_existing && downloading {
            ctx.seed_hashes();
        }
        Ok(ctx)
    }

    /// GET request with the User-Agent / client-hint headers applied.
    pub fn get(&self, url: &str) -> ureq::Request {
        let mut r = self.agent.get(url);
        for (k, v) in &self.headers {
            r = r.set(k, v);
        }
        r
    }

    /// --hash-existing: hash the files already in --dir (and below with -R).
    /// Files already recorded in the CSV are not hashed again; new ones are added to it
    /// with a blank url and is_dupe=0.
    fn seed_hashes(&self) {
        let csv_path = self.cli.live_csv.as_ref().and_then(|p| p.canonicalize().ok());
        let (mut hashed, mut known) = (0usize, 0usize);

        for p in walk_files(&self.dir, self.cli.recursive) {
            let rel = p.strip_prefix(&self.dir).unwrap_or(&p).to_string_lossy().replace('\\', "/");
            if rel.ends_with(".webgrab_download") {
                continue;
            }
            if let (Some(c), Ok(me)) = (&csv_path, p.canonicalize()) {
                if *c == me {
                    continue; // the workspace file itself
                }
            }
            if let Some(c) = &self.csv {
                if c.has_loaded_name(&rel) {
                    known += 1;
                    continue;
                }
            }
            let Ok(h) = hex_of_file(&p) else { continue };
            self.hashes.lock().unwrap_or_else(|e| e.into_inner()).entry(h.clone()).or_insert_with(|| rel.clone());
            if let Some(c) = &self.csv {
                if let Err(e) = c.write_row(MEDIA, "", &rel, &h, false) {
                    self.note(K::Err, &format!("ERR[live-csv] {}", e));
                }
            }
            hashed += 1;
        }
        self.ui.say(
            K::Inf,
            &format!("hash-existing: hashed {} file(s), {} already in csv ({})", hashed, known, self.dir.display()),
        );
    }

    /// Filter rules. Updates the counters.
    pub fn accept(&self, link: &str, hint: Option<Hint>) -> bool {
        if !self.filters.passes(link, hint) {
            self.stats.disposed.fetch_add(1, Relaxed);
            return false;
        }
        self.stats.accepted.fetch_add(1, Relaxed);
        true
    }

    /// Warnings/errors are hidden with --clean (and --compact).
    fn note(&self, k: K, text: &str) {
        if !self.cli.clean {
            self.ui.say(k, text);
        }
    }

    fn log_row(&self, url: &str, name: &str, hash: &str, dupe: bool) {
        if let Some(c) = &self.csv {
            if let Err(e) = c.write_row(MEDIA, url, name, hash, dupe) {
                self.note(K::Err, &format!("ERR[live-csv] {}", e));
            }
        }
    }

    // ---- crawled pages in the --live-csv workspace

    /// Is the page already in the CSV?
    pub fn page_seen(&self, url: &str) -> bool {
        self.csv.as_ref().map_or(false, |c| c.has_page(url))
    }

    /// Is the CSV hashing pages?
    pub fn tracks_pages(&self) -> bool {
        self.csv.is_some()
    }

    /// URL of an earlier page with exactly this content.
    pub fn page_hash_owner(&self, hash: &str) -> Option<String> {
        self.csv.as_ref().and_then(|c| c.page_hash_owner(hash))
    }

    /// Add a crawled page: name = its title (or the last part of its URL if it has none).
    pub fn log_page(&self, url: &str, title: &str, hash: &str, dupe: bool) {
        let Some(c) = &self.csv else { return };
        let name = if !title.is_empty() {
            title.to_string()
        } else {
            file_name_from_url(url)
                .or_else(|| url::Url::parse(url).ok().and_then(|u| u.host_str().map(String::from)))
                .unwrap_or_else(|| url.to_string())
        };
        if let Err(e) = c.write_row(PAGE, url, &name, hash, dupe) {
            self.note(K::Err, &format!("ERR[live-csv] {}", e));
        }
    }

    /// Pick the final file name (thread safe). Returns the name and what happened.
    fn reserve(&self, name: &str) -> (String, Clash) {
        let mut res = self.reserved.lock().unwrap_or_else(|e| e.into_inner());
        let taken = |c: &str, res: &HashSet<String>| res.contains(c) || self.dir.join(c).exists();

        let mut candidate = name.to_string();
        let mut clash = Clash::None;

        if taken(name, &res) {
            if self.cli.replace && !res.contains(name) {
                clash = Clash::Replaced; // old file from an earlier run: overwrite it
            } else {
                let (stem, ext) = split_name(name);
                let mut c = 1;
                loop {
                    candidate = format!("{} [{}]{}", stem, c, ext);
                    if !taken(&candidate, &res) {
                        break;
                    }
                    c += 1;
                }
                clash = Clash::Renamed;
            }
        }
        res.insert(candidate.clone());
        (candidate, clash)
    }

    fn release(&self, name: &str) {
        self.reserved.lock().unwrap_or_else(|e| e.into_inner()).remove(name);
    }

    fn fail(&self, url: &str, name: Option<&str>, msg: &str) -> Result<Outcome, String> {
        if let Some(n) = name {
            self.release(n);
        }
        self.stats.failed.fetch_add(1, Relaxed);
        self.note(K::Err, &format!("ERR[{}] {}", msg, url));
        Err(msg.to_string())
    }

    /// Stream the body to `tmp`, hashing on the fly when needed.
    fn stream(&self, mut reader: impl Read, tmp: &Path, want_hash: bool) -> io::Result<(u64, Option<String>)> {
        let mut file = File::create(tmp)?;
        let mut hasher = want_hash.then(Sha256::new);
        let mut buf = vec![0u8; 64 * 1024];
        let mut size = 0u64;
        loop {
            let n = reader.read(&mut buf)?;
            if n == 0 {
                break;
            }
            file.write_all(&buf[..n])?;
            if let Some(h) = hasher.as_mut() {
                h.update(&buf[..n]);
            }
            size += n as u64;
        }
        file.flush()?;
        Ok((size, hasher.map(|h| format!("{:x}", h.finalize()))))
    }

    /// Download one URL. Safe to call from many threads.
    pub fn download(&self, url: &str) -> Result<Outcome, String> {
        let n = self.stats.processed.fetch_add(1, Relaxed) + 1;
        let cli = self.cli;

        let name = file_name_from_url(url).unwrap_or_else(|| format!("download_{}", n));
        if cli.verbose {
            self.ui.say(K::Cur, &format!("DING[{}]: {} ({})", n, url, name));
        }

        // resume: this URL is already in the --live-csv workspace
        if let Some(c) = &self.csv {
            if c.has_media_url(url) {
                self.stats.skipped.fetch_add(1, Relaxed);
                self.note(K::War, &format!("WAR[SKIPPED] {} (already in csv)", url));
                return Ok(Outcome::Skipped);
            }
        }

        // --skip: same file name already exists
        if self.skipper.active() && self.skipper.contains(&name) {
            self.stats.skipped.fetch_add(1, Relaxed);
            self.note(K::War, &format!("WAR[SKIPPED] {} ({})", url, name));
            return Ok(Outcome::Skipped);
        }

        let resp = match self.get(url).call() {
            Ok(r) => r,
            Err(ureq::Error::Status(code, _)) => return self.fail(url, None, &format!("HTTP {}", code)),
            Err(e) => return self.fail(url, None, &e.to_string()),
        };

        // URL without an extension (".../image?id=5"): take it from the Content-Type
        let name = match resp.header("content-type").and_then(ext_for_mime) {
            Some(ext) if split_name(&name).1.is_empty() => format!("{}{}", name, ext),
            _ => name,
        };

        let (final_name, clash) = self.reserve(&name);
        let final_path = self.dir.join(&final_name);
        let tmp = self.dir.join(format!("{}.webgrab_download", final_name));

        let want_hash = cli.nodupes || self.csv.is_some();
        let (size, hash) = match self.stream(resp.into_reader(), &tmp, want_hash) {
            Ok(v) => v,
            Err(e) => {
                let _ = fs::remove_file(&tmp);
                return self.fail(url, Some(&final_name), &format!("IO {}", e));
            }
        };

        // --nodupes: same content already saved -> delete the new file
        if cli.nodupes {
            if let Some(h) = &hash {
                let existing = {
                    let mut map = self.hashes.lock().unwrap_or_else(|e| e.into_inner());
                    match map.get(h) {
                        Some(o) => Some(o.clone()),
                        None => {
                            map.insert(h.clone(), final_name.clone());
                            None
                        }
                    }
                };
                if let Some(orig) = existing {
                    let _ = fs::remove_file(&tmp);
                    self.release(&final_name);
                    self.stats.hash_dupes.fetch_add(1, Relaxed);
                    self.log_row(url, &final_name, h, true);
                    self.note(K::War, &format!("WAR[DUPLICATE] {} (same content as {}) deleted", url, orig));
                    return Ok(Outcome::HashDupe);
                }
            }
        }

        if let Err(e) = fs::rename(&tmp, &final_path) {
            let _ = fs::remove_file(&tmp);
            if let (true, Some(h)) = (cli.nodupes, &hash) {
                self.hashes.lock().unwrap_or_else(|e| e.into_inner()).remove(h);
            }
            return self.fail(url, Some(&final_name), &format!("IO {}", e));
        }
        if self.skipper.active() {
            self.skipper.add(&final_name);
        }

        match clash {
            Clash::Renamed => {
                self.stats.renamed.fetch_add(1, Relaxed);
            }
            Clash::Replaced => {
                self.stats.replaced.fetch_add(1, Relaxed);
            }
            Clash::None => {}
        }
        let d = self.stats.dled.fetch_add(1, Relaxed) + 1;
        self.stats.bytes.fetch_add(size, Relaxed);

        // live workspace: written + synced after every file
        if let Some(h) = &hash {
            self.log_row(url, &final_name, h, false);
        }

        let tag = match clash {
            Clash::Renamed => " WAR[RENAMED]",
            Clash::Replaced => " WAR[REPLACED]",
            Clash::None => "",
        };
        let idx = if cli.count { format!("[{}]", d) } else { String::new() };
        self.ui.say(
            K::Pri,
            &format!("DLED{}: {} ({}) ({} ({} bytes)){}", idx, url, final_name, human(size as f64), size, tag),
        );
        Ok(Outcome::Downloaded)
    }
}
