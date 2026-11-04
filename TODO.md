# Task: Refactor and Update CLI Options, Skip Logic, and State Management for `webgrab`

Please update the `webgrab` CLI tool according to the following comprehensive refactoring specification.

## 1. General CLI & Flag Changes
- **Short Flags:** Add standard short flags for major options (e.g., `-d` for `--dir`, `-t` for `--threads`, `-r` for `--read`, `-v` for `--verbose`, etc.).
- **Aliases Removal:** Remove all aliases for the **Modes** options (`--list`, `--test`, `--read`, `--watch`).
- **Underscores & Hyphens:** Ensure flexible flag parsing supports both hyphens and underscores (e.g., `--skip_all` or `--skip-all`), as well as single or double dashes (`-threads 4` or `--threads 4`).
- **Rename Flag:** Rename the `--only` filter flag to `--any`.

## 2. User-Agent Spoofing Options
Add/update User-Agent preset flags:
- `--agent`: Send `"User-Agent: webgrab"`
- `--agent-firefox`: Use a standard modern Firefox User-Agent
- `--agent-chromium`: Use a standard Chromium User-Agent
- `--agent-chrome`: Use a standard Google Chrome User-Agent
- `--agent-brave`: Use a standard Brave User-Agent (and make sure it handles Chrome/Brave client hints correctly if applicable)
- Keep `--user-agent <STRING>` for custom strings.

## 3. Recursive Options & `--skip-find` Replacement
- **Remove:** Drop `--skip-find`.
- **Add:** Introduce `-R`, `--recursive` option.
- **Behavior of `-R` / `--recursive`:**
  - When used with `--skip`, it searches download directories recursively for matching file names.
  - When used with `--hash-existing`, it recursively hashes all files found within subdirectories of `--dir`.

## 4. Overhaul: State Management & CSV Workspace (`--live-csv`)
Remove `--export-hashes` and `--export-append`. Replace them entirely with a single state-tracking workspace flag:
- **`--live-csv <file.csv>`**:
  - If `<file.csv>` already exists, read it upon startup and load its entries into memory (acting as our workspace/historical record). Never overwrite or destroy existing records; always append/update.
  - **CSV Structure (3 columns):** `url,filename,hash,is_dupe`
    - `url`: The source URL (blank if generated purely from `--hash-existing`).
    - `filename`: Relative path to `--dir` (e.g., `file.ext` or `subdir/file.ext`).
    - `hash`: SHA-256 hash of the file.
    - `is_dupe`: `0` if successfully downloaded (unique content), `1` if flagged as a duplicate and deleted.
  - **Robust CSV Sanitization:** Ensure any commas in filenames or values are properly escaped/quoted (like Python's `csv` module does) to prevent breaking the CSV parser.
  - **`--hash-existing` Integration:** When `--hash-existing` is passed with `--live-csv`, it should scan (recursively if `-R` is enabled) the target directory, hash existing files, and write them to the CSV with a blank `url` field and `is_dupe=0`.

## 5. Simplified Skip & Resume Logic
- **Remove Old Skip Modifiers:** Completely remove `--skip-file`, `--skip-ext`, `--skip-case`, `--skip-output`, and `--skip-all`.
- **Core Resumption Flow:**
  - `--unique` keeps URLs unique in the queue.
  - `--nodupes` keeps file content unique via hashing.
  - `--skip` strictly checks if the exact file name already exists.
  - *Note for `--help`: Explicitly document in `--help` that `--skip` only checks matching file names. If two different links point to different files that happen to share the same name, both `--skip` and `--replace` can be problematic.*
  - **Resumption Use Case:** If a command like `webgrab --live-csv data.csv --nodupes --unique download_links.html` gets interrupted, rerunning the exact same command should seamlessly continue by skipping URLs/files already logged in `data.csv`. Once fully downloaded, subsequent reruns will skip everything cleanly.
