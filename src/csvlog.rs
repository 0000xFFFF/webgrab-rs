use std::collections::HashSet;
use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::Path;
use std::sync::Mutex;

const HEADER: [&str; 4] = ["url", "filename", "hash", "is_dupe"];

/// --live-csv workspace: `url,filename,hash,is_dupe`.
/// Loaded on startup, then only ever appended to (each row is written, flushed and synced at once,
/// so an interrupted run leaves a complete record of everything finished so far).
pub struct CsvLog {
    file: Mutex<File>,
    urls: Mutex<HashSet<String>>,
    /// (hash, filename) of every loaded is_dupe=0 row
    loaded_hashes: Vec<(String, String)>,
    /// filenames of every loaded is_dupe=0 row
    loaded_names: HashSet<String>,
    pub loaded_rows: usize,
    pub bad_rows: usize,
}

impl CsvLog {
    pub fn open(path: &Path) -> Result<CsvLog, String> {
        let err = |e: &dyn std::fmt::Display| format!("--live-csv {}: {}", path.display(), e);

        let mut urls = HashSet::new();
        let mut loaded_hashes = Vec::new();
        let mut loaded_names = HashSet::new();
        let (mut rows, mut bad) = (0usize, 0usize);

        let existing_len = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
        if existing_len > 0 {
            let mut rdr = csv::ReaderBuilder::new()
                .has_headers(false)
                .flexible(true)
                .from_path(path)
                .map_err(|e| err(&e))?;
            for rec in rdr.records() {
                let Ok(rec) = rec else {
                    bad += 1;
                    continue;
                };
                if rec.get(0) == Some("url") && rec.get(2) == Some("hash") {
                    continue; // header row
                }
                let (url, name, hash) = (
                    rec.get(0).unwrap_or("").trim(),
                    rec.get(1).unwrap_or("").trim(),
                    rec.get(2).unwrap_or("").trim(),
                );
                if name.is_empty() && url.is_empty() {
                    bad += 1;
                    continue;
                }
                let dupe = rec.get(3).map(|s| s.trim() == "1").unwrap_or(false);
                rows += 1;
                if !url.is_empty() {
                    urls.insert(url.to_string());
                }
                if !dupe {
                    if !hash.is_empty() {
                        loaded_hashes.push((hash.to_string(), name.to_string()));
                    }
                    loaded_names.insert(name.to_string());
                }
            }
        }

        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .read(true)
            .open(path)
            .map_err(|e| err(&e))?;

        if existing_len == 0 {
            let mut w = csv::WriterBuilder::new().has_headers(false).from_writer(Vec::new());
            w.write_record(HEADER).map_err(|e| err(&e))?;
            let buf = w.into_inner().map_err(|e| err(&e))?;
            file.write_all(&buf).map_err(|e| err(&e))?;
            file.sync_data().map_err(|e| err(&e))?;
        } else {
            // a run killed mid-write may have left a last line without newline
            let mut last = [0u8; 1];
            file.seek(SeekFrom::End(-1)).map_err(|e| err(&e))?;
            file.read_exact(&mut last).map_err(|e| err(&e))?;
            if last[0] != b'\n' {
                file.write_all(b"\n").map_err(|e| err(&e))?;
            }
        }

        Ok(CsvLog {
            file: Mutex::new(file),
            urls: Mutex::new(urls),
            loaded_hashes,
            loaded_names,
            loaded_rows: rows,
            bad_rows: bad,
        })
    }

    pub fn has_url(&self, url: &str) -> bool {
        self.urls.lock().unwrap_or_else(|e| e.into_inner()).contains(url)
    }

    pub fn loaded_hashes(&self) -> &[(String, String)] {
        &self.loaded_hashes
    }

    pub fn has_loaded_name(&self, name: &str) -> bool {
        self.loaded_names.contains(name)
    }

    /// Append one row and make sure it is on disk. Fields containing commas, quotes or
    /// newlines are quoted/escaped by the csv writer.
    pub fn write_row(&self, url: &str, filename: &str, hash: &str, dupe: bool) -> std::io::Result<()> {
        let mut w = csv::WriterBuilder::new().has_headers(false).from_writer(Vec::new());
        w.write_record([url, filename, hash, if dupe { "1" } else { "0" }])?;
        let buf = w.into_inner().map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))?;

        let mut f = self.file.lock().unwrap_or_else(|e| e.into_inner());
        f.write_all(&buf)?;
        f.flush()?;
        f.sync_data()?;
        drop(f);

        if !url.is_empty() {
            self.urls.lock().unwrap_or_else(|e| e.into_inner()).insert(url.to_string());
        }
        Ok(())
    }
}
