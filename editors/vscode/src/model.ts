/**
 * The shapes `tuff --json` emits, and the pure transforms the views need.
 *
 * Nothing in this file imports `vscode`. The extension is a client of the
 * CLI rather than a second implementation of it, so the parsing and the
 * folding live here where `node --test` can reach them, and the editor
 * API stays a thin shell in `extension.ts`.
 */

/** What a lockfile entry's `version` string actually is (RFC-101). */
export type VersionScheme = "semver" | "declared" | "sha";

/** One row of `tuff list --json`: a capability at one agent, in one scope. */
export interface ListRow {
  id: string;
  type: string;
  version: string;
  version_scheme: VersionScheme;
  scope: string;
  target: string;
  status: string;
  path: string;
}

/** One row of `tuff outdated --json`. */
export interface OutdatedRow {
  id: string;
  type: string;
  target: string;
  version_scheme: VersionScheme;
  current: string;
  /** Null where the CLI table prints a dash: nothing to compare against. */
  latest: string | null;
  status: string;
  /** `major`, `minor`, or `patch`, only when both sides are releases. */
  change?: string;
}

/** One result of `tuff check --json`. */
export interface CheckResult {
  id: string;
  type: string;
  target: string;
  status: string;
  files?: string[];
}

export interface CheckOutcome {
  valid: boolean;
  results: CheckResult[];
}

/** A capability as installed for one agent. */
export interface Installation {
  target: string;
  /** `clean`, `modified`, `missing`, or `error`, as `tuff list` reports. */
  status: string;
  /** Path to the installed tree, relative to the scope root. */
  path: string;
  update?: OutdatedRow;
}

/** A capability, with every agent it is installed for folded into it. */
export interface Capability {
  /** Stable identity for tree state: scope, type, and id. */
  key: string;
  id: string;
  type: string;
  scope: string;
  version: string;
  versionScheme: VersionScheme;
  /** The worst status across its installations. */
  status: string;
  installations: Installation[];
  /** The most serious update finding across its installations, if any. */
  update?: OutdatedRow;
}

/**
 * How bad a drift status is, so folding many agents into one row can pick
 * the one worth showing. Unknown words sort above `clean` rather than
 * below it: a status this extension has not heard of is not reassurance.
 */
const DRIFT_SEVERITY: ReadonlyMap<string, number> = new Map([
  ["clean", 0],
  ["not checked", 1],
  ["modified", 3],
  ["error", 4],
  ["missing", 5],
]);

const UNKNOWN_DRIFT_SEVERITY = 2;

export function driftSeverity(status: string): number {
  return DRIFT_SEVERITY.get(status) ?? UNKNOWN_DRIFT_SEVERITY;
}

/**
 * The status to show for a capability installed for several agents, which
 * is the worst of them. `tuff list` folds its own rows the same way.
 */
export function aggregateDriftStatus(statuses: readonly string[]): string {
  let worst = "clean";
  for (const status of statuses) {
    if (driftSeverity(status) > driftSeverity(worst)) {
      worst = status;
    }
  }
  return worst;
}

/**
 * How an `outdated` status should read. `repointed` and `tag missing` are
 * integrity findings rather than staleness: the release you have may not
 * be the release it claims to be, so they outrank a plain `outdated`.
 */
export type UpdateKind = "current" | "unknown" | "outdated" | "integrity" | "error";

export function updateKind(status: string): UpdateKind {
  switch (status) {
    case "up to date":
      return "current";
    case "outdated":
      return "outdated";
    case "repointed":
    case "tag missing":
      return "integrity";
    case "error":
      return "error";
    default:
      // `not checked`, and anything a newer CLI adds: an absence of
      // information, which must not read as a clean bill of health.
      return "unknown";
  }
}

const UPDATE_SEVERITY: Record<UpdateKind, number> = {
  current: 0,
  unknown: 1,
  error: 2,
  outdated: 3,
  integrity: 4,
};

