# webgrab (Rust)

Web page file scanner / downloader.

Build: `cargo build --release` -> `target/release/webgrab` (`webgrab.exe` on Windows)

Flags work with two dashes or one (`--threads 4` = `-threads 4`), hyphens or underscores (`--live-csv` = `--live_csv`).
Short flags (`-t 4`, `-d out`, `-R`) take their value after a space and can't be combined.

```
webgrab <URL|FILE> [flags]
webgrab -r urls.txt [flags]
webgrab -w [flags]
```

## Flags
| short | long | |
|---|---|---|
| `-l` | `--list` | print links, don't download |
| `-T` | `--test` | print counts only |
| `-r FILE` | `--read FILE` | one URL per line |
| `-w` | `--watch` | download URLs copied to the clipboard |
| `-t N` | `--threads N` | parallel downloads (default 1) |
| `-d DIR` | `--dir DIR` | download folder |
| `-R` | `--recursive` | `--skip` and `--hash-existing` also look in subfolders |
| `-D N` | `--depth N` | crawl depth (default 0 = only the given page, see below) |
| | `--external` | with `--depth`: also follow links to other hosts |
| `-u` | `--unique` | queue every URL once |
| `-n` | `--nodupes` | hash each download; same content already saved -> delete the new file |
| `-H` | `--hash-existing` | hash files already in `--dir` first |
| `-L FILE` | `--live-csv FILE` | workspace/state file (below) |
| `-s` | `--skip` | skip if a file with the same **name** exists |
| `-p` | `--replace` | overwrite same-name files (default: `name [1].ext`) |
| `-b URL` | `--base URL` | resolve relative links in a local .html |
| | `--valid` | http(s) only |
| | `--timeout SECS` | |
| `-a` | `--any a,b` | link must contain ANY of these |
| `-m` | `--must a,b` | link must contain ALL of these |
| `-i` | `--ignore a,b` | link must contain NONE of these |
| `-M` | `--media` | + `--video --image --other --codec --grab` |
| `-q` `-c` `-v` `-f` | `--clean --count --verbose --filename` | + `--compact --color` |

User-Agent: `--agent` (`webgrab`), `--agent-firefox`, `--agent-chromium`, `--agent-chrome`, `--agent-brave`
(the Chromium-family ones also send matching `Sec-CH-UA*` client hints), `--user-agent STRING` (wins).
Browser versions are constants at the top of `src/ua.rs`.

## Crawling (`--depth`)
Links on a page are split into **pages** and **files**:
- pages (crawled): `<a href>`, `<area href>`, `<iframe>`/`<frame>` links that look like web pages
  (no extension, `.html`, `.php`, ... , not media/archive/asset files)
- files (downloaded): every other link that passes your filters (`--media`, `--any`, `--must`, `--ignore` ...)

`--depth 0` scans only the page you gave (as before), `--depth 1` also scans the pages it links to,
`--depth 2` the pages those link to, and so on. Each page is visited once, only on the start page's host
(`www.` ignored) unless `--external`; files are downloaded from any host. Files are downloaded as soon as a
page is scanned. `--ignore logout` also keeps the crawler away from matching pages; pages are only saved
with `--codec`. Use `--unique` so a file shown on many pages is fetched once. `--list --depth N` prints the
files found while crawling (progress goes to stderr); for a local .html file add `--base`.
With `--live-csv`, a re-run re-reads the pages but skips every file already in the CSV.

## Skip / resume
- `--unique` same URL once. `--nodupes` same content once. `--skip` same file *name* (only the name!
  Two different links with the same file name confuse `--skip` and `--replace`).
- `--live-csv data.csv` columns: `url,filename,hash,is_dupe`. Loaded at start, only appended to, each row
  flushed to disk immediately. `filename` is relative to `--dir`; `is_dupe=1` = deleted duplicate;
  blank `url` = came from `--hash-existing`. Fields with commas/quotes are quoted like Python's `csv`.
- Re-running an interrupted `webgrab --live-csv data.csv --nodupes --unique links.html` skips every URL
  already in the CSV (downloaded or deleted as duplicate) and uses its hashes for `--nodupes`.
  Failed downloads aren't logged, so they are retried.

Exit code: 0 ok, 1 bad input / fatal, 2 some downloads failed.
