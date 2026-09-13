# Hooks specification conformance kit

This kit tests an implementation of the [Tuff Hooks Specification](../SPEC.md) against Tuff itself. It installs the same hook into two fresh projects, once with `tuff` and once with the implementation under test, for every harness and every event name the matrices mention, and reports every way the two differ.

## What is here

| File | What it is |
|---|---|
| `compare.py` | The comparison. Python 3.11 or newer, standard library only. |
| `example/tuff_hooks_example.py` | A second implementation of the specification, written from the specification alone by an author who never saw Tuff's source. It passes the kit. |
| `example/test_example.py` | That implementation's own tests, one or more per checklist item in section 7 of the specification. Run with `python3 -m unittest` from `example/`. |
| `example/GAPS-0.1.0.md` | Everything that author had to guess from version 0.1.0 of the specification. |
| `example/GAPS-0.2.0.md` | How a draft of 0.2.0 settled those, what it still left open, and how the final 0.2.0 text settled the rest. |

## Running it

Build Tuff, then point the kit at the binary, an implementation, and the specification document:

```sh
cargo build -p tuffcli
python3 spec/hooks/conformance/compare.py \
  --tuff target/debug/tuff \
  --impl spec/hooks/conformance/example/tuff_hooks_example.py \
  --spec spec/hooks/hooks-spec.json
```

It prints each disagreement and a summary line, and exits 0 only when there are none. `mise run check` runs it against the example on every change. `--keep` leaves the temporary projects in place for inspection.

The kit runs two groups of cases for every harness.

**Event cases**, one per canonical event, alias, and native name in the harness's matrix, plus one name no row knows. Each writes a hook with a listed runtime file under `scripts/`, an unlisted file that must not be installed, a single quote in the command, and a space in the working directory, into a project whose settings file already holds a key and a registration of the user's own. It then compares:

- whether each side installed or refused;
- on refusal, that the implementation changed nothing;
- the hook directory, file by file and byte for byte;
- the settings file, as JSON;
- the recorded registrations, including the entry hash;
- whether a partial-coverage warning was given;
- that a second install leaves each side's settings file byte for byte unchanged;
- after removal, the settings file as JSON and that the hook directory is gone.

**Rule cases**, one of each per harness, for rules a first implementation is likely to get wrong: a `files` entry listed twice, two entries that would install under the same path, a settings file that is not JSON, a settings file holding a malformed event the install does not add to, a nested id installed and removed, comparing the directories left under `hooks/`, and a removal whose settings file has become invalid JSON, comparing which files remain.

## The command-line contract

`compare.py` drives an implementation as a program. Run through `python3`, it must accept these commands. Paths it prints are relative to the project, with forward slashes.

### install

```sh
python3 IMPL install --spec SPEC_JSON --project PROJECT_DIR --harness HARNESS_ID MANIFEST_DIR
```

Installs the Tuff-standard hook in `MANIFEST_DIR` for one harness, as sections 4 to 6 of the specification describe. On success it exits 0, prints any partial-coverage warning to standard error, and prints one JSON object to standard output:

```json
{
  "id": "format-check",
  "files": ["run.sh", "scripts/check-format.sh"],
  "registrations": [
    {
      "settings_path": ".claude/settings.json",
      "native_event": "Stop",
      "canonical_event": "before_finish",
      "command": "sh .claude/hooks/format-check/run.sh",
      "entry_hash": "3e090147c235…"
    }
  ],
  "warnings": ["…"]
}
```

On refusal it exits 1 with a message on standard error and changes nothing in `PROJECT_DIR`. A failure while writing, which section 4.4 treats as an error rather than a refusal, also exits 1; its message says which files were already written. The kit does not tell the two apart, because it only ever creates conditions for refusals.

### remove

```sh
python3 IMPL remove --spec SPEC_JSON --project PROJECT_DIR --harness HARNESS_ID --registrations REGISTRATIONS_JSON HOOK_ID
```

`REGISTRATIONS_JSON` is a file holding the `registrations` array an earlier `install` printed. Removes the hook as section 6.5 describes and exits 0.

### validate-fragment and check-matrices

```sh
python3 IMPL validate-fragment --spec SPEC_JSON --harness HARNESS_ID FRAGMENT_JSON
python3 IMPL check-matrices --spec SPEC_JSON
```

Neither is used by `compare.py`. They exist so an implementation can be checked against sections 6.2 and 7 in isolation, and the example's own unit tests use them.

## Adding an implementation

An implementation in another language can take part by wrapping itself in a small Python script that honours the contract above. When the kit reports a disagreement, first decide which side is wrong. If Tuff and the specification disagree, that is a bug in one of them and worth an issue. If the specification did not say what to do, that is a gap in the specification, and closing it is the point of this kit.
