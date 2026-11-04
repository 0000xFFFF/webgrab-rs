use crate::cli::Cli;
use crate::util::strip_ext;
use std::collections::HashSet;
use std::fs;
use std::path::Path;
use std::sync::Mutex;

/// File names that must not be downloaded again.
/// Sources: --skip (files in the download dir), --skip-find (also subdirs),
/// --skip-file (names listed in *.webgrab_skip files, one per line, '#' = comment).
pub struct Skipper {
    active: bool,
    ignore_case: bool,
    ignore_ext: bool,
    names: Mutex<HashSet<String>>,
}

fn walk(dir: &Path, recursive: bool, f: &mut dyn FnMut(&Path)) {
    if let Ok(rd) = fs::read_dir(dir) {
        for e in rd.flatten() {
            let p = e.path();
            match e.file_type() {
                Ok(ft) if ft.is_dir() => {
                    if recursive {
                        walk(&p, true, f);
                    }
                }
                Ok(_) => f(&p),
                Err(_) => {}
            }
        }
    }
}

impl Skipper {
    pub fn new(cli: &Cli, dir: &Path) -> Self {
        let s = Skipper {
            active: cli.skip || cli.skip_find || cli.skip_file,
            ignore_case: cli.skip_case,
            ignore_ext: cli.skip_ext,
            names: Mutex::new(HashSet::new()),
        };

        if cli.skip_find {
            walk(dir, true, &mut |p| s.add(&p.file_name().unwrap_or_default().to_string_lossy()));
        } else if cli.skip {
            walk(dir, false, &mut |p| s.add(&p.file_name().unwrap_or_default().to_string_lossy()));
        }

        if cli.skip_file {
            walk(dir, true, &mut |p| {
                if p.extension().map(|e| e == "webgrab_skip").unwrap_or(false) {
                    if let Ok(text) = fs::read_to_string(p) {
                        for line in text.lines() {
                            let line = line.trim();
                            if !line.is_empty() && !line.starts_with('#') {
                                s.add(line);
                            }
                        }
                    }
                }
            });
        }
        s
    }

    fn key(&self, name: &str) -> String {
        let n = if self.ignore_ext { strip_ext(name) } else { name };
        if self.ignore_case {
            n.to_lowercase()
        } else {
            n.to_string()
        }
    }

    pub fn active(&self) -> bool {
        self.active
    }

    pub fn add(&self, name: &str) {
        let k = self.key(name);
        self.names.lock().unwrap_or_else(|e| e.into_inner()).insert(k);
    }

    pub fn contains(&self, name: &str) -> bool {
        let k = self.key(name);
        self.names.lock().unwrap_or_else(|e| e.into_inner()).contains(&k)
    }
}
