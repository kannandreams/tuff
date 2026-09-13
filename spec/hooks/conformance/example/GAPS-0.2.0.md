# Gaps under SPEC.md 0.2.0

Second pass, clean-room rule unchanged: read only the three files in
`spec-0.2.0/` and this implementation's own files. (One process note: at
the start of this pass I ran `diff` between the *old* `spec/hooks-spec.json`
and the new `spec-0.2.0/hooks-spec.json` to see what had changed, which
reads the old, now off-limits file. I stopped immediately, did not use the
diff for anything beyond "only spec_version changed," and read the new
JSON directly afterward. Flagged rather than hidden -- see the final
report for the same disclosure.)

## Part 1: status of the 14 original gaps

1. **Where runtime files are installed / whether `remove` can find them.**
   CLOSED by 4.2 + 6.5. Only manifest-listed `files` are installed, under
   the hook directory with one leading `src/` stripped; removal deletes
   the whole hook directory by id, needing no files list. The tension I
   flagged (a command can't portably reach its own runtime files since
   `dir_prefix` varies per harness) is now spelled out by the spec itself
   as a "Known limitation" in 4.2, rather than left for an implementer to
   discover. See new gap N below, though -- I think this limitation is
   worse than the spec's own framing suggests.

2. **Single-quote escaping mechanism.** CLOSED by 4.3: the exact
   five-character replacement (`'"'"'`) is now specified, with a worked
   example (`echo 'hi'` -> `'echo '"'"'hi'"'"''`). I switched from the
   backslash-escape idiom I'd chosen in 0.1.0 to this exact one; they are
   behaviorally equivalent but not byte-identical, and 0.2.0's checklist
   item 9 now requires byte-for-byte conformance.

3. **Hash algorithm and hashed payload.** CLOSED by 6.4: SHA-256,
   lowercase hex, compact JSON with sorted keys and non-ASCII written as
   UTF-8 rather than escaped, over "the object carrying the command": the
   typed entry inside the group in the grouped shape (not the group), the
   entry itself in the flat shape. This is a real behavior change from my
   0.1.0 choice, which hashed the group.

4. **An event name matching no row at all.** CLOSED by 5: refused "in the
   same way as an unsupported row," with a message that "SHOULD list the
   canonical events the harness supports, with the aliases each accepts."
   Also newly explicit: a name matching an alias on an *unsupported* row
   still resolves to that row (and is refused), rather than falling
   through as if no row matched.

5. **Equality test for de-duplicating a group.** CLOSED by 6.3: "equal to
   it as a JSON value, in which object key order does not matter" --
   exact value equality, ignoring key order. This confirms exact
   structural equality was the right reading; Python dict `==` already
   ignores key order, so no implementation change was needed here, just
   confirmation.

6. **What `remove` matches on.** CLOSED by 6.5, but the answer is more
   specific than what I guessed in 0.1.0. I had matched by removing whole
   groups equal to the one we'd have written. 6.5 instead says: within
   the recorded native event, strip matching-command *entries* out of
   every group's inner array, then drop any group left empty -- so a
   group holding both our entry and a user's own entry keeps the user's
   entry. I rewrote `remove_registration` for this finer granularity.

7. **Whether a flat fragment's `version` is required.** CLOSED by 6.2 --
   and the answer is the opposite of what I chose in 0.1.0: "A flat
   fragment MAY omit `version`." I required it before; 0.2.0 makes it
   optional. Fixed.

8. **Extra structural checks in `validate-fragment` beyond the top-level
   key rule.** PARTLY CLOSED. 6.2 adds one new normative rule -- "Each
   event in `hooks` MUST be an array" -- which I now enforce as spec text,
   not a guess. But it still says nothing about the shape *inside* each
   array (whether a grouped group has a `hooks` list of typed command
   entries, whether a flat entry has a `command` field); my deeper checks
   there remain an addition beyond the letter of the spec, same as before.

9. **JSON formatting, key order, trailing newline.** CLOSED by 6.3:
   "Formatting is not significant... An implementation MAY preserve key
   order and MAY choose its own formatting, but it MUST write a file that
   is byte for byte identical when a merge changes nothing." My choice
   (2-space indent, trailing newline, insertion-order keys, same for
   merge and removal) is now explicitly licensed rather than guessed.

