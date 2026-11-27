//! The "Packs from Rust" guide's code is compiled as `examples/guide.rs`.
//! This keeps the page and the compiled copy the same.

use std::path::Path;

#[test]
fn every_guide_example_is_compiled() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let page = std::fs::read_to_string(
        root.join("../../website/src/content/docs/guides/packs-from-rust.md"),
    )
    .unwrap();
    let example = std::fs::read_to_string(root.join("examples/guide.rs")).unwrap();
    let mut blocks = 0;
    for block in page.split("```rust\n").skip(1) {
        let block = block.split("```").next().unwrap();
        let body = block
            .lines()
            .filter(|line| !line.starts_with("use "))
            .collect::<Vec<_>>()
            .join("\n");
        assert!(
            example.contains(body.trim()),
            "examples/guide.rs is missing this block from the guide:\n{body}"
        );
        blocks += 1;
    }
    assert!(blocks >= 5, "found {blocks} Rust blocks");
}
