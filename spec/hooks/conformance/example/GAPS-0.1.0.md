# Gaps found implementing the Tuff Hooks Specification (0.1.0)

Each entry: the section concerned, what needed deciding, the choice made
in `tuff_hooks_toy.py`, and confidence that another implementer would
land in the same place.

## 1. Where a hook's runtime files are installed, and whether `remove` can find them

**Section:** 4 ("Any files beside the manifest are the hook's runtime
files and are installed with it"), 6 ("Removing").

**What was needed:** Section 4 only names a destination for the wrapper
(`<dir_prefix>/hooks/<id>/run.sh`); it never says where sibling files of
`tuff.toml` land, and `remove`'s CLI contract (`--registrations
REGISTRATIONS_JSON HOOK_ID`) is not given a files list, only the
registration records and the hook id.

**Choice:** Install copies every file under `MANIFEST_DIR` except
`tuff.toml` itself, recursively, preserving relative paths, under
`<dir_prefix>/hooks/<id>/` next to `run.sh` (so `scripts/summarize.sh`
in the manifest lands at `.claude/hooks/<id>/scripts/summarize.sh`).
`remove` then deletes that whole directory by id, needing nothing else.
This is the only placement that makes `remove` (which gets no files
list) self-consistent, so it also decided the `remove` design.

**Wrinkle this creates:** section 4's own worked example writes
`working_directory = "."` (project root) and `command =
"scripts/summarize.sh"`. Under this placement the wrapper `cd`s to the
project root, not to the hook's own directory, so a relative command
like `scripts/summarize.sh` would in fact resolve against the *project
root*, not the copied runtime file -- unless the runtime file also
happens to exist there already, or the author sets `working_directory`
to point at the hook's own install directory (which the manifest cannot
name, since `dir_prefix` differs per harness and one manifest renders to
several harnesses). I did not find a way to reconcile "runtime files
travel with the manifest" and "the worked example's `command` is project-root-relative"
that also keeps `remove` implementable from its stated CLI contract. I
went with the placement that keeps `remove` correct as specified, and
flag the tension rather than silently picking a placement that would
make `remove` under-specified.

**Confidence:** Low. This is the single largest hole in the spec as
given to me. A different implementer working from the same three files
could plausibly place runtime files at the project root instead (mirroring
their manifest-relative path), matching the worked example more literally,
at the cost of `remove` then needing a `files` list it isn't given.

## 2. Single-quote escaping for the wrapper's two values

**Section:** 4.

**What was needed:** The template shows `'<working_directory>'` and
`'<command>'` but never says how a literal single quote *inside* one of
those values is escaped.

**Choice:** The standard POSIX idiom: close the quote, emit `\'`, reopen
the quote (`it's` -> `'it'\''s'`).

**Confidence:** High. It's the only widely-used correct technique for
this in a `sh`/`bash` single-quoted context; any implementer who tested
against a shell would converge on it or something byte-equivalent.

## 3. Hash algorithm and hashed payload for "a hash of the entry as written"

**Section:** 6 ("Recording").

**What was needed:** No algorithm is named, and "the entry" isn't
defined precisely -- it could mean the registered command string, the
single typed entry (`{"type":"command","command":...}` in the grouped
shape), or the whole group/array entry that gets added/removed as a
unit.

**Choice:** SHA-256 hex digest of the canonical (sorted-key,
no-whitespace) JSON encoding of the atomic unit that install actually
adds and remove actually matches on -- the *group* object
(`{"hooks":[{"type":"command","command":...}]}`) in the grouped shape,
the *entry* object (`{"command":...}`) in the flat shape.

**Confidence:** Medium on the algorithm (SHA-256 is the ubiquitous
default and the toolchain nearby -- `tuff.lock` -- already uses JSON,
per this project's own history, so SHA-256-of-canonical-JSON is a
natural fit), low on exactly what's hashed. Another implementation might
hash only the command string, or the whole settings-file event array, or
use a non-JSON serialization.

## 4. An event name that resolves to no row at all

**Section:** 5 ("Resolution").

**What was needed:** Section 5 describes resolution when a canonical or
alias match exists, and section 5's coverage table describes what to do
for `full`/`partial`/`unsupported` *rows*. It says nothing about a
manifest event that is neither a canonical name nor any row's alias for
the target harness (a typo, or a name that's only meaningful for a
different harness).

**Choice:** Treated as a refusal distinct from `unsupported` (which
implies a real row that the harness doesn't honour): exit 1, message
names the event, lists the seven canonical events, and lists what the
target harness does support. Nothing is written, matching the
`unsupported` write behaviour.

**Confidence:** Medium-high. The task description's own phrasing ("an
unsupported or unknown event") suggests this distinction was anticipated,
but the spec text itself never defines "unknown" as a case, so the exact
message and whether it's really a different code path from `unsupported`
is my inference.

## 5. Equality test for de-duplicating a group/entry

**Section:** 6 ("Merging": "MUST NOT add a group an event already
holds").

**What was needed:** "The same group" isn't defined -- exact structural
equality, equality of just the `command` field, or something else (e.g.
ignoring key order or extra fields another tool might have added to the
same group).

**Choice:** Exact structural (deep) equality of the JSON value, using
the identical shape this implementation itself constructs
(`{"hooks":[{"type":"command","command":cmd}]}` / `{"command":cmd}`).
This is sufficient for idempotency of this implementation's own writes
(checklist item 8), which is the only case the spec actually requires to
be idempotent.

**Confidence:** Medium. Exact-equality is the simplest reading and
suffices for the stated conformance test, but a group with the same
command plus extra keys (added by hand, or by another tool) would not be
recognised as "the same," so a second install could add a duplicate
command entry in that edge case. The spec doesn't say whether de-dup
should be by command alone within an event.

## 6. What `remove` matches on, precisely

**Section:** 6 ("Removing": "matched by command within the recorded
event").

**What was needed:** Whether "the recorded event" means the *native*
event (what's actually keyed in the settings file) or the *canonical*
event, and whether matching is against the exact group/entry structure
or just the command string appearing anywhere in that event's array.

**Choice:** Matches against `native_event` (the settings file's actual
key) and removes array elements that are exactly equal to the
group/entry this implementation would itself construct for that
command -- i.e. the same equality test as installation's de-dup, applied
in reverse. This means a hand-edited entry with the same command but
extra fields survives removal, which seems like the safer failure mode
("leave the user's other registrations... alone" beats over-deleting).

**Confidence:** Medium-high on using the native event (the settings file
has no canonical-event key to match against); medium on exact-structural
matching versus command-only matching.

## 7. What the fragment's `version` in the flat shape means: required or merely permitted

**Section:** 6 ("A hook fragment is a document in one of these shapes
containing only `hooks`, plus `version` in the flat shape.").

**What was needed:** This sentence could mean "a flat fragment MUST also
carry `version`" (mirroring the merging rule "In the flat shape it MUST
ensure `version` is present") or "a flat fragment MAY additionally carry
`version` without being refused for it" (i.e. `version` is merely on the
allow-list of top-level keys, not mandatory).

**Choice:** Required: `validate-fragment` refuses a flat-shape fragment
missing `version`, since a flat *settings file* always carries it (per
the section 6 worked example) and the merging rule elsewhere in the same
section treats it as something that must always be present.

**Confidence:** Medium. The literal normative sentence in section 6 ("An
implementation MUST refuse a fragment carrying any other top-level key")
only constrains *extra* keys, not missing ones, so a reading that treats
`version` as optional-but-allowed for a fragment is equally defensible.

## 8. Extra structural checks in `validate-fragment` beyond the top-level-key rule

**Section:** 6.

**What was needed:** The only normative rule for what makes a fragment
acceptable is the top-level-key restriction. The spec doesn't say
whether an implementation must also check that, e.g., a grouped group
actually has a `hooks` array of `{"type":"command","command":...}`
entries, or a flat entry has a `command` field.

**Choice:** Added those structural checks anyway (refusing a fragment
whose inner shape is nonsensical), on the theory that a genuinely useful
implementation shouldn't accept garbage merely because it has the right
top-level keys. This is a deliberate addition beyond the letter of the
spec, not a gap resolved by reading it more carefully.

**Confidence:** N/A (implementation choice, not a spec ambiguity) -- but
worth noting: a stricter clean-room reader might implement only the
top-level-key check and accept anything else, which would still conform.

## 9. JSON formatting, key order, and trailing newline when writing a settings file

**Section:** 6 ("Merging").

**What was needed:** No indentation, key-ordering, or newline convention
is specified for the settings file this implementation writes/rewrites.

**Choice:** `json.dumps(..., indent=2)` plus a trailing `\n`; keys keep
whatever order the in-memory dict has (insertion order: existing file's
order preserved, new keys appended at the end). This is what makes
checklist item 8 ("byte for byte unchanged") achievable without extra
bookkeeping, since re-serializing an unchanged dict is deterministic.

**Confidence:** Low on the specific formatting (2-space indent, trailing
newline) matching another implementation's choice; medium-high that
*some* deterministic, order-preserving serialization is required, since
without one item 8 (idempotent merge) couldn't hold.

## 10. Wrapper script file mode

**Section:** 4.

**What was needed:** No file mode is specified. The registration always
invokes the wrapper as `sh <dir_prefix>/hooks/<id>/run.sh`, so an
execute bit is not actually required for the harness to run it.

**Choice:** `chmod 0o755` anyway, since it's conventional for a script
file and harmless (the registration doesn't rely on it).

**Confidence:** Medium-high that this doesn't matter functionally
(confirmed by the registration always going through `sh <path>`, never
direct execution), low on the exact mode bits another implementation
would pick, since nothing requires picking any particular mode at all.

## 11. Whether `tuff.toml` itself counts as a "runtime file"

**Section:** 4.

**What was needed:** "Any files beside the manifest are the hook's
runtime files" -- does "beside" include the manifest file itself?

**Choice:** No: `tuff.toml` is excluded from the files copied into the
project. It's metadata consumed to produce the two rendered artifacts,
not a runtime file the hook's command needs at run time.

**Confidence:** High. This reading is strongly implied by "beside," and
copying `tuff.toml` itself into the harness's hook directory serves no
purpose the spec describes.

## 12. What the `id` is and where it comes from

**Section:** 4, 8 (CLI output `"id"` field).

**What was needed:** Not really ambiguous -- flagged only for
completeness. The worked example's `id = "require-log-summary"` is
clearly the manifest's own `id` field, reused verbatim as the directory
name (`<dir_prefix>/hooks/<id>/`) and the CLI output's `id`.

**Choice:** `id` = the manifest's `id` field, unvalidated beyond "must
be a non-empty string" (no character-set restriction is given, so none
is enforced here beyond what makes a safe path component in practice --
this implementation does not sanitize it against path traversal, e.g.
an `id` containing `../`, since the spec says nothing about validating
it and a clean-room reading has no basis to invent a stricter rule).

**Confidence:** High that `id` comes from the manifest; low confidence
that every implementation would leave it completely unsanitized the way
this one does -- a production implementation almost certainly should
reject a path-hostile `id`, but nothing in the three files given says so.

## 13. Whether the merged settings file counts as one of the `files` an install reports

**Section:** 8 (CLI contract, not SPEC.md, but the ambiguity is about
mapping "files" to the spec's vocabulary).

**What was needed:** The task's output contract has a `files` array
separate from `registrations` (which carries `settings_path`). It isn't
said whether `files` should also list the harness's settings file.

**Choice:** `files` lists only what install wrote under
`<dir_prefix>/hooks/<id>/` (the wrapper and any copied runtime files);
the settings file is reported solely via `registrations[].settings_path`,
since it's a merge into a possibly-pre-existing file the implementation
doesn't "own," unlike the hook's own directory.

**Confidence:** Medium-high; this reading keeps `files` a clean answer
to "what did installing this hook create," matching remove's use of the
hook's own directory as everything it needs to delete (see gap 1).

## 14. Manifest fields this implementation validates that the spec doesn't explicitly require

**Section:** 4.

**What was needed:** The spec doesn't say what to do if `type` in
`tuff.toml` is present but isn't `"hook"`, or if `id`/`[hook].event`/
`[hook].command` are missing or the wrong TOML type.

**Choice:** Refuse (exit 1, nothing written) on any of these, since this
toy only implements "the Tuff-standard hook part" of the specification
and a manifest that doesn't describe a hook, or is missing a required
field the worked example always has, cannot be safely rendered.

**Confidence:** Medium-high that *some* validation is expected (the spec
clearly treats `id`, `event`, and `command` as required by showing them
in every example and using them normatively), low on the exact error
messages/behaviour for malformed TOML, since that's not discussed at all.
