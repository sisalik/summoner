//! One end-to-end fixture: a real Claude Code reply, rendered through vt100
//! with the escapes it actually emits, copied as markdown.

use summoner::ui::selection::{Selection, SelectionMode};
use summoner::ui::smart;

const CODE: &str = "\x1b[38;5;153m";
const OFF: &str = "\x1b[0m";

const EXPECTED: &str = "\
And custom link schemes survive as ordinary markdown links with the URL intact (`confluence-user:xyz` → `link(url='confluence-user:xyz')`)), so mentions and page links need no custom lexer rule at all.

---

## markfluence fix list

P0 — makes the round-trip safe *by construction* (this is the actual blocker)

| # | Item |
|:---:|---|
| MF-1 | Add a `confluence` fenced-block passthrough, both directions. In `md_to_storage.py`, override `block_code`: when the info string is `confluence`, emit the content verbatim (no CDATA, no escaping). In `converter.py`, emit that fence for any construct with no lossless markdown form. |
| MF-2 | Flip the unknown-macro fallback from drop to preserve (`macros.py:54`). Emit the macro's original XML in a `confluence` fence instead of `return body_text`. Keep the warning. This is the safety net that makes future macros safe forever without new handlers. |";

#[test]
fn a_claude_code_reply_copies_as_markdown() {
    let w: u16 = 105;
    let mut p = vt100::Parser::new(40, w, 200);
    let lines = vec![
        format!("  And custom link schemes survive as ordinary markdown links with the URL intact ({CODE}confluence-user:xyz{OFF}"),
        format!("  → {CODE}link(url='confluence-user:xyz'){OFF})), so mentions and page links need no custom lexer rule at all."),
        String::new(),
        "  ---".into(),
        String::new(),
        "  \x1b[3;4mmarkfluence fix list\x1b[0m".into(),
        String::new(),
        "  P0 — makes the round-trip safe \x1b[3mby construction\x1b[0m (this is the actual blocker)".into(),
        String::new(),
        "  ┌──────┬─────────────────────────────────────────────────────────────────────────────────────────┐".into(),
        "  │  #   │                                          Item                                           │".into(),
        "  ├──────┼─────────────────────────────────────────────────────────────────────────────────────────┤".into(),
        format!("  │      │ Add a {CODE}confluence{OFF} fenced-block passthrough, both directions. In {CODE}md_to_storage.py{OFF},        │"),
        format!("  │ MF-1 │ override {CODE}block_code{OFF}: when the info string is {CODE}confluence{OFF}, emit the content verbatim (no  │"),
        format!("  │      │ CDATA, no escaping). In {CODE}converter.py{OFF}, emit that fence for any construct with no         │"),
        "  │      │ lossless markdown form.                                                                 │".into(),
        "  ├──────┼─────────────────────────────────────────────────────────────────────────────────────────┤".into(),
        format!("  │      │ Flip the unknown-macro fallback from drop to preserve ({CODE}macros.py:54{OFF}). Emit the macro's  │"),
        format!("  │ MF-2 │ original XML in a {CODE}confluence{OFF} fence instead of {CODE}return body_text{OFF}. Keep the warning. This  │"),
        "  │      │ is the safety net that makes future macros safe forever without new handlers.           │".into(),
        "  └──────┴─────────────────────────────────────────────────────────────────────────────────────────┘".into(),
    ];
    for line in &lines {
        p.process(line.as_bytes());
        p.process(b"\r\n");
    }
    let sel = Selection {
        anchor: (0, 0),
        moving: (lines.len() as u64 - 1, w - 1),
        dragged: true,
        mode: SelectionMode::Smart,
    };
    assert_eq!(smart::compute_spans(p.screen(), &sel).text(), EXPECTED);
}

const GREEN: &str = "\x1b[38;2;13;188;121m";
const BLUE: &str = "\x1b[38;2;36;114;200m";
const YELLOW: &str = "\x1b[38;2;229;229;16m";
const CYAN: &str = "\x1b[38;2;78;201;176m";

/// The colours are the ones measured off a live Claude Code code block.
#[test]
fn a_syntax_highlighted_code_block_fences_and_does_not_reflow() {
    let w: u16 = 82;
    let mut p = vt100::Parser::new(30, w, 200);
    let lines = vec![
        "  Here is a snippet from the detector:".to_string(),
        String::new(),
        format!("  {GREEN}/// Runs of consecutive code lines.{OFF}"),
        format!("  {BLUE}fn{OFF} {CYAN}detect_blocks{OFF}(lines: &[{YELLOW}Line{OFF}]) -> {YELLOW}Vec{OFF}<{YELLOW}Range{OFF}<{YELLOW}usize{OFF}>> {{"),
        format!("      {BLUE}let{OFF} {BLUE}mut{OFF} blocks = {YELLOW}Vec{OFF}::{CYAN}new{OFF}();"),
        format!("      {BLUE}let{OFF} code = i < lines.{CYAN}len{OFF}() && {CYAN}is_code_line{OFF}(&lines[i], guards);"),
        format!("      {BLUE}match{OFF} (code, start) {{"),
        format!("          ({YELLOW}true{OFF}, {YELLOW}None{OFF}) => start = {YELLOW}Some{OFF}(i),"),
        "      }".to_string(),
        "  }".to_string(),
        String::new(),
        "  That is the whole thing.".to_string(),
    ];
    for line in &lines {
        p.process(line.as_bytes());
        p.process(b"\r\n");
    }
    let sel = Selection {
        anchor: (0, 0),
        moving: (lines.len() as u64 - 1, w - 1),
        dragged: true,
        mode: SelectionMode::Smart,
    };
    let text = smart::compute_spans(p.screen(), &sel).text().to_string();
    assert!(text.contains("```"), "{text}");
    // The widest line in the selection is `let code = ...`; outside a fence the
    // wrap estimate would make the `match` below it look like a continuation.
    assert!(
        text.contains("guards);\n    match (code, start) {"),
        "{text}"
    );
    assert!(text.starts_with("Here is a snippet from the detector:\n\n```\n"), "{text}");
    assert!(text.ends_with("```\n\nThat is the whole thing."), "{text}");
}
