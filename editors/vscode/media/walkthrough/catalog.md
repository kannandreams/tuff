# The built-in catalog

The catalog is a curated list compiled into your `tuff` binary: GitHub, Linear, Context7, Playwright, filesystem, and more. Each entry is resolved through the same code that installs it, so the list can never offer a server Tuff would then refuse.

Pick one and Tuff writes it into the MCP configuration of every harness this project uses, in each harness's own dialect: `.mcp.json` for Claude Code, `.cursor/mcp.json` for Cursor, `.agents/mcp.json` for the rest.

## Keys stay in your environment

An entry that needs an API key names the variables it expects and asks before installing. Tuff records the **variable name**, never its value:

```toml
GITHUB_PERSONAL_ACCESS_TOKEN = { from_env = "GITHUB_PERSONAL_ACCESS_TOKEN" }
```

Export it in the shell that runs your agent. The key reaches neither the editor, the lockfile, nor any committed file.

The rest of the registry is reachable from the terminal with `tuff mcp search`.
