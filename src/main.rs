use std::path::PathBuf;

use anyhow::Result;

#[cfg(feature = "dev-creature")]
mod render_creature {
    use anyhow::Result;
    use std::fs::File;
    use std::io::BufWriter;

    use summoner::creature::generate::CellKind;
    use summoner::creature::locomotion::LocomotionState;
    use summoner::creature::outline::{rasterize_skeleton_scaled, rasterize_skeleton_wireframe};
    use summoner::creature::render::{state_palette, capsule_shade_offset, Palette};
    use summoner::creature::skeleton::{Skeleton, archetype_index};
    use summoner::session::SessionState;

    fn parse_state(s: &str) -> SessionState {
        match s {
            "working" => SessionState::Working,
            "waiting" => SessionState::Waiting,
            "idle" => SessionState::Idle,
            "sleeping" => SessionState::Sleeping,
            "disconnected" => SessionState::Disconnected,
            _ => SessionState::Idle,
        }
    }

    fn shade_color_rgb(r: u8, g: u8, b: u8, offset: i16) -> (u8, u8, u8) {
        (
            (r as i16 + offset).clamp(0, 255) as u8,
            (g as i16 + offset).clamp(0, 255) as u8,
            (b as i16 + offset).clamp(0, 255) as u8,
        )
    }

    fn component_color_rgb(id: u8) -> (u8, u8, u8) {
        const COLORS: &[(u8, u8, u8)] = &[
            (255, 100, 100), (100, 255, 100), (100, 100, 255),
            (255, 255, 100), (255, 100, 255), (100, 255, 255),
            (255, 180, 100), (180, 100, 255), (100, 200, 150),
            (200, 200, 100),
        ];
        const LIMB_COLORS: &[(u8, u8, u8)] = &[
            (255, 80, 80), (80, 200, 255), (255, 200, 80), (80, 255, 160),
            (200, 80, 255), (255, 160, 80), (80, 255, 80), (80, 160, 255),
        ];
        if id == 0 { (20, 20, 30) }
        else if id >= 200 { (180, 140, 255) }
        else if id >= 100 { LIMB_COLORS[(id - 100) as usize % LIMB_COLORS.len()] }
        else { COLORS[(id as usize).saturating_sub(1) % COLORS.len()] }
    }

    fn palette_to_rgb(palette: &Palette) -> ((u8, u8, u8), (u8, u8, u8)) {
        let body = match palette.body {
            ratatui::style::Color::Rgb(r, g, b) => (r, g, b),
            _ => (100, 100, 100),
        };
        let border = match palette.border {
            ratatui::style::Color::Rgb(r, g, b) => (r, g, b),
            _ => (60, 60, 60),
        };
        (body, border)
    }

