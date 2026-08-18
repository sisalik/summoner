# vt100

**Vendored copy — upstream vt100 0.16.2, patched for summoner.**

Two things are missing upstream.

*Stream rows.* Summoner needs selection coordinates that stay glued to content
while output streams past. Upstream exposes only `scrollback_offset`, which is
relative to the live screen top, so content shifts under a selection as lines
scroll into scrollback, and the offset alone cannot recover an absolute
position once the scrollback ring starts evicting rows.

*OSC 8 hyperlinks.* Upstream drops them, so a label whose URL never appears on
screen is unrecoverable. The target has to live on the cell: the label can be
scrolled, overwritten and repainted freely, so any out-of-band record of screen
regions goes stale immediately.

Local patches (re-apply these when bumping the upstream version):

- `src/grid.rs` — `Grid::scrolled_lines: u64`, a monotonic count of rows ever
  pushed into scrollback, incremented in `scroll_up`; `Grid::scrolled_lines()`
  and `Grid::stream_row()` accessors. Also `Grid::clear_links()`.
- `src/screen.rs` — non-mutating stream-row readers: `scrolled_lines()`,
  `stream_row_wrapped()`, `stream_row_contents()`, `stream_cell()`. These let
  callers read any retained row without mutating the scrollback offset. Also
  the `link` pen field and `links` table, `set_hyperlink()` and `hyperlink()`.
- `src/row.rs` — `cells()` made public; `clear_links()`.
- `src/cell.rs` — `Cell::link: u16` (0 = none) with `Cell::link_id()`.
  `CONTENT_BYTES` drops 22 -> 20 to pay for it, so `Cell` stays exactly 32
  bytes; `set()` takes the link id, `clear()` resets it.
- `src/links.rs` — new: the interning table behind those ids. Ids are 1-based
  indices, so equal targets share an id and the OSC 8 `id=` param is
  redundant. URIs are validated here, at the boundary, because they come from
  arbitrary program output and end up in a URL the user can click.
- `src/perform.rs` — `osc_dispatch` arms for OSC 8.

Known gap: `Attrs::write_escape_code_diff` does not re-emit OSC 8, so
`contents_formatted()` and `contents_diff()` lose hyperlinks. Summoner renders
cells directly and never calls those.

Everything below is upstream documentation.

This crate parses a terminal byte stream and provides an in-memory
representation of the rendered contents.

## Overview

This is essentially the terminal parser component of a graphical terminal
emulator pulled out into a separate crate. Although you can use this crate
to build a graphical terminal emulator, it also contains functionality
necessary for implementing terminal applications that want to run other
terminal applications - programs like `screen` or `tmux` for example.

## Synopsis

```rust
let mut parser = vt100::Parser::new(24, 80, 0);

let screen = parser.screen().clone();
parser.process(b"this text is \x1b[31mRED\x1b[m");
assert_eq!(
    parser.screen().cell(0, 13).unwrap().fgcolor(),
    vt100::Color::Idx(1),
);

let screen = parser.screen().clone();
parser.process(b"\x1b[3D\x1b[32mGREEN");
assert_eq!(
    parser.screen().contents_formatted(),
    &b"\x1b[?25h\x1b[m\x1b[H\x1b[Jthis text is \x1b[32mGREEN"[..],
);
assert_eq!(
    parser.screen().contents_diff(&screen),
    &b"\x1b[1;14H\x1b[32mGREEN"[..],
);
```
