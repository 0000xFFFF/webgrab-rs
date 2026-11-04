mod cli;
mod download;
mod filter;
mod scan;
mod skip;
mod stats;
mod ui;
mod util;

use clap::Parser;
use cli::Cli;
use download::Ctx;
use stats::Stats;
use std::collections::HashSet;
use std::io::Read;
use std::path::Path;
use std::process::ExitCode;
use std::sync::atomic::Ordering::Relaxed;
use std::sync::atomic::{AtomicBool, AtomicUsize};
use std::time::Duration;
use ui::{Printer, K};
use url::Url;
use util::{file_name_from_url, human, url_ext};

fn main() -> ExitCode {
    #[cfg(windows)]
    {
        let _ = enable_ansi_support::enable_ansi_support();
    }
    let mut cli = Cli::parse_from(cli::normalized_args());
    cli.resolve();
    ExitCode::from(run(&cli))
}

fn run(cli: &Cli) -> u8 {
    let ui = Printer::new(cli.color, cli.compact);
    let stats = Stats::default();
    let ctx = match Ctx::new(cli, &ui, &stats) {
        Ok(c) => c,
        Err(e) => {
            ui.always(K::Cer, &e);
            return 1;
        }
    };

    if cli.watch {
        watch(&ctx)
    } else if let Some(p) = &cli.read {
        read_mode(&ctx, p)
    } else if let Some(input) = &cli.input {
        page_mode(&ctx, input)
    } else {
        ui.always(K::Cer, "no input: give a URL, an .html file, --read <file> or --watch");
        1
    }
}

// ------------------------------------------------------------------ page

fn is_direct_file(input: &str) -> bool {
    use filter::{TYPES_IMAGE, TYPES_OTHER, TYPES_VIDEO};
    match url_ext(input) {
        Some(e) => [TYPES_VIDEO, TYPES_IMAGE, TYPES_OTHER]
            .iter()
            .any(|l| l.contains(&e.as_str())),
        None => false,
    }
}

fn page_mode(ctx: &Ctx, input: &str) -> u8 {
    let (cli, ui, stats) = (ctx.cli, ctx.ui, ctx.stats);
    let dl_mode = !cli.list && !cli.test;
    let is_file = Path::new(input).is_file();

    if !is_file && !filter::is_http(input) {
        ui.always(K::Cer, &format!("not an url or file: {}", input));
        return 1;
    }

    // direct link to a media file: download it (or just list it) and stop
    if !is_file && is_direct_file(input) {
        if cli.list {
            ui.plain(input);
            return 0;
        }
        if dl_mode {
            stats.total.store(1, Relaxed);
            stats.accepted.store(1, Relaxed);
            return finish_downloads(ctx, &[input.to_string()]);
        }
    }

    let mut base: Option<Url> = cli.base.as_deref().and_then(|b| Url::parse(b).ok());
    if cli.base.is_some() && base.is_none() {
        ui.always(K::Cer, "--base is not a valid URL");
        return 1;
    }

    let html: Vec<u8> = if is_file {
        match std::fs::read(input) {
            Ok(b) => b,
            Err(e) => {
                ui.always(K::Cer, &format!("ERR[IO] {}: {}", input, e));
                return 1;
            }
        }
    } else {
        if dl_mode {
            ui.say(K::Def, "");
            ui.say(K::Top, "===[ DOWNLOADING HTML ]");
            ui.say(K::Cur, &format!("DING..: {}", input));
        }
        match ctx.agent.get(input).call() {
            Ok(resp) => {
                base = Url::parse(resp.get_url()).ok(); // after redirects
                let mut buf = Vec::new();
                if let Err(e) = resp.into_reader().take(128 << 20).read_to_end(&mut buf) {
                    ui.always(K::Cer, &format!("ERR[IO] {}", e));
                    return 1;
                }
                buf
            }
            Err(ureq::Error::Status(code, _)) => {
                ui.always(K::Cer, &format!("ERR[HTTP {}] {}", code, input));
                return 1;
            }
            Err(e) => {
                ui.always(K::Cer, &format!("ERR[{}] {}", e, input));
                return 1;
            }
        }
    };

    let text = String::from_utf8_lossy(&html);
    let title = scan::title(&text);

    if cli.test {
        ui.always(K::Top, "===[ TESTING WEBSITE ]");
        ui.always(K::Pri, &format!("html size (DLED)..: {} ({} bytes)", human(html.len() as f64), html.len()));
        ui.always(K::Inf, &format!("title from html...: {}", title));
    } else if dl_mode {
        ui.say(K::Pri, &format!("DLED..: {} ({} bytes)", human(html.len() as f64), html.len()));
        ui.say(K::Def, "");
        ui.say(K::Top, "===[ SCANNING HTML ]");
        ui.say(K::Inf, &format!("title from html.: {}", title));
    }

    let all = scan::extract(base.as_ref(), &text);
    drop(text);
    stats.total.store(all.len(), Relaxed);
    let all = unique(ctx, all);

    if dl_mode {
        ui.say(K::Inf, &format!("total items ....: {}", stats.total.load(Relaxed)));
    }
    let links: Vec<String> = all.into_iter().filter(|l| ctx.accept(l)).collect();
    if dl_mode {
        ui.say(K::Inf, &format!("accepted items..: {}", stats.accepted.load(Relaxed)));
    }

    finish(ctx, links)
}

