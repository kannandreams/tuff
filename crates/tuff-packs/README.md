# tuff-packs

Tuff capability packs from Rust. Build a source pack for its harnesses and for Claude Code, describe a pack for a review screen, read a Claude Code plugin as a pack, check out a pack from git and compare it with its release tags, preview an update, and write packs as a Claude Code marketplace.

The `tuff` command is built on these functions. They return values and never print, and they write only to the folders you give them and to temporary folders they remove.

```toml
[dependencies]
tuff-packs = "0.19"
```

```rust
use std::path::Path;

use tuff_packs::build::build_for_declared_targets;
use tuff_packs::inspect;

fn main() -> tuff_packs::Result<()> {
    let built = build_for_declared_targets(Path::new("review-pack"))?;
    let source = serde_json::json!({ "kind": "folder", "path": "review-pack" });
    let document = inspect::document(&built.artifact, &source)?;
    for member in &document.members {
        println!("{} runs code: {}", member.id, member.runs_code);
    }
    Ok(())
}
```

Installing, updating, and removing packs are not in this crate yet. Run `tuff add pack`, `tuff update`, and `tuff pack remove` with `--json` for those.

The Rust API is 0.x and can change in a minor release. The JSON documents the `tuff` command prints are the stable interface; see the [Pack JSON Specification](https://tuffcli.dev/spec/packs/). Guide: [Using Tuff from Rust](https://tuffcli.dev/guides/packs-from-rust/).
