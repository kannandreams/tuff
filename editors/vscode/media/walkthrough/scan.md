# Tracking what is already there

A scan reads `.claude`, `.cursor`, and `.agents` and lists every capability directory in them, with whether Tuff already tracks it.

Pick what to track and Tuff records each one **in place**: the lockfile stores the path it already has, and the files are left alone. Nothing is moved or copied.

Some directories are reported rather than offered:

- **Two directories declaring the same id.** One lockfile entry cannot hold two paths, so the pair is shown and you choose which to keep, from the terminal with `tuff add <path> --name <name>`.
- **A hook or tool with no `tuff.toml`.** Only a skill describes itself. The reason names the section that is missing.

Once tracked, a capability takes part in everything else: drift, validation, diffs, and updates.

Adopted capabilities have no upstream, since they were already on your disk, so update checks cannot report on them.
