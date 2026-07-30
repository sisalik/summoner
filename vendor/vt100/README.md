# vt100

**Vendored copy — upstream vt100 0.16.2, patched for summoner.**

Summoner needs selection coordinates that stay glued to content while output
streams past. Upstream exposes only `scrollback_offset`, which is relative to
the live screen top, so content shifts under a selection as lines scroll into
scrollback, and the offset alone cannot recover an absolute position once the
scrollback ring starts evicting rows.

Local patches (re-apply these when bumping the upstream version):

- `src/grid.rs` — `Grid::scrolled_lines: u64`, a monotonic count of rows ever
  pushed into scrollback, incremented in `scroll_up`; `Grid::scrolled_lines()`
  and `Grid::stream_row()` accessors.
- `src/screen.rs` — non-mutating stream-row readers: `scrolled_lines()`,
  `stream_row_wrapped()`, `stream_row_contents()`, `stream_cell()`. These let
  callers read any retained row without mutating the scrollback offset.

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
