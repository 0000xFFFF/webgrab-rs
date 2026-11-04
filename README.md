# webgrab (Rust)

Web page file scanner / downloader. Rust rewrite of the C# webgrab.

Build: `cargo build --release` -> `target/release/webgrab` (`webgrab.exe` on Windows)

Flags work with two dashes or one: `--threads 4` = `-threads 4`. Underscores work too (`--skip_all`).
Options are separate arguments now, not one comma list (`--image --unique --must maxres`).

```
webgrab <URL|FILE> [flags]
webgrab --read urls.txt [flags]
webgrab --watch [flags]
```

## New in this version
| flag | what it does |
|---|---|
| `--list` | print the links found, don't download (works on URLs and local .html files) |
| `--unique` | de-duplicate the URL list (this is what the old `@nodupes` did) |
| `--nodupes` | hash every file after download; if the content was already downloaded, delete the new file |
| `--hash-existing` | also hash files already in `--dir` first (implies `--nodupes`) |
| `--threads N` / `-t N` | parallel downloads (default 1) |
| `--export-hashes FILE` | live `sha256sum`-style list (`<hash>  <name>`), flushed after every file |
| `--export-append` | append to that file instead of overwriting |
| `--dir DIR` | download folder |
| `--base URL` | resolve relative links when scanning a local .html file |
| `--only / --must / --ignore a,b` | keyword filters (replace `~kw`, `kw`, `-kw`) |
| `--user-agent STR`, `--timeout SECS` | request options |

## Kept from the original
`--test`, `--read`, `--watch`, `--valid`, `--replace`, `--skip`, `--skip-find`, `--skip-file`,
`--skip-ext`, `--skip-case`, `--skip-output`, `--skip-all`, `--agent`, `--compact`, `--clean`,
`--count`, `--verbose`, `--filename`, `--color`, `--media`, `--video`, `--image`, `--other`,
`--codec`, `--grab`. Old names still work as aliases (`--out/--print/--echo` = `--list`, `--ping/--try` = `--test`,
`--txt/--text` = `--read`, `--listen` = `--watch`). `-h`, `--help` and `/?` show help.

Exit code: 0 ok, 1 bad input / fatal, 2 some downloads failed.
