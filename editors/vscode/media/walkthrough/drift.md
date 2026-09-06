# Drift, and updates when you ask

Every tracked capability shows one of a few states:

- **clean** — the files match what was installed.
- **modified** — something was edited by hand since. Open **Show Local Changes** on the row to see a diff, and **Update Capability** to accept the edit as the new baseline or to restore the source.
- **missing** — the files are gone.

The status bar carries the counts, so a hand edit is visible before an agent session runs into it.

## Updates are a command, not a background task

Checking for updates reaches the network and clones git sources, which a sidebar should not do every time a file is saved. Run **Tuff: Check for Updates** and rows gain the move available, such as `1.2.0 to 1.4.0 (minor)`.

Until that has run, the view says *updates not checked* rather than showing everything as current. A release tag that moved or vanished upstream is reported as its own finding, not as staleness.