10. **Wrapper script file mode.** CLOSED by 4.3: "The file's mode is not
    significant, because the registration runs it through `sh`; Tuff
    writes it with the default permissions of the process." I removed
    the `chmod(0o755)` I'd added in 0.1.0 and now just rely on the
    platform's default `open()`/`write_text` permissions.

11. **Whether `tuff.toml` itself counts as a runtime file.** CLOSED by
    4.2, with a nuance my 0.1.0 blanket exclusion didn't capture: "Other
    files in the manifest's directory, including `tuff.toml` itself
    *unless it is listed*, are not [installed]." So `tuff.toml` is
    excluded by default, but an author *could* list it in `files` and
    have it installed like any other runtime file. My 0.2.0
    implementation doesn't special-case it out of the `files` list, so
    this now falls out correctly from "only listed files are installed."

12. **What the id is and whether it's sanitized.** CLOSED by 4.1: a
    thorough, explicit rule (relative path of plain names; no `\`, NUL,
    or leading/trailing whitespace; no empty/`.`/`..` segment), with
    worked valid/invalid examples. My 0.1.0 "low confidence, left
    unsanitized" note is fully resolved.

13. **Whether the merged settings file counts as one of `install`'s
    reported `files`.** STILL OPEN. This was never really a question
    about SPEC.md's own vocabulary -- SPEC.md doesn't define a `files`
    output field at all; that's this task's CLI contract layered on top
    of the spec. 0.2.0 doesn't touch it. I kept my 0.1.0 choice (`files`
    lists only what was written under the hook directory; the settings
    file is reported solely via `registrations[].settings_path`).

14. **Manifest field validation beyond what 0.1.0's spec stated.** CLOSED
    by 4.1: `id`, `version`, `description` required non-empty strings,
    `type` must be `"hook"`, are now normative, not something I had to
    infer from the worked example alone.

**Tally: 12 closed, 1 partly closed (8), 1 still open (13, and it was
always outside SPEC.md's own scope).**

## Part 2: new gaps and concerns under 0.2.0

Most important first.

### A. Section 4.4's numbered steps, read literally, contradict its own atomicity rule

**Section:** 4.4.

**What I needed:** 4.4 lists, in order: (1) validate the manifest, (2)
resolve the event, (3) write the wrapper and runtime files, (4) merge the
registration and record it. The same paragraph then says "A refusal at
any step MUST leave the project exactly as it was: no file written, no
settings file changed, nothing recorded." But 6.3 says merging (step 4)
can itself refuse -- "A file that is not a JSON object, whose `hooks` is
not an object, or whose event entry is not an array, MUST be refused as
corrupt and left unchanged." If an implementation actually performs the
steps in the order listed -- write files in step 3, *then* attempt the
merge in step 4 and discover the settings file is corrupt -- it has
already written files in step 3 before the step-4 refusal, violating "no
file written." The steps as enumerated and the atomicity guarantee in the
same paragraph cannot both be followed literally in sequence; the only
way to satisfy both is to compute (not necessarily write) everything
step 4 would do *before* performing any of step 3's filesystem writes,
which is what my implementation does (`load_settings` and
`merge_registration` build the new in-memory settings dict before
`run.sh` or any runtime file is written to disk). I think this is a
genuine internal inconsistency, not just an underspecified corner: the
numbered list reads as a sequential procedure, and a literal
implementation of it is unsafe.

**Choice:** Treat "validate" as including everything that could still
cause a refusal -- including detecting a corrupt settings file -- and
perform all of it before any write, regardless of where it falls in the
four-step list.

**Confidence:** Medium-high that this is a real tension (not just my
misreading): the atomicity sentence is unconditional ("at any step"), and
6.3's corruption refusal is unconditional too, so both can't be true
under a literal step-by-step reading. Low confidence that every
implementer would notice this rather than writing files before checking
the settings file's shape.

### B. Duplicate or colliding installed paths among `files` entries

**Section:** 4.1, 4.2.

**What I needed:** Two different `files` entries can produce the same
*installed* path after `src/` stripping (`"src/x.sh"` and `"x.sh"` both
install as `x.sh`), or a manifest can simply list the same entry twice.
Section 4.1's rules are all about a single entry's own validity; nothing
says what happens when two valid entries collide at the destination.

**Choice:** Refuse the manifest (a collision is treated as a validation
failure, before anything is written), rather than silently letting the
second copy overwrite the first.

**Confidence:** Medium. Refusing feels safer and more in the spirit of
4.1's general stance (refuse anything that could silently do something
the user wasn't shown), but "last one wins" is an equally defensible,
simpler reading, and the spec is silent either way.

### C. Does 6.3's corruption check apply to the whole `hooks` object or only the event being touched?

**Section:** 6.3.

**What I needed:** "A file... whose event entry is not an array, MUST be
refused as corrupt" -- "the event entry" could mean only the native event
this particular install is about to write into, or every key already
present under `hooks`, even ones this install never touches.

**Choice:** The stricter reading: validate every entry under `hooks`, not
just the one being merged into. A user with unrelated corruption
elsewhere in their settings file gets every install refused until they
fix it, not just installs that touch the broken key.

**Confidence:** Low-medium. The narrower reading (only check the event
we're about to write) is friendlier and arguably more in keeping with
"leave the user's other registrations and other keys alone," and I can
see an implementer choosing it instead.

### D. `remove`'s own corruption / partial-failure behavior is unspecified

**Section:** 6.5 (contrast with 6.3 and 4.4, which both speak to
`install`'s failure behavior explicitly).

**What I needed:** 4.4 states install's atomicity contract in so many
words; 6.3 states what counts as a corrupt settings file during merging.
Section 6.5 gives `remove` a five-step algorithm but never says what
happens if the recorded settings file turns out to be missing, not JSON,
or otherwise corrupt, nor whether a partial failure across multiple
recorded registrations should roll back what it already changed.

**Choice:** Best-effort, not atomic: `remove` skips a settings file that
doesn't exist, and if a settings file is present but fails to parse as
JSON, the whole command aborts with a generic error (uncaught
`json.JSONDecodeError`) after whatever it had already changed for other
registrations. In this toy that can't actually produce a half-removed
hook, because one Tuff-standard hook installed for one harness has
exactly one registration and thus one settings file; a more general
implementation juggling several registrations per removal could leave
things half-done under this behavior.

**Confidence:** Low. Nothing in 0.2.0 discusses this, so I have no textual
anchor either way; I'd guess a production implementation should probably
extend 4.4's atomicity language to `remove` too, but the spec doesn't say
so.

### E. A nested `id` can leave an empty intermediate directory behind after removal

**Section:** 6.5: "An implementation SHOULD also remove
`<dir_prefix>/hooks/` when it is left empty."

**What I needed:** Section 4.1 explicitly allows a multi-segment id such
as `security/format-check`, which installs at
`<dir_prefix>/hooks/security/format-check/`. 6.5 only names
`<dir_prefix>/hooks/` itself as a directory worth pruning when empty; it
says nothing about the intermediate `security/` directory a nested id
creates. I verified this against my own implementation: after installing
and then removing a hook with id `security/format-check` for Claude, the
hook's own directory is gone but `.claude/hooks/security/` is left behind
empty, because `.claude/hooks/` itself still contains `security/` and so
is not itself empty and not pruned.

**Choice:** Left as specified, literally: only `<dir_prefix>/hooks/` is
checked and pruned; intermediate directories created by a nested id are
not walked back and cleaned up.

**Confidence:** Medium. This reads like an oversight in 6.5 rather than a
deliberate choice -- section 4.1 clearly anticipates nested ids ("plain
names... one or more segments"), and leaving an empty directory tree
behind is exactly the kind of untidiness 6.5's existing prune rule is
trying to avoid one level up. I'd expect most implementers to either miss
this too, or generalize the "SHOULD" to every empty directory the removal
leaves behind up to `<dir_prefix>/hooks/`; I did not take that liberty,
since the sentence names one specific path.

### F. What "no leading or trailing whitespace" means for `id`, precisely

**Section:** 4.1.

**What I needed:** "Whitespace" isn't defined. I used Python's default
`str.strip()`, which strips more than ASCII space (tabs, newlines, and a
number of Unicode space characters).

**Choice:** Accept the default-`strip()` definition of whitespace.

**Confidence:** Medium-high for the common case (nobody's `id` begins or
ends with a literal space or tab), low for exotic Unicode whitespace,
which the spec almost certainly never considered either way.

### G. `id`/`files`-segment character set is only restricted by what 4.1 lists, nothing more

**Section:** 4.1.

**What I needed:** 4.1's rule for `id` (and, by "the same sense," for
`files` entries) forbids empty/`.`/`..` segments, backslash, NUL, and
edge whitespace -- and nothing else. It's silent on other characters that
are awkward or unsafe as path components on some filesystems (colons,
asterisks, control characters other than NUL, and so on).

**Choice:** Read the list as exhaustive and enforce nothing beyond it --
an id like `"a\x01b"` (an embedded, non-NUL control character, not at the
edges) is accepted.

**Confidence:** Medium-high that this is the intended reading: the rule
is written with unusual precision ("no segment is empty, `.`, or `..`,
and the whole contains no `\`, no NUL, and no leading or trailing
whitespace") compared to vaguer language elsewhere in the document, which
reads as a deliberately closed list rather than a representative sample.

### H. Refusal-message wording for an unsupported *row* doesn't ask for aliases, unlike the no-row-match case

**Section:** 5.

**What I needed:** The coverage table's refusal text for `unsupported`
says "with the row's caveat when there is one and the event names the
harness does accept" -- no mention of aliases. The very next paragraph,
about a name matching no row at all, explicitly says the message "SHOULD
list the canonical events the harness supports, with the aliases each
accepts." Taken literally, only the second case need mention aliases.

**Choice:** I mention aliases in both refusal messages, for consistency
and because it's strictly more informative, not because the first
sentence asks for it.

**Confidence:** N/A as a spec ambiguity (this is a minor implementation
liberty, not two irreconcilable readings) -- noted for completeness since
the two adjacent sentences really do differ in what they ask for.

### N. The section 4.2 "Known limitation" is a bigger problem than its framing suggests

**Section:** 4.2 (and 4.3's "An implementation does not control that
directory").

**What I needed:** Not something to resolve so much as something worth
saying plainly, since the task invites it: 4.2 now documents, honestly,
that a command can't portably reach its own bundled runtime files by a
relative path, because the hook directory's prefix differs per harness
and the command doesn't run from that directory anyway. 4.3 compounds
this: `working_directory = "."` isn't guaranteed to mean the project root
at all -- it's "resolved by the shell relative to the directory the
harness runs the hook from," which the spec says outright an
implementation "does not control." So a hook that ships a runtime file
and wants a *reliable* working directory has no portable way to get one:
it cannot name its own hook directory (harness-dependent prefix, and the
manifest is rendered once for every harness), and it cannot even fully
trust `.` to mean the project root (harness-dependent invocation
directory, undocumented per-harness and not part of the matrix this
document publishes). For a spec whose entire premise is "a hook authored
once installs anywhere," a runtime file bundled with the hook -- surely a
common case, e.g. a real script rather than a one-line shell command --
is precisely the scenario this leaves unsupported today, and 4.2 says as
much only for the "reach my own runtime file" half of the problem, not
the "even my working directory isn't guaranteed" half. I don't think this
is merely unclear; I think it's a real hole in what the spec can
currently promise, and the document would be stronger saying so in one
place rather than leaving the second half implicit in 4.3's aside.

**Choice:** Implemented exactly as specified (no portable reference is
possible; I did not invent a placeholder token, since 4.2 says a future
version may add one and none exists yet). Flagging the combination of
4.2 and 4.3 as a design concern, not picking a workaround the spec
doesn't offer.

**Confidence:** High that this is a real, not imagined, limitation (it
follows directly from the two sections' own words); this is a design
critique rather than an ambiguity, so "confidence another implementer
would agree" doesn't quite apply -- but I'd expect any implementer who
tried to actually use a bundled runtime file from a command to hit this
same wall and notice.

## Part 3: revised 0.2.0

Third pass. Clean-room rule unchanged: read only the revised SPEC.md (the
two JSON files were unchanged and not re-read), plus this
implementation's own files in `toy/`. The revision targeted sections 4.1,
4.2, 4.4, 6.3, 6.5, 7, and 10, and settles gaps A through H and N.

### Status of A-H and N

**A. Section 4.4's steps vs. its atomicity rule.** CLOSED, cleanly, by
the rewritten 4.4. The four steps are now: (1) validate the manifest,
(2) resolve the event, (3) *read the settings file and compute the
merged result*, (4) *write* everything and record. Reading and computing
the merge -- including 6.3's corruption check -- is now explicitly its
own step (3) that happens entirely before writing (step 4), so the
former tension is gone: "A refusal in steps 1 to 3 MUST leave the project
exactly as it was" is now consistent with the step order as written, not
in spite of it. The revision goes further and names the case I hadn't
considered: a failure *during* step 4 itself (full disk, an
unrepresentable name) "is an error rather than a refusal," need not
leave the project untouched, and an implementation "SHOULD report which
files it had already written." I implemented this distinction (see the
new `test_section4_4_...` test, which triggers a genuine step-4
filesystem failure without any monkeypatching).

**B. Duplicate/colliding `files` entries.** CLOSED by 4.1/4.2, and the
answer differs from my own draft choice: a *repeated* entry (literally
the same entry twice, or once with a leading `./` and once without)
installs once and is not a refusal; only two *different* entries that
would land on the same installed path are refused. I had refused both
cases alike; fixed to dedupe silently on a normalized-source-path match
and only refuse on a colliding-but-different-source destination.

**C. Whether 6.3's corruption check inspects the whole `hooks` object or
only the touched event.** CLOSED, and settled as the narrower reading I'd
flagged as the alternative to my stricter draft choice: "Events the
fragment does not add to are not inspected, and are kept exactly as they
are, whatever they hold." `load_settings` now takes the specific
`native_event` being merged into and only validates that key.

**D. `remove`'s corruption/partial-failure behavior** (not one of A-H,
but the coordinator's own re-check list named "the settings file when
not valid JSON" and "the order of steps during removal," which is this
gap). CLOSED by the rewritten 6.5: "It MUST update the settings file
before deleting any file, so that if the settings file exists but is not
valid JSON, removal stops with the hook's files still in place and the
hook still recorded." `cmd_remove` now parses every involved settings
file *before* touching any of them or deleting the hook directory, and
raises before any write if one is invalid JSON.

**E. Nested-id empty directory pruning.** CLOSED, exactly as I'd
guessed the fix should read: "SHOULD also remove each directory left
empty between the hook directory and `<dir_prefix>/hooks/`, that one
included, which for a nested id such as `security/format-check` includes
`security/`." Implemented as `_prune_empty_dirs`, walking from the hook
directory's parent up to and including `<dir_prefix>/hooks/`, stopping
at the first non-empty directory. Verified against the spec's own
`security/format-check` example.

**F. What "whitespace" means for `id`.** CLOSED: "meaning any character
with the Unicode `White_Space` property." The standard library has no
direct table for that property (`unicodedata` doesn't expose it), so I
still use `str.isspace()` per edge character as the closest available
proxy -- a minor implementation detail now pinned against explicit spec
text rather than a guess about what "whitespace" even meant.

**G. Whether 4.1's character restrictions on `id` are exhaustive.**
CLOSED, confirmed exactly as I'd guessed: "These are the only
restrictions; any other character is allowed." While re-reading 4.1
closely for this pass I noticed my *files*-entry validation hadn't
actually enforced the backslash/NUL/edge-whitespace rules at all (only
the segment-emptiness rule), even though "in the same sense as id" reads
as importing the whole rule, not just part of it. Fixed now (both `id`
and a `files` entry route through one shared `validate_plain_path`), and
covered by a new test. This was a real bug in my own 0.2.0 second-pass
code, not something the spec left ambiguous -- worth flagging since I
didn't catch it until asked to re-read 4.1 closely.

**H. Alias-listing wording inconsistency between the unsupported-row
message and the no-row-match message.** STILL OPEN. Section 5 was not
among the sections revised for this pass (the coordinator's own list of
revised sections -- 4.1, 4.2, 4.4, 6.3, 6.5, 7, 10 -- excludes it), and
comparing the text, section 5 is unchanged: the coverage table's
`unsupported` row still says only "the event names the harness does
accept," while the resolution paragraph for "no row at all" still
separately says "with the aliases each accepts." My implementation still
includes aliases in both messages, which remains a liberty beyond the
letter of the unsupported-row sentence, not something the text now
requires.

**N. The 4.2 "Known limitation" (bundled runtime files / working
directory portability).** ACKNOWLEDGED, not closed -- and acknowledged
in an unusually direct way. The revised 4.2 keeps the limitation exactly
as before (a command still cannot portably reach its own runtime files,
and closing it is still deferred to a future minor version), but now
adds: "This undercuts writing a hook once for every harness exactly when
a hook ships more than a one-line command, and *the second implementation
of this specification singled it out for that reason*." That sentence is
a direct reference to this clean-room implementation's own gap N from
the previous pass. The spec authors read the critique, agreed with it in
writing, explained precisely why the fix is deferred (it would change
"the wrapper every existing install carries," i.e. a backward-compatibility
concern, not a design disagreement), and left the underlying limitation
unfixed for a stated reason. I consider this a good outcome for a gap
report -- the point wasn't lost -- even though the limitation itself is
unchanged and a hook that ships more than a one-line command still has
no portable way to find its own files. Nothing to change in the
implementation here; noted for the record.

### New findings from this revision

1. **4.1/4.2's own worked collision example, `check.sh` vs.
   `src/check.sh`, and the "repeated entry" carve-out interact in a way
   worth double-checking:** three entries `["a.sh", "a.sh", "src/a.sh"]`
   -- two literal repeats of `a.sh`, plus a *different* source
   (`src/a.sh`) that also resolves to the installed path `a.sh` -- must
   dedupe the first two and then refuse against the third. The spec
   doesn't spell out this three-entry case, but it falls out correctly
   from the two rules read together (normalize-and-dedupe by source path
   first, then check the deduped set for installed-path collisions), and
   my implementation does this. Not really an ambiguity, just confirming
   the two new rules compose the way I'd expect; I did not find a case
   where they conflict.

2. **The internal order of step 4's three writes (wrapper, runtime
   files, settings file) is still unspecified.** 4.4 lists what step 4
   writes but not in what order, which matters only for what "files it
   had already written" means if step 4 fails partway through. I write
   the wrapper first, then runtime files, then the settings file last
   (registration recorded only once everything else has succeeded), which
   seems the more conservative order (a registration is never recorded
   for files that didn't make it to disk) but is my own choice, not
   stated.

3. **6.5's "no `hooks` object" leniency is asymmetric with 6.3's
   strictness for install, and this asymmetry is now explicit rather
   than a gap.** Install refuses a settings file whose `hooks` key is
   present but not an object (a corruption case, section 6.3). Remove,
   given the same malformed file, treats "has no `hooks` object" as
   nothing to remove and proceeds without error (6.5) -- explicitly
   distinct from the "not valid JSON" case, which does stop removal. I
   don't think this is a bug: install is creating new state so it is
   cautious about a corrupt file it would otherwise have to merge into;
   remove is only trying to find its own prior registrations, and a file
   with no usable `hooks` object trivially has none to find. Noting it
   here because it's easy to assume symmetry between the two paths and
   the spec text (correctly, I think) doesn't give it to you.

4. **Exit-code contract tension (not a SPEC.md gap, a note on top of
   it):** this task's own CLI contract (outside SPEC.md) defines only two
   outcomes for `install` -- exit 0 on success, exit 1 with a message on
   refusal. 4.4's new "error rather than a refusal" case for step 4 is a
   third outcome the CLI contract doesn't provide a code for. I reused
   exit 1 with a message that says "error while installing..." rather
   than a refusal-style message, so a caller can still distinguish the
   two by reading stderr, but cannot distinguish them by exit code alone.
   This is a property of the task's CLI contract, not something SPEC.md
   is responsible for settling, so I'm not counting it as a spec gap --
   but it's worth naming since 0.2.0 is the first version where the
   distinction exists at all.