/** A one-line summary of an update finding, or undefined when there is none. */
export function describeUpdate(update: OutdatedRow | undefined): string | undefined {
  if (!update) {
    return undefined;
  }
  switch (updateKind(update.status)) {
    case "outdated": {
      // A git source whose content moved without its declared version
      // moving reports both sides equal. Saying "1.3.0 to 1.3.0" would
      // read as a bug; the honest reading is that the version stopped
      // tracking the content, which is why a declared version is weaker
      // than a released tag.
      if (update.latest === null) {
        return `${update.current}, newer available`;
      }
      if (update.latest === update.current) {
        return `${update.current}, source changed without a version bump`;
      }
      const size = update.change ? ` (${update.change})` : "";
      return `${update.current} to ${update.latest}${size}`;
    }
    case "integrity":
      return update.status === "repointed"
        ? "tag repointed upstream"
        : "release tag missing upstream";
    case "error":
      return "upstream unavailable";
    default:
      return undefined;
  }
}

/**
 * Fold `list` rows into capabilities and hang any `outdated` rows off them.
 *
 * Rows are grouped by scope, type, and id, so the same id installed in both
 * project and global scope stays two rows, as the CLI table shows it.
 * `outdated` does not report a scope, so its rows are matched on id and
 * agent alone; an id installed in both scopes therefore shows the same
 * update finding on both.
 */
export function buildCapabilities(
  list: readonly ListRow[],
  outdated: readonly OutdatedRow[] = [],
): Capability[] {
  const updates = new Map<string, OutdatedRow>();
  for (const row of outdated) {
    updates.set(`${row.id} ${row.target}`, row);
  }

  const byKey = new Map<string, Capability>();
  for (const row of list) {
    const key = `${row.scope} ${row.type} ${row.id}`;
    let capability = byKey.get(key);
    if (!capability) {
      capability = {
        key,
        id: row.id,
        type: row.type,
        scope: row.scope,
        version: row.version,
        versionScheme: row.version_scheme,
        status: row.status,
        installations: [],
      };
      byKey.set(key, capability);
    }
    const update = updates.get(`${row.id} ${row.target}`);
    capability.installations.push({
      target: row.target,
      status: row.status,
      path: row.path,
      ...(update ? { update } : {}),
    });
  }

  const capabilities = [...byKey.values()];
  for (const capability of capabilities) {
    capability.installations.sort((a, b) => a.target.localeCompare(b.target));
    capability.status = aggregateDriftStatus(
      capability.installations.map((installation) => installation.status),
    );
    const worst = worstUpdate(capability.installations);
    if (worst) {
      capability.update = worst;
    }
  }
  capabilities.sort(
    (a, b) =>
      a.scope.localeCompare(b.scope) ||
      a.type.localeCompare(b.type) ||
      a.id.localeCompare(b.id),
  );
  return capabilities;
}

function worstUpdate(installations: readonly Installation[]): OutdatedRow | undefined {
  let worst: OutdatedRow | undefined;
  for (const installation of installations) {
    const update = installation.update;
    if (!update) {
      continue;
    }
    if (
      !worst ||
      UPDATE_SEVERITY[updateKind(update.status)] > UPDATE_SEVERITY[updateKind(worst.status)]
    ) {
      worst = update;
    }
  }
  return worst;
}

/** Capabilities grouped for display, one group per capability type. */
export interface TypeGroup {
  type: string;
  capabilities: Capability[];
}

/** Sort order for the type groups: the order the docs introduce them. */
const TYPE_ORDER = ["skill", "tool", "hook", "workflow", "mcp-server"];

export function groupByType(capabilities: readonly Capability[]): TypeGroup[] {
  const byType = new Map<string, Capability[]>();
  for (const capability of capabilities) {
    const group = byType.get(capability.type);
    if (group) {
      group.push(capability);
    } else {
      byType.set(capability.type, [capability]);
    }
  }
  return [...byType.entries()]
    .map(([type, capabilities]) => ({ type, capabilities }))
    .sort((a, b) => {
      const left = TYPE_ORDER.indexOf(a.type);
      const right = TYPE_ORDER.indexOf(b.type);
      // A type this extension predates sorts last rather than first.
      return (
        (left === -1 ? TYPE_ORDER.length : left) - (right === -1 ? TYPE_ORDER.length : right) ||
        a.type.localeCompare(b.type)
      );
    });
}

