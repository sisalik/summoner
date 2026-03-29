use ratatui::buffer::Buffer;
use ratatui::layout::{Position, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::Widget;

pub struct TerminalView<'a> {
    screen: &'a vt100::Screen,
}

impl<'a> TerminalView<'a> {
    pub fn new(screen: &'a vt100::Screen) -> Self {
        Self { screen }
    }
}

impl<'a> Widget for TerminalView<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let (vt_rows, vt_cols) = self.screen.size();

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

                        let fg = convert_color(vt_cell.fgcolor());
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
                        if vt_cell.underline() {
                            modifiers |= Modifier::UNDERLINED;
                        }
                        if vt_cell.inverse() {
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
