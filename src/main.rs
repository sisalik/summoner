use anyhow::Result;

fn main() -> Result<()> {
    let mut terminal = ratatui::init();
    let result = summoner::app::run(&mut terminal);
    ratatui::restore();
    result
}