    pub fn run(args: &[String]) -> Result<()> {
        let get_arg = |flag: &str| -> Option<String> {
            args.iter().position(|a| a == flag).and_then(|i| args.get(i + 1).cloned())
        };

        let archetype_name = get_arg("--archetype").unwrap_or_else(|| "bipedal".into());
        let archetype = archetype_index(&archetype_name);
        let state_name = get_arg("--state").unwrap_or_else(|| "rest".into());
        let seed: u64 = get_arg("--seed").and_then(|s| s.parse().ok()).unwrap_or(42);
        let phase: f32 = get_arg("--phase").and_then(|s| s.parse().ok()).unwrap_or(0.0);
        let color_mode = get_arg("--color").unwrap_or_else(|| "shaded".into());
        let scale: usize = get_arg("--scale").and_then(|s| s.parse().ok()).unwrap_or(16);
        let output = get_arg("-o").unwrap_or_else(|| "creature.png".into());

        let mut skel = Skeleton::instantiate(archetype, seed);

        if phase > 0.0 && state_name != "rest" {
            let state = parse_state(&state_name);
            let mut loco = LocomotionState::new(skel, state);
            let ticks = (phase * 60.0).round() as u32;
            for _ in 0..ticks {
                loco.tick(std::time::Duration::from_millis(33));
            }
            skel = loco.skeleton().clone();
        }

        let raster = if color_mode == "wireframe" {
            rasterize_skeleton_wireframe(&skel, scale)
        } else {
            rasterize_skeleton_scaled(&skel, scale)
        };

        let img_w = raster.sprite.width;
        let img_h = raster.sprite.height;
        let mut pixels = vec![0u8; img_w * img_h * 3];
        let bg = (20u8, 20u8, 30u8);

        let palette = if state_name == "rest" {
            state_palette(SessionState::Idle)
        } else {
            state_palette(parse_state(&state_name))
        };
        let (body_rgb, border_rgb) = palette_to_rgb(&palette);

        for y in 0..img_h {
            for x in 0..img_w {
                let idx = y * img_w + x;
                let cell = raster.sprite.get(x, y);
                let cap_id = raster.capsule_ids[idx];

                let (r, g, b) = match color_mode.as_str() {
                    "shaded" => match cell {
                        CellKind::Body => shade_color_rgb(body_rgb.0, body_rgb.1, body_rgb.2, capsule_shade_offset(cap_id)),
                        CellKind::Border => border_rgb,
                        CellKind::Empty => bg,
                    },
                    "limb" | "wireframe" => match cell {
                        CellKind::Body | CellKind::Border => component_color_rgb(cap_id),
                        CellKind::Empty => bg,
                    },
                    _ => bg,
                };

                let pi = idx * 3;
                pixels[pi] = r;
                pixels[pi + 1] = g;
                pixels[pi + 2] = b;
            }
        }

        let file = File::create(&output)?;
        let w = BufWriter::new(file);
        let mut encoder = png::Encoder::new(w, img_w as u32, img_h as u32);
        encoder.set_color(png::ColorType::Rgb);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header()?;
        writer.write_image_data(&pixels)?;

        eprintln!("Wrote {}x{} PNG to {}", img_w, img_h, output);
        Ok(())
    }
}

fn summoner_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".summoner")
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();

    #[cfg(feature = "dev-creature")]
    {
        if args.iter().any(|a| a == "--render-creature") {
            return render_creature::run(&args);
        }

        if args.iter().any(|a| a == "--test-creatures") {
            let mut terminal = ratatui::init();
            let result = summoner::test_creatures::run(&mut terminal);
            ratatui::restore();
            return result;
        }
    }

    match args.get(1).map(|s| s.as_str()) {
        Some("install") => {
            let dir = summoner_dir();
            std::fs::create_dir_all(&dir)?;
            summoner::hooks::install_hooks(&dir);
            eprintln!("Summoner hooks installed.");
            eprintln!("  Hook script:     ~/.summoner/hooks/claude-state.sh");
            eprintln!("  StatusLine wrap:  ~/.summoner/hooks/statusline-wrapper.sh");
            eprintln!("  Claude settings:  ~/.claude/settings.json (updated)");
            Ok(())
        }
        Some("uninstall") => {
            let dir = summoner_dir();
            summoner::hooks::uninstall_hooks(&dir);
            eprintln!("Summoner hooks uninstalled.");
            eprintln!("  Removed hook entries from ~/.claude/settings.json");
            eprintln!("  Removed scripts from ~/.summoner/hooks/");
            eprintln!("  Cleaned up state files");
            Ok(())
        }
        _ => {
            let mut terminal = ratatui::init();
            crossterm::execute!(
                std::io::stdout(),
                crossterm::event::EnableMouseCapture,
                crossterm::event::EnableBracketedPaste,
            )?;
            let result = summoner::app::run(&mut terminal);
            let _ = crossterm::execute!(
                std::io::stdout(),
                crossterm::event::DisableMouseCapture,
                crossterm::event::DisableBracketedPaste,
            );
            ratatui::restore();
            result
        }
    }
}
