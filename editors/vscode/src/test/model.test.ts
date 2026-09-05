import assert from "node:assert/strict";
import { test } from "node:test";

import {
  type ListRow,
  type OutdatedRow,
  aggregateDriftStatus,
  buildCapabilities,
  describeUpdate,
  groupByType,
  statusBarText,
  summarize,
  updateKind,
  versionLabel,
} from "../model";

function listRow(overrides: Partial<ListRow> = {}): ListRow {
  return {
    id: "example",
    type: "skill",
    version: "1.0.0",
    version_scheme: "declared",
    scope: "project",
    target: "open-agents",
    status: "clean",
    path: ".agents/skills/example",
    ...overrides,
  };
}

function outdatedRow(overrides: Partial<OutdatedRow> = {}): OutdatedRow {
  return {
    id: "example",
    type: "skill",
    target: "open-agents",
    version_scheme: "semver",
    current: "1.2.0",
    latest: "1.4.0",
    status: "outdated",
    ...overrides,
  };
}

test("a capability installed for several agents folds into one row", () => {
  const capabilities = buildCapabilities([
    listRow({ target: "claude" }),
    listRow({ target: "cursor" }),
    listRow({ target: "open-agents" }),
  ]);

  assert.equal(capabilities.length, 1);
  assert.equal(capabilities[0]?.installations.length, 3);
  assert.deepEqual(
    capabilities[0]?.installations.map((installation) => installation.target),
    ["claude", "cursor", "open-agents"],
  );
});

test("the same id in two scopes stays two rows", () => {
  const capabilities = buildCapabilities([
    listRow({ scope: "project" }),
    listRow({ scope: "global", path: "~/.agents/skills/example" }),
  ]);

  assert.equal(capabilities.length, 2);
  assert.deepEqual(
    capabilities.map((capability) => capability.scope),
    ["global", "project"],
  );
});

test("the worst drift status wins, and an unknown one is not reassurance", () => {
  assert.equal(aggregateDriftStatus(["clean", "modified", "clean"]), "modified");
  assert.equal(aggregateDriftStatus(["modified", "missing"]), "missing");
  assert.equal(aggregateDriftStatus(["clean", "clean"]), "clean");
  // A status word this extension predates must not sort below `clean`.
  assert.equal(aggregateDriftStatus(["clean", "quarantined"]), "quarantined");
  assert.equal(aggregateDriftStatus([]), "clean");
});

test("an integrity finding outranks staleness on the same capability", () => {
  const capabilities = buildCapabilities(
    [listRow({ target: "claude" }), listRow({ target: "cursor" })],
    [
      outdatedRow({ target: "claude", status: "outdated", change: "minor" }),
      outdatedRow({ target: "cursor", status: "repointed" }),
    ],
  );

  assert.equal(capabilities[0]?.update?.status, "repointed");
  assert.equal(describeUpdate(capabilities[0]?.update), "tag repointed upstream");
});

test("update descriptions read as a move, and say nothing when nothing is known", () => {
  assert.equal(describeUpdate(outdatedRow({ change: "minor" })), "1.2.0 to 1.4.0 (minor)");
  // No `change` key: the CLI omits it unless both sides are releases.
  assert.equal(describeUpdate(outdatedRow()), "1.2.0 to 1.4.0");
  assert.equal(describeUpdate(outdatedRow({ status: "up to date" })), undefined);
  assert.equal(describeUpdate(outdatedRow({ status: "not checked" })), undefined);
  assert.equal(describeUpdate(undefined), undefined);
  assert.equal(
    describeUpdate(outdatedRow({ status: "tag missing" })),
    "release tag missing upstream",
  );
  // A row with no comparable version must not render an empty move.
  assert.equal(describeUpdate(outdatedRow({ latest: null })), "1.2.0, newer available");
  // A declared version that did not move while the content did: both
  // sides are equal, and "1.3.0 to 1.3.0" would read as a bug.
  assert.equal(
    describeUpdate(outdatedRow({ current: "1.3.0", latest: "1.3.0" })),
    "1.3.0, source changed without a version bump",
  );
});

test("an unrecognised outdated status is treated as unknown, never as current", () => {
  assert.equal(updateKind("quarantined"), "unknown");
  assert.equal(updateKind("not checked"), "unknown");
  assert.equal(updateKind("up to date"), "current");
});

test("groups follow the documented type order and unknown types sort last", () => {
  const groups = groupByType(
    buildCapabilities([
      listRow({ id: "server", type: "mcp-server" }),
      listRow({ id: "future", type: "policy" }),
      listRow({ id: "skill", type: "skill" }),
      listRow({ id: "tool", type: "tool" }),
    ]),
  );

  assert.deepEqual(
    groups.map((group) => group.type),
    ["skill", "tool", "mcp-server", "policy"],
  );
});

test("the summary counts capabilities, not installations", () => {
  const summary = summarize(
    buildCapabilities(
      [
        listRow({ id: "a", target: "claude", status: "modified" }),
        listRow({ id: "a", target: "cursor", status: "modified" }),
        listRow({ id: "b", status: "missing" }),
        listRow({ id: "c" }),
      ],
      [outdatedRow({ id: "c" })],
    ),
  );

  assert.deepEqual(summary, {
    total: 3,
    modified: 1,
    missing: 1,
    outdated: 1,
    integrity: 0,
    checkedForUpdates: true,
  });
});

test("the status bar stays silent about updates until they are checked", () => {
  const unchecked = statusBarText(summarize(buildCapabilities([listRow(), listRow({ id: "b" })])));
  assert.equal(unchecked.text, "$(check) Tuff 2");
  assert.match(unchecked.tooltip, /Updates not checked yet/);
  assert.equal(unchecked.attention, false);

  const checked = statusBarText(
    summarize(buildCapabilities([listRow()], [outdatedRow({ id: "example" })])),
  );
  assert.equal(checked.text, "$(warning) Tuff 1 outdated");
  assert.doesNotMatch(checked.tooltip, /Updates not checked yet/);
  assert.equal(checked.attention, false, "an available update is not an alarm");

  const broken = statusBarText(
    summarize(buildCapabilities([listRow({ status: "missing" })])),
  );
  assert.match(broken.text, /1 missing/);
  assert.equal(broken.attention, true);
});

test("an empty inventory reads as empty rather than as healthy", () => {
  const bar = statusBarText(summarize([]));
  assert.equal(bar.text, "$(circle-outline) Tuff");
  assert.equal(bar.attention, false);
});

test("a commit version is shown as a commit", () => {
  const [pinned] = buildCapabilities([listRow({ version: "9b9c499", version_scheme: "sha" })]);
  const [released] = buildCapabilities([listRow({ version: "1.4.0", version_scheme: "semver" })]);
  assert.equal(versionLabel(pinned!), "@9b9c499");
  assert.equal(versionLabel(released!), "1.4.0");
});