// ------------------------------------------------------------- --read file

fn read_mode(ctx: &Ctx, path: &Path) -> u8 {
    let (cli, ui, stats) = (ctx.cli, ctx.ui, ctx.stats);
    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) => {
            ui.always(K::Cer, &format!("ERR[IO] {}: {}", path.display(), e));
            return 1;
        }
    };
    ui.say(K::Def, "");
    ui.say(K::Top, "===[ SCANNING TEXT FILE ]");
    let lines: Vec<String> = text
        .lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(String::from)
        .collect();
    stats.total.store(lines.len(), Relaxed);
    if !cli.list && !cli.test {
        ui.say(K::Inf, &format!("total items....: {}", lines.len()));
    }
    let lines = unique(ctx, lines);
    let links: Vec<String> = lines.into_iter().filter(|l| ctx.accept(l)).collect();
    if !cli.list && !cli.test {
        ui.say(K::Inf, &format!("accepted items.: {}", links.len()));
    }
    finish(ctx, links)
}

// ------------------------------------------------------------------ watch

#[cfg(feature = "clipboard")]
fn watch(ctx: &Ctx) -> u8 {
    let ui = ctx.ui;
    let mut cb = match arboard::Clipboard::new() {
        Ok(c) => c,
        Err(e) => {
            ui.always(K::Cer, &format!("ERR[clipboard] {}", e));
            return 1;
        }
    };
    ui.say(K::Def, "");
    ui.say(K::Top, "===[ DOWNLOADING COPIED URLS ]  (Ctrl+C to stop)");

    // whatever is on the clipboard right now is not "newly copied"
    let mut past = cb.get_text().unwrap_or_default();
    let mut seen: HashSet<String> = HashSet::new();
    loop {
        std::thread::sleep(Duration::from_millis(100));
        let clip = match cb.get_text() {
            Ok(c) => c,
            Err(_) => continue,
        };
        if clip == past {
            continue;
        }
        past = clip.clone();
        let clip = clip.trim();
        if clip.is_empty() {
            continue;
        }
        if ctx.cli.unique && !seen.insert(clip.to_string()) {
            ctx.stats.disposed.fetch_add(1, Relaxed);
            continue;
        }
        if ctx.accept(clip) {
            let _ = ctx.download(clip);
        }
    }
}

#[cfg(not(feature = "clipboard"))]
fn watch(ctx: &Ctx) -> u8 {
    ctx.ui.always(K::Cer, "this build has no clipboard support (rebuild with the `clipboard` feature)");
    1
}

// ----------------------------------------------------------- shared steps

/// --unique: keep only the first occurrence of every URL.
fn unique(ctx: &Ctx, items: Vec<String>) -> Vec<String> {
    if !ctx.cli.unique {
        return items;
    }
    let before = items.len();
    let mut seen = HashSet::new();
    let out: Vec<String> = items.into_iter().filter(|u| seen.insert(u.clone())).collect();
    ctx.stats.disposed.fetch_add(before - out.len(), Relaxed);
    out
}

fn finish(ctx: &Ctx, links: Vec<String>) -> u8 {
    if ctx.cli.list {
        print_list(ctx, &links);
        0
    } else if ctx.cli.test {
        let s = ctx.stats;
        ctx.ui.always(K::Inf, &format!("total items.......: {}", s.total.load(Relaxed)));
        ctx.ui.always(K::Inf, &format!("accepted items....: {}", s.accepted.load(Relaxed)));
        ctx.ui.always(K::War, &format!("skipped/found.....: {}", s.skipped.load(Relaxed)));
        ctx.ui.always(K::Eri, &format!("disposed/rejected.: {}", s.disposed.load(Relaxed)));
        0
    } else {
        finish_downloads(ctx, &links)
    }
}

