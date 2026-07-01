use std::{error::Error, fs, path::Path};

const LOGO: &str = r#"
▗▖    ▗▄▖   ▄  ▗▄▄   ▗▄▖ ▗▖ ▗▖▗▄▄▄▖
▐▌    █▀█  ▐█▌ ▐▛▀█  █▀█ ▐▌ ▐▌▝▀█▀▘
▐▌   ▐▌ ▐▌ ▐█▌ ▐▌ ▐▌▐▌ ▐▌▐▌ ▐▌  █
▐▌   ▐▌ ▐▌ █ █ ▐▌ ▐▌▐▌ ▐▌▐▌ ▐▌  █
▐▌   ▐▌ ▐▌ ███ ▐▌ ▐▌▐▌ ▐▌▐▌ ▐▌  █
▐▙▄▄▖ █▄█ ▗█ █▖▐▙▄█  █▄█ ▝█▄█▘  █
▝▀▀▀▘ ▝▀▘ ▝▘ ▝▘▝▀▀   ▝▀▘  ▝▀▘   ▀
"#;

fn main() -> Result<(), Box<dyn Error>> {
    let source_path = Path::new("assets/loadout.png");
    let output_path = Path::new("assets/loadout.txt");

    println!("cargo:rerun-if-changed={}", source_path.display());
    println!("cargo:rerun-if-changed=build.rs");

    fs::write(output_path, LOGO.trim())?;

    Ok(())
}
