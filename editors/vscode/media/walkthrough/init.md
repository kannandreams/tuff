# What `tuff init` creates

One file, `tuff.lock`, at the root of the project. It records every capability Tuff tracks: where it came from, which version, which agents it was installed for, and a hash of what was installed, so a later edit can be noticed.

It also detects which agent harnesses the project uses, from the folders already there, and writes a short guide skill into each so an agent session knows Tuff exists.

Nothing you have written is moved, rewritten, or copied. If the project already has skills, the next step finds them.

Commit `tuff.lock`. It is the record the rest of the team, and CI, read.
