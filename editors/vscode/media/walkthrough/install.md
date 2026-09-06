# One Tuff on the machine

The extension is a view over the `tuff` command-line tool, and it runs the one you already have. It bundles no binary, so the version in the editor is the version in your terminal and in your agent's sessions.

Install it however you install tools:

```sh
uv tool install tuffcli
brew install kannandreams/tuff/tuff
cargo install tuffcli
```

Then confirm it answers:

```sh
tuff --version
```

If the editor cannot find `tuff`, the Capabilities view says so. Set **Tuff: Path** in settings to the full path, or make sure the directory it lives in is on the editor's `PATH`.

The extension needs Tuff **0.6.0** or newer. Scanning and the MCP catalog need **0.7.0**.