fn print_list(ctx: &Ctx, links: &[String]) {
    let cli = ctx.cli;
    let ui = ctx.ui;
    if cli.verbose {
        let w_num = links.len().to_string().len().max(3);
        let w_url = links.iter().map(|l| l.len()).max().unwrap_or(0).max(3);
        ui.plain(&format!("{:<w_num$} {:<w_url$} {}", "NUM", "URL", "FILENAME"));
        for (i, l) in links.iter().enumerate() {
            let n = file_name_from_url(l).unwrap_or_default();
            ui.plain(&format!("{:<w_num$} {:<w_url$} {}", i + 1, l, n));
        }
        ctx.stats.processed.store(links.len(), Relaxed);
        print_debug(ctx);
    } else if cli.filename {
        for l in links {
            if let Some(n) = file_name_from_url(l) {
                ui.plain(&n);
            }
        }
    } else if cli.count {
        for (i, l) in links.iter().enumerate() {
            ui.plain(&format!("[{}] {}", i + 1, l));
        }
    } else {
        for l in links {
            ui.plain(l);
        }
    }
}

fn finish_downloads(ctx: &Ctx, urls: &[String]) -> u8 {
    ctx.ui.say(K::Def, "");
    ctx.ui.say(K::Top, "===[ DOWNLOADING ]");
    run_downloads(ctx, urls);

    if ctx.cli.compact {
        ctx.ui.ticker(&compact_line(ctx.stats));
        ctx.ui.always(K::Def, "");
    } else {
        print_debug(ctx);
    }
    if ctx.stats.failed.load(Relaxed) > 0 {
        2
    } else {
        0
    }
}

/// Worker pool: `--threads` threads pull URLs from a shared index.
fn run_downloads(ctx: &Ctx, urls: &[String]) {
    let threads = ctx.cli.threads.min(urls.len()).max(1);
    let next = AtomicUsize::new(0);
    let done = AtomicBool::new(false);

    std::thread::scope(|outer| {
        if ctx.cli.compact {
            outer.spawn(|| {
                while !done.load(Relaxed) {
                    ctx.ui.ticker(&compact_line(ctx.stats));
                    std::thread::sleep(Duration::from_millis(50));
                }
            });
        }
        std::thread::scope(|inner| {
            for _ in 0..threads {
                inner.spawn(|| loop {
                    let i = next.fetch_add(1, Relaxed);
                    if i >= urls.len() {
                        break;
                    }
                    let _ = ctx.download(&urls[i]);
                });
            }
        });
        done.store(true, Relaxed);
    });
}

fn compact_line(s: &Stats) -> String {
    let g = |a: &AtomicUsize| a.load(Relaxed);
    let mut parts = Vec::new();
    let mut add = |label: &str, v: usize| {
        if v != 0 {
            parts.push(format!("{}: {}", label, v));
        }
    };
    add("total", g(&s.total));
    add("acptd", g(&s.accepted));
    add("count", g(&s.processed));
    add("dled", g(&s.dled));
    add("renamed", g(&s.renamed));
    add("replc", g(&s.replaced));
    add("hdupe", g(&s.hash_dupes));
    add("skip", g(&s.skipped));
    add("junk", g(&s.disposed));
    add("fail", g(&s.failed));
    parts.join(" ")
}

fn print_debug(ctx: &Ctx) {
    let (ui, s) = (ctx.ui, ctx.stats);
    let g = |a: &AtomicUsize| a.load(Relaxed);
    let bytes = s.bytes.load(Relaxed);
    ui.always(K::Def, "");
    ui.always(K::Top, "===[ DEBUG ]");
    ui.always(K::Inf, &format!("total/found/all...: {}", g(&s.total)));
    ui.always(K::Inf, &format!("accepted/valid....: {}", g(&s.accepted)));
    ui.always(K::Cur, &format!("count/processed...: {}", g(&s.processed)));
    ui.always(K::Pri, &format!("dled/downloaded...: {} ({} ({} bytes))", g(&s.dled), human(bytes as f64), bytes));
    ui.always(K::War, &format!("renamed...........: {}", g(&s.renamed)));
    ui.always(K::War, &format!("replaced..........: {}", g(&s.replaced)));
    ui.always(K::War, &format!("same hash (deleted): {}", g(&s.hash_dupes)));
    ui.always(K::War, &format!("skipped/found.....: {}", g(&s.skipped)));
    ui.always(K::Eri, &format!("disposed/rejected.: {}", g(&s.disposed)));
    ui.always(K::Eri, &format!("failed............: {}", g(&s.failed)));
}
