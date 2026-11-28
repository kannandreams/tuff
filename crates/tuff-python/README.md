# tuff-sdk

The Tuff SDK for Python. Install, update, and remove agent capability packs (skills, MCP servers, and hooks for Claude Code, Codex, Cursor, OpenCode, Pi, and Open Agents) from a Python application, with a plan you can show before anything is written.

```sh
pip install tuff-sdk
```

```python
import tuff_sdk as tuff

workspace = tuff.Workspace.open("my-project")
pack = tuff.Pack.checkout("https://github.com/example/review-pack", targets=["claude"])

plan = workspace.plan_install(pack, ["claude"])
for file in plan.files:
    print(file.change, file.path)

workspace.apply(plan)
```

The module runs the same Rust code as the `tuff` command, in process. It never prints: notes and warnings are lists on the results. Errors are `tuff_sdk.TuffError` subclasses with `kind` and `hint`.

The API is 0.x and can change in a minor release. Guide: [Tuff from Python](https://tuffcli.dev/sdk/python/). The command-line tool is the separate `tuffcli` package.
