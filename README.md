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
| `-D N` | `--crawl-depth N` | crawl depth (default 0 = only the given page, see below) |
| | `--crawl-sleep MS` | wait this long before every page request (avoid HTTP 429) |
| | `--crawl-retry-429` | on HTTP 429 retry the page with exponential backoff + random offset |
| | `--crawl-retries N` | max retries per page for `--crawl-retry-429` (default 6) |
| | `--external` | with `--crawl-depth`: also follow links to other hosts |
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

## Crawling (`--crawl-depth`)
Links on a page are split into **pages** and **files**:
- pages (crawled): `<a href>`, `<area href>`, `<iframe>`/`<frame>` links that look like web pages
  (no extension, `.html`, `.php`, ... , not media/archive/asset files)
- files (downloaded): every other link that passes your filters (`--media`, `--any`, `--must`, `--ignore` ...).
  Links inside `<img>`, `<video>`, `<audio>`, `<source>` (`src`, `srcset`, `poster`, `data-src` ...) count
  as media even without a file extension: `--image` takes `<img>` links, `--video` takes `<video>`/`<audio>`
  links, `--media` both. This works with `--crawl-depth 0` too. Files saved from such URLs get their
  extension from the response's Content-Type (`.jpg`, `.mp4`, ...).

`--crawl-depth 0` scans only the page you gave (as before), `--crawl-depth 1` also scans the pages it links to,
`--crawl-depth 2` the pages those link to, and so on. Each page is visited once, only on the start page's host
(`www.` ignored) unless `--external`; files are downloaded from any host. Files are downloaded as soon as a
page is scanned. `--ignore logout` also keeps the crawler away from matching pages; pages are only saved
with `--codec`. Use `--unique` so a file shown on many pages is fetched once. `--list --crawl-depth N` prints the
files found while crawling (progress goes to stderr); for a local .html file add `--base`.
**Rate limits:** `--crawl-sleep 1500` waits 1.5 s before every page request. `--crawl-retry-429` handles a
server that answers HTTP 429 anyway: the page is retried after 1 s, 2 s, 4 s ... (max 60 s; a longer
`Retry-After` from the server wins), each wait plus a random offset (up to 20% + 250 ms) so it isn't a fixed
rhythm. After `--crawl-retries` failed tries the page is counted as failed and the crawl moves on. These
apply to page requests only; file downloads are not throttled.
Resuming a crawl: with `--live-csv` every crawled page is logged once its files are done. On a re-run,
pages at the last depth that are already logged are skipped; logged pages *above* it are read again
only to find the pages below them (their files are skipped, nothing is logged twice), otherwise an
interrupted crawl could never continue. Pages that failed (e.g. 429) aren't logged and are retried.
Raising `--crawl-depth` between runs works the same way.

## Skip / resume
- `--unique` same URL once. `--nodupes` same content once. `--skip` same file *name* (only the name!
  Two different links with the same file name confuse `--skip` and `--replace`).
- `--live-csv data.csv` columns: `type,url,name,hash,is_dupe`. Loaded at start, only appended to, each row
  flushed to disk immediately. Fields with commas/quotes are quoted like Python's `csv`.
  - `type=media`: a downloaded file. `name` is relative to `--dir`, `hash` = sha256 of the file,
    `is_dupe=1` = deleted duplicate, blank `url` = came from `--hash-existing`.
  - `type=page`: a crawled page. `name` = the page's title, `hash` = sha256 of its HTML,
    `is_dupe=1` = same HTML as an earlier page (not scanned again).
  - An old 4-column file (`url,filename,hash,is_dupe`) is converted on first use (copy kept as
    `<file>.legacy.bak`).
- Re-running an interrupted `webgrab --live-csv data.csv --nodupes --unique links.html` skips every URL
  already in the CSV (downloaded or deleted as duplicate) and uses its hashes for `--nodupes`.
  Failed downloads aren't logged, so they are retried.

Exit code: 0 ok, 1 bad input / fatal, 2 some downloads failed.
