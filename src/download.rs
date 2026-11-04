use crate::cli::Cli;
use crate::filter::Filters;
use crate::skip::Skipper;
use crate::stats::Stats;
use crate::ui::{Printer, K};
use crate::util::{file_name_from_url, human, split_name};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::fs::{self, File, OpenOptions};
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
    pub agent: ureq::Agent,
    pub dir: PathBuf,
    /// file names handed out during this run (so two threads never write the same name)
    reserved: Mutex<HashSet<String>>,
    /// sha256 -> file name of the first file with that content
    hashes: Mutex<HashMap<String, String>>,
    /// live hash export
    export: Mutex<Option<File>>,
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

impl<'a> Ctx<'a> {
    pub fn new(cli: &'a Cli, ui: &'a Printer, stats: &'a Stats) -> Result<Self, String> {
        let dir = cli.dir.clone().unwrap_or_else(|| PathBuf::from("."));
        let downloading = !cli.list && !cli.test;

        if downloading {
            fs::create_dir_all(&dir).map_err(|e| format!("can't create {}: {}", dir.display(), e))?;
        }

        let mut ab = ureq::AgentBuilder::new()
            .timeout_connect(Duration::from_secs(15))
            .timeout_read(Duration::from_secs(cli.timeout.max(1)));
        if let Some(ua) = &cli.user_agent {
            ab = ab.user_agent(ua);
        } else if cli.agent {
            ab = ab.user_agent("webgrab");
        }

        // live hash export
        let export = match (&cli.export_hashes, downloading) {
            (Some(p), true) => {
                let mut oo = OpenOptions::new();
                oo.create(true).write(true);
                if cli.export_append {
                    oo.append(true);
                } else {
                    oo.truncate(true);
                }
                Some(oo.open(p).map_err(|e| format!("can't open {}: {}", p.display(), e))?)
            }
            _ => None,
        };

        let ctx = Ctx {
            cli,
            ui,
            stats,
            filters: Filters::new(cli),
            skipper: Skipper::new(cli, &dir),
            agent: ab.build(),
            dir,
            reserved: Mutex::new(HashSet::new()),
            hashes: Mutex::new(HashMap::new()),
            export: Mutex::new(export),
        };

        if cli.hash_existing && downloading {
            ctx.seed_hashes();
        }
        Ok(ctx)
    }

    /// --hash-existing: remember the hashes of files already in the download directory.
    fn seed_hashes(&self) {
        let mut n = 0usize;
        if let Ok(rd) = fs::read_dir(&self.dir) {
            let mut map = self.hashes.lock().unwrap_or_else(|e| e.into_inner());
            for e in rd.flatten() {
                let p = e.path();
                let is_file = e.file_type().map(|t| t.is_file()).unwrap_or(false);
                let name = e.file_name().to_string_lossy().to_string();
                if !is_file || name.ends_with(".webgrab_download") {
                    continue;
                }
                if let Ok(h) = hex_of_file(&p) {
                    map.entry(h).or_insert(name);
                    n += 1;
                }
            }
        }
        self.ui.say(K::Inf, &format!("hashed {} existing file(s) in {}", n, self.dir.display()));
    }

    /// Filter rules + skip_output. Updates the counters.
    pub fn accept(&self, link: &str) -> bool {
        if !self.filters.passes(link) {
            self.stats.disposed.fetch_add(1, Relaxed);
            return false;
        }
        if self.cli.skip_output && self.skipper.active() {
            if let Some(name) = file_name_from_url(link) {
                if self.skipper.contains(&name) {
                    self.stats.skipped.fetch_add(1, Relaxed);
                    return false;
                }
            }
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

    fn fail(&self, url: &str, name: &str, msg: &str) -> Result<Outcome, String> {
        self.release(name);
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

        // --skip rules
        if self.skipper.active() && self.skipper.contains(&name) {
            self.stats.skipped.fetch_add(1, Relaxed);
            self.note(K::War, &format!("WAR[SKIPPED] {} ({})", url, name));
            return Ok(Outcome::Skipped);
        }

        let (final_name, clash) = self.reserve(&name);
        if self.skipper.active() {
            self.skipper.add(&final_name);
        }
        let final_path = self.dir.join(&final_name);
        let tmp = self.dir.join(format!("{}.webgrab_download", final_name));

        // request
        let resp = match self.agent.get(url).call() {
            Ok(r) => r,
            Err(ureq::Error::Status(code, _)) => return self.fail(url, &final_name, &format!("HTTP {}", code)),
            Err(e) => return self.fail(url, &final_name, &e.to_string()),
        };

        let want_hash = cli.nodupes || cli.export_hashes.is_some();
        let (size, hash) = match self.stream(resp.into_reader(), &tmp, want_hash) {
            Ok(v) => v,
            Err(e) => {
                let _ = fs::remove_file(&tmp);
                return self.fail(url, &final_name, &format!("IO {}", e));
            }
        };

        // --nodupes: same content already downloaded -> delete the new file
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
            return self.fail(url, &final_name, &format!("IO {}", e));
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

        // live export: written + flushed + synced after every file
        if let Some(h) = &hash {
            let mut g = self.export.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(f) = g.as_mut() {
                let r = writeln!(f, "{}  {}", h, final_name).and_then(|_| f.flush()).and_then(|_| f.sync_data());
                if let Err(e) = r {
                    drop(g);
                    self.note(K::Err, &format!("ERR[export-hashes] {}", e));
                }
            }
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
