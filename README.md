# ers

`ers` is a terminal ebook reader written in Rust. It supports EPUB, FB2, MOBI, AZW, AZW3, and HTTP/HTTPS pages, with reading progress, search, bookmarks, links, images, and annotations for EPUB books and cached web articles.

**Platform:** `ers` has been tested on macOS only. Other platforms are untested.

`ers` is a Rust port of [epy](https://github.com/wustho/epy), which was originally based on [epr](https://github.com/wustho/epr).

## Install

Install Rust 1.100 or newer and the macOS command-line developer tools, then run from this checkout:

```sh
cargo install --locked --path .
```

This installs `ers` in Cargo's binary directory, normally `~/.cargo/bin`. Python is not required to build or run the reader.

To build without installing:

```sh
cargo build --locked --release
./target/release/ers /path/to/book.epub
```

## Open a book

```sh
ers /path/to/book.epub
ers https://example.com/story.html
ers "author or title"
ers --history
ers --dump /path/to/book.epub
```

Run `ers` without arguments to resume the last book, or provide a history number or title/author search. Use `ers --help` for CLI options.

## Web articles and offline reading

Open an HTTP/HTTPS URL to save a local snapshot of its text and supported images. Reopening that URL, including from history, uses the saved snapshot without network requests. Website edits do not change saved articles or move their annotations.

Snapshots live in `~/.config/ers/articles/`, beside `states.db` (or under your configured `XDG_CONFIG_HOME`). Back up both to preserve offline articles and their annotations. Removing an entry from history does not delete its snapshot or annotations. Snapshots do not expire automatically; this release has no refresh command.

The first download attempts to save up to 128 distinct images, within a 128 MiB text-and-image budget and a 30-second image-download budget. Individual resources are limited to 32 MiB. Missing, unsupported, or oversized images do not prevent reading; their previews report that they were not cached. Images loaded only by JavaScript, CSS backgrounds, and `srcset`-only images are not captured. Links to other pages still open in your browser.

URLs with different fragments (`#section`) share a snapshot. Query strings remain significant, and redirect aliases are stored separately. `--dump` remains a fresh, text-only fetch and does not create application storage. Corrupt snapshots produce an error rather than silently downloading changed text.

## Reading controls

| Keys | Action |
| --- | --- |
| `j` / `k`, Up / Down | Scroll down / up. |
| `l` / `h`, Space, Right / Left | Next / previous page. |
| Ctrl-D / Ctrl-U | Move down / up half a page. |
| `L` / `H` | Next / previous chapter. |
| `g` / `G` | Beginning / end of chapter. |
| `t` or Tab | Table of contents. |
| `/` | Search a regular expression. Use `n` / `N` for next / previous match. |
| `b` / `B` | Add a bookmark / open bookmarks. |
| `m` then a digit / backtick then a digit | Save / jump to a session quickmark. |
| Ctrl-O / Ctrl-I | Backward / forward through jump history. |
| `R` | Open the library. |
| `D` | Toggle double spread. |
| `-` / `+` | Shrink / enlarge text in single-column mode. |
| `d` | Look up a word. |
| `c` / `p` | Change color profile / toggle progress. |
| `?` / `q` | Help / quit. |

Numeric prefixes repeat supported commands. Terminals must distinguish Ctrl-I from Tab for forward jump history. In tmux, enable `extended-keys`; `ers` accepts CSI-u and xterm extended-key formats. The outer terminal must support extended keys as well.

The reader requires a terminal of at least 22 columns by 12 rows. If the terminal becomes smaller, resize it to continue viewing the book.

During long operations, input buffers have limits. If a buffer fills, `ers` reports ignored input; retry after the operation finishes.

## Highlights and notes

Annotations are available for local EPUB books and cached web articles. They do not modify the ebook or saved article.

1. Press `v` to enter selection mode, then move with `h/j/k/l` or the arrow keys.
2. Press Space to start a selection and move to its inclusive end. Press Enter to save a highlight, or `n` to add a note.
3. Notes open in **INSERT** mode, where Enter inserts a newline. Esc switches to **NORMAL** mode without closing the draft. Press Tab to select **Save**, then Enter to commit, or select **Cancel** to discard. Press `v` or Esc to leave selection mode.

In note NORMAL mode, use `h/j/k/l` to move, `w/b/e` for words, and `0/$` for line boundaries. Use `i/a` to insert before/after the cursor, `I/A` at the first nonblank character/line end, and `o/O` to open a line below/above. `x` deletes a character, `dd` deletes a line, `u` undoes, and Ctrl-R redoes. Each INSERT session forms one undo step. Motions follow logical lines, including when text wraps on screen. Counts, text objects, registers, and repeat commands are not supported.

Press `a` to open the annotation list. Select a highlight and press Enter to jump, `e` to edit its note, `f` to read it, or `d` to delete it after confirmation. Press `r` to copy annotations from another identity after validating their exact positions.

Press `e` to link a Markdown file for one-way export. Confirm the destination before writing. Existing handwritten text outside the managed section is preserved. If export fails or encounters a conflict, the annotation remains saved locally; `e` provides retry and recovery options.

## Links and images

Links are underlined, and the status line shows the number of visible links and the configured follow key. Press `f` to show numbers on visible links, then type a number and press Enter. Internal links open in the book; HTTP/HTTPS links open in your browser. Other external schemes are rejected.

Inline images are enabled by default. Press `o` to select an image, Enter to preview it, and `e` in the preview to open it externally. Kitty-compatible terminals can use the Kitty graphics protocol directly. Local tmux sessions with one Ghostty or Kitty client and `allow-passthrough` enabled also use native graphics, with private temporary image files. Other sessions use a text-cell fallback. No `kitten` executable is required.

## Configuration and data

By default, `ers` stores `configuration.json` and `states.db` under `~/.config/ers/`. An absolute `XDG_CONFIG_HOME` changes the parent directory. The configuration file is optional; missing settings use defaults.

For example, create `configuration.json` with:

```json
{
  "settings": {
    "mouse_support": true,
    "between_chapters": false,
    "inline_images": true,
    "dictionary_client": "auto"
  },
  "bindings": {
    "jump_forward": "F"
  }
}
```

Mouse support is disabled by default. When enabled, a left click turns a page, a right click opens the table of contents, the wheel scrolls, and Ctrl-wheel changes text width.

For dictionary lookup, install `wkdict`, `sdcv`, or `dict`, or set `dictionary_client` to a command and its arguments. With `auto`, the reader checks those clients in that order.

The reader uses its own data directory and does not automatically import epy data. Back up `states.db` and linked Markdown files while the reader is closed.

## Credits

Credit to [wustho](https://github.com/wustho) and the contributors to [epy](https://github.com/wustho/epy) for the original Python ebook reader. `ers` ports that reader to Rust and builds on this fork's annotation, image, and link features.

`epy` was originally based on [epr](https://github.com/wustho/epr).

## License

This project is licensed under [GPL-3.0-only](LICENSE). Kindle decoding includes adaptations from [boko](https://github.com/zacharydenton/boko); attribution and license details are in `src/book/kindle/BOKO-NOTICE` and `src/book/kindle/BOKO-LICENSE`.
