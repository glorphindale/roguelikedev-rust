mod game;

#[macro_use]
extern crate serde_derive;

use tcod::console::*;

fn main() {
    let args: Vec<String> = std::env::args().collect();

    let args_len = args.len();
    let (font_name, font_layout) = match args_len {
        2 => (args[1].to_string(), FontLayout::AsciiInRow),
        _ => ("courier12x12_aa_tc.png".to_string(), FontLayout::Tcod)
    };
    
    game::run_game(&font_name, font_layout);
}
