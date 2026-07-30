use ratatui::buffer::Buffer;
use ratatui::layout::{Position, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::Widget;

use super::links::Links;
use super::selection::viewport_top;
use super::smart::SpanSet;

/// Colour for detected URLs (bright blue).
const LINK_COLOR: Color = Color::Indexed(12);

pub struct TerminalView<'a> {
    screen: &'a vt100::Screen,
    selection: Option<&'a SpanSet>,
    links: Option<&'a Links>,
}

impl<'a> TerminalView<'a> {
    pub fn new(screen: &'a vt100::Screen) -> Self {
        Self { screen, selection: None, links: None }
    }

    /// Highlight exactly the cells a selection would copy.
    pub fn with_selection(mut self, selection: Option<&'a SpanSet>) -> Self {
        self.selection = selection;
        self
    }

    pub fn with_links(mut self, links: Option<&'a Links>) -> Self {
        self.links = links;
        self
    }
}

impl<'a> Widget for TerminalView<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let (vt_rows, vt_cols) = self.screen.size();
        let top = viewport_top(self.screen);

        for row in 0..area.height.min(vt_rows) {
            for col in 0..area.width.min(vt_cols) {
                if let Some(vt_cell) = self.screen.cell(row, col) {
                    if vt_cell.is_wide_continuation() {
                        continue;
                    }

                    let pos = Position {
                        x: area.x + col,
                        y: area.y + row,
                    };

                    if let Some(buf_cell) = buf.cell_mut(pos) {
                        let contents = vt_cell.contents();
                        buf_cell.set_symbol(if contents.is_empty() { " " } else { contents });

                        let stream_row = top + u64::from(row);
                        let is_link = self.links
                            .is_some_and(|links| links.contains(stream_row, col));

                        let fg = if is_link {
                            LINK_COLOR
                        } else {
                            convert_color(vt_cell.fgcolor())
                        };
                        let bg = convert_color(vt_cell.bgcolor());

                        let mut modifiers = Modifier::empty();
                        if vt_cell.bold() {
                            modifiers |= Modifier::BOLD;
                        }
                        if vt_cell.dim() {
                            modifiers |= Modifier::DIM;
                        }
                        if vt_cell.italic() {
                            modifiers |= Modifier::ITALIC;
                        }
                        if vt_cell.underline() || is_link {
                            modifiers |= Modifier::UNDERLINED;
                        }
                        if vt_cell.inverse() {
                            modifiers |= Modifier::REVERSED;
                        }

                        let selected = self.selection
                            .is_some_and(|sel| sel.contains(stream_row, col));
                        if selected {
                            modifiers |= Modifier::REVERSED;
                        }

                        buf_cell.set_style(
                            Style::default().fg(fg).bg(bg).add_modifier(modifiers),
                        );
                    }
                }
            }
        }

        // Render cursor only when visible and not scrolled back
        if !self.screen.hide_cursor() && self.screen.scrollback() == 0 {
            let (cur_row, cur_col) = self.screen.cursor_position();
            let cursor_pos = Position {
                x: area.x + cur_col,
                y: area.y + cur_row,
            };
            if let Some(cell) = buf.cell_mut(cursor_pos) {
                cell.set_style(cell.style().add_modifier(Modifier::REVERSED));
            }
        }
    }
}

fn convert_color(c: vt100::Color) -> Color {
    match c {
        vt100::Color::Default => Color::Reset,
        vt100::Color::Idx(i) => Color::Indexed(i),
        vt100::Color::Rgb(r, g, b) => Color::Rgb(r, g, b),
    }
}
