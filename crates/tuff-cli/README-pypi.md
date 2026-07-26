# Tuff

Tuff manages the skills, tools, hooks, and workflows that coding agents load
from a project.

## Install

```sh
pip install tuffcli
```

For an isolated CLI installation with uv:

```sh
uv tool install tuffcli
```

The distribution is named `tuffcli`, but the installed executable is `tuff`:

```sh
tuff --version
tuff init
```

Tuff currently publishes wheels for macOS arm64, macOS x86_64, and Linux
x86_64. The package contains the Rust CLI binary and does not require Rust or
Python runtime dependencies after installation.

Documentation: https://tuffcli.dev
