use anyhow::Result;

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();

    if args.iter().any(|a| a == "--test-creatures") {
        let mut terminal = ratatui::init();
        let result = summoner::test_creatures::run(&mut terminal);
        ratatui::restore();
        return result;
    }

    let mut terminal = ratatui::init();
    let result = summoner::app::run(&mut terminal);
    ratatui::restore();
    result
}