export interface Summary {
  total: number;
  modified: number;
  missing: number;
  outdated: number;
  integrity: number;
  /** Whether any update information has been fetched at all. */
  checkedForUpdates: boolean;
}

export function summarize(capabilities: readonly Capability[]): Summary {
  const summary: Summary = {
    total: capabilities.length,
    modified: 0,
    missing: 0,
    outdated: 0,
    integrity: 0,
    checkedForUpdates: capabilities.some((capability) => capability.update !== undefined),
  };
  for (const capability of capabilities) {
    if (capability.status === "missing") {
      summary.missing += 1;
    } else if (driftSeverity(capability.status) >= driftSeverity("modified")) {
      summary.modified += 1;
    }
    const kind = capability.update ? updateKind(capability.update.status) : "unknown";
    if (kind === "integrity") {
      summary.integrity += 1;
    } else if (kind === "outdated") {
      summary.outdated += 1;
    }
  }
  return summary;
}

export interface StatusBar {
  text: string;
  tooltip: string;
  /** True when something needs a person, so the item can be highlighted. */
  attention: boolean;
}

/**
 * The status bar line. It reports only what has actually been checked:
 * until `tuff outdated` has run, it says nothing about updates rather than
 * implying everything is current.
 */
export function statusBarText(summary: Summary): StatusBar {
  if (summary.total === 0) {
    return {
      text: "$(circle-outline) Tuff",
      tooltip: "Tuff: no capabilities installed",
      attention: false,
    };
  }

  const parts: string[] = [];
  const tooltip: string[] = [
    `${summary.total} ${summary.total === 1 ? "capability" : "capabilities"}`,
  ];
  if (summary.missing > 0) {
    parts.push(`${summary.missing} missing`);
    tooltip.push(`${summary.missing} missing`);
  }
  if (summary.modified > 0) {
    parts.push(`${summary.modified} modified`);
    tooltip.push(`${summary.modified} modified locally`);
  }
  if (summary.integrity > 0) {
    parts.push(`${summary.integrity} repointed`);
    tooltip.push(`${summary.integrity} with a release tag that moved or vanished upstream`);
  }
  if (summary.outdated > 0) {
    parts.push(`${summary.outdated} outdated`);
    tooltip.push(`${summary.outdated} with a newer version available`);
  }
  if (!summary.checkedForUpdates) {
    tooltip.push("Updates not checked yet; run Tuff: Check for Updates");
  }

  if (parts.length === 0) {
    return {
      text: `$(check) Tuff ${summary.total}`,
      tooltip: tooltip.join("\n"),
      attention: false,
    };
  }
  return {
    text: `$(warning) Tuff ${parts.join(", ")}`,
    tooltip: tooltip.join("\n"),
    attention: summary.missing > 0 || summary.integrity > 0,
  };
}

/** Human-readable label for a capability type, for the group headings. */
export function typeLabel(type: string): string {
  switch (type) {
    case "skill":
      return "Skills";
    case "tool":
      return "Tools";
    case "hook":
      return "Hooks";
    case "workflow":
      return "Workflows";
    case "mcp-server":
      return "MCP Servers";
    default:
      return type;
  }
}

/**
 * The version as the tree shows it. A `sha` version is a commit rather
 * than a version anybody chose, so it is prefixed to read as one.
 */
export function versionLabel(capability: Capability): string {
  return capability.versionScheme === "sha" ? `@${capability.version}` : capability.version;
}

/**
 * Why that version string is worth what it is. `tuff list` prints a
 * git-sourced declared version as `1.2.0 (declared)` because a version the
 * author merely wrote is weaker than a released tag; `list --json` does not
 * say which source a row came from, so the tree carries the explanation in
 * the tooltip instead of guessing at the source.
 */
export function versionExplanation(scheme: VersionScheme): string {
  switch (scheme) {
    case "semver":
      return "a release chosen by tag";
    case "declared":
      return "declared by the source, not a release";
    case "sha":
      return "a pinned commit; the source declares no version";
  }
}
