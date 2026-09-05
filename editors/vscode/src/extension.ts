/**
 * The editor surface: a tree of installed capabilities, a status bar
 * summary, and commands that hand work to the CLI.
 *
 * Everything shown here comes from `tuff --json`. The extension decides
 * nothing about capabilities on its own, so a Tuff that knows something
 * this extension does not still renders correctly.
 */

import * as os from "node:os";
import * as path from "node:path";

import * as vscode from "vscode";

import * as cli from "./cli";
import { TuffCliError, TuffNotFoundError } from "./cli";
import {
  type Capability,
  type Installation,
  type TypeGroup,
  buildCapabilities,
  describeUpdate,
  groupByType,
  statusBarText,
  summarize,
  typeLabel,
  updateKind,
  versionExplanation,
  versionLabel,
} from "./model";

type Node =
  | { kind: "group"; group: TypeGroup }
  | { kind: "capability"; capability: Capability }
  | { kind: "installation"; capability: Capability; installation: Installation };

/** Files whose change means the tree is stale. */
const WATCHED = "**/{tuff.lock,tuff.config.json}";

/** The file to open when a capability node is clicked, by capability type. */
const ENTRY_FILES = ["SKILL.md", "server.toml", "tuff.toml"];

export function activate(context: vscode.ExtensionContext): void {
  const output = vscode.window.createOutputChannel("Tuff");
  const provider = new CapabilityTreeProvider(output);
  const tree = vscode.window.createTreeView("tuff.capabilities", {
    treeDataProvider: provider,
    showCollapseAll: true,
  });

  const status = vscode.window.createStatusBarItem(vscode.StatusBarAlignment.Left, 40);
  status.command = "tuff.capabilities.focus";
  provider.onDidChangeSummary(() => {
    const bar = statusBarText(provider.summary);
    status.text = bar.text;
    status.tooltip = bar.tooltip;
    status.backgroundColor = bar.attention
      ? new vscode.ThemeColor("statusBarItem.warningBackground")
      : undefined;
    if (provider.hasProject) {
      status.show();
    } else {
      status.hide();
    }
    tree.description = provider.describeTree();
  });

  const watcher = vscode.workspace.createFileSystemWatcher(WATCHED);
  const refresh = () => void provider.refresh();
  watcher.onDidCreate(refresh);
  watcher.onDidChange(refresh);
  watcher.onDidDelete(refresh);

  context.subscriptions.push(
    output,
    tree,
    status,
    watcher,
    vscode.workspace.onDidChangeConfiguration((event) => {
      if (event.affectsConfiguration("tuff")) {
        void provider.refresh();
      }
    }),
    vscode.workspace.onDidChangeWorkspaceFolders(() => void provider.refresh()),
    vscode.commands.registerCommand("tuff.refresh", () => provider.refresh()),
    vscode.commands.registerCommand("tuff.checkUpdates", () => provider.checkUpdates()),
    vscode.commands.registerCommand("tuff.runCheck", () => provider.runCheck()),
    vscode.commands.registerCommand("tuff.mcpDoctor", () => provider.runDoctor()),
    vscode.commands.registerCommand("tuff.diff", (node?: Node) => provider.diff(node, false)),
    vscode.commands.registerCommand("tuff.diffUpstream", (node?: Node) =>
      provider.diff(node, true),
    ),
    vscode.commands.registerCommand("tuff.update", (node?: Node) => provider.update(node)),
    vscode.commands.registerCommand("tuff.reveal", (node?: Node) => provider.reveal(node)),
    vscode.commands.registerCommand("tuff.open", (target: vscode.Uri) => openTarget(target)),
    vscode.commands.registerCommand("tuff.showOutput", () => output.show(true)),
    vscode.commands.registerCommand("tuff.openSettings", () =>
      vscode.commands.executeCommand("workbench.action.openSettings", "tuff"),
    ),
  );

  void provider.refresh().then(() => {
    const startup = vscode.workspace
      .getConfiguration("tuff")
      .get<boolean>("checkUpdatesOnStartup", false);
    if (startup && provider.hasProject) {
      void provider.checkUpdates();
    }
  });
}

export function deactivate(): void {
  // Every disposable is owned by the extension context.
}

class CapabilityTreeProvider implements vscode.TreeDataProvider<Node> {
  private readonly changed = new vscode.EventEmitter<Node | undefined>();
  readonly onDidChangeTreeData = this.changed.event;

  private readonly summaryChanged = new vscode.EventEmitter<void>();
  readonly onDidChangeSummary = this.summaryChanged.event;

  private capabilities: Capability[] = [];
  private groups: TypeGroup[] = [];
  private updatesChecked = false;
  hasProject = false;

  constructor(private readonly output: vscode.OutputChannel) {}

  get summary() {
    return summarize(this.capabilities);
  }

  describeTree(): string {
    if (!this.hasProject) {
      return "";
    }
    return this.updatesChecked ? "" : "updates not checked";
  }

  /**
   * The workspace folder Tuff runs in: the first one holding a lockfile,
   * so a multi-root window with one Tuff project finds it wherever it sits.
   */
  private folder(): vscode.WorkspaceFolder | undefined {
    const folders = vscode.workspace.workspaceFolders ?? [];
    // The CLI runs against a real directory, so a virtual workspace has
    // nothing to offer it. Whether that folder is a Tuff project is left
    // to the CLI to answer in refresh().
    return folders.find((folder) => folder.uri.scheme === "file");
  }

  private options(): cli.CliOptions | undefined {
    const folder = this.folder();
    if (!folder) {
      return undefined;
    }
    const binary = vscode.workspace.getConfiguration("tuff").get<string>("path", "tuff");
    return { binary: binary.trim() || "tuff", cwd: folder.uri.fsPath };
  }

  async refresh(): Promise<void> {
    const options = this.options();
    if (!options) {
      this.apply([], false, false, false);
      return;
    }

    const scope = vscode.workspace.getConfiguration("tuff").get<string>("scope", "all");
    try {
      const rows = await cli.list(options, scope);
      // Global-scope rows come back even without a project lockfile, so a
      // project exists only when something is installed for this folder or
      // the lockfile itself is there. `list` returning at all means the CLI
      // ran, which is the distinction the welcome views care about.
      this.apply(buildCapabilities(rows, []), true, false, rows.length === 0);
    } catch (error) {
      if (error instanceof TuffNotFoundError) {
        this.apply([], false, true, true);
        return;
      }
      this.apply([], false, false, true);
      this.report(error);
    }
  }

  /**
   * Fetch update status. Separate from `refresh` and never automatic
   * without opt-in: `tuff outdated` reaches the network and clones git
   * sources, which is not something a view should do on a file save.
   */
  async checkUpdates(): Promise<void> {
    const options = this.options();
    if (!options) {
      return;
    }
    await vscode.window.withProgress(
      { location: { viewId: "tuff.capabilities" }, title: "Checking for updates" },
      async () => {
        try {
          const scope = vscode.workspace.getConfiguration("tuff").get<string>("scope", "all");
          const [rows, updates] = await Promise.all([
            cli.list(options, scope),
            cli.outdated(options),
          ]);
          this.updatesChecked = true;
          this.apply(buildCapabilities(rows, updates), true, false, rows.length === 0);
        } catch (error) {
          this.report(error);
        }
      },
    );
  }

  async runCheck(): Promise<void> {
    const options = this.options();
    if (!options) {
      return;
    }
    try {
      const outcome = await cli.check(options);
      await this.refresh();
      const failures = outcome.results.filter((result) => result.status !== "ok");
      if (outcome.valid) {
        void vscode.window.showInformationMessage(
          `Tuff: all ${outcome.results.length} installed capabilities match their recorded state.`,
        );
        return;
      }
      this.output.appendLine(`tuff check: ${failures.length} of ${outcome.results.length} failed`);
      for (const failure of failures) {
        this.output.appendLine(
          `  ${failure.id} (${failure.target}): ${failure.status}${
            failure.files?.length ? ` [${failure.files.join(", ")}]` : ""
          }`,
        );
      }
      const choice = await vscode.window.showWarningMessage(
        `Tuff: ${failures.length} ${failures.length === 1 ? "capability does" : "capabilities do"} not match their recorded state.`,
        "Show Details",
      );
      if (choice === "Show Details") {
        this.output.show(true);
      }
    } catch (error) {
      this.report(error);
    }
  }

  async runDoctor(): Promise<void> {
    const options = this.options();
    if (!options) {
      return;
    }
    this.output.show(true);
    this.output.appendLine("$ tuff mcp doctor");
    await vscode.window.withProgress(
      { location: vscode.ProgressLocation.Notification, title: "Tuff: probing MCP servers" },
      async () => {
        try {
          const result = await cli.runText(["mcp", "doctor"], options);
          this.output.appendLine(result.output || "(no output)");
        } catch (error) {
          this.report(error);
        }
      },
    );
  }

  async diff(node: Node | undefined, upstream: boolean): Promise<void> {
    const target = this.resolve(node);
    const options = this.options();
    if (!target || !options) {
      return;
    }
    const args = ["diff", target.capability.id];
    if (target.installation) {
      args.push("--agent", target.installation.target);
    }
    if (upstream) {
      args.push("--upstream");
    }
    try {
      const result = await cli.runText(args, options);
      const patch = result.output.trim();
      if (!patch) {
        void vscode.window.showInformationMessage(
          `Tuff: no ${upstream ? "upstream" : "local"} changes in ${target.capability.id}.`,
        );
        return;
      }
      const document = await vscode.workspace.openTextDocument({
        content: patch,
        language: "diff",
      });
      await vscode.window.showTextDocument(document, { preview: true });
    } catch (error) {
      this.report(error);
    }
  }

  async update(node: Node | undefined): Promise<void> {
    const target = this.resolve(node);
    const options = this.options();
    if (!target || !options) {
      return;
    }
    const args = ["update", target.capability.id];
    if (target.installation) {
      args.push("--agent", target.installation.target);
    }
    this.output.appendLine(`$ tuff ${args.join(" ")}`);
    try {
      const result = await cli.runText(args, options);
      this.output.appendLine(result.output || "(no output)");
      await this.refresh();
      if (result.code === 0) {
        void vscode.window.showInformationMessage(
          `Tuff: ${result.output.split("\n")[0] ?? `updated ${target.capability.id}`}`,
        );
        return;
      }
      const choice = await vscode.window.showWarningMessage(
        `Tuff: could not update ${target.capability.id}.`,
        "Show Output",
      );
      if (choice === "Show Output") {
        this.output.show(true);
      }
    } catch (error) {
      this.report(error);
    }
  }

  async reveal(node: Node | undefined): Promise<void> {
    const target = this.resolve(node);
    if (!target?.installation) {
      return;
    }
    const uri = this.locate(target.capability, target.installation);
    if (uri) {
      await vscode.commands.executeCommand("revealInExplorer", uri);
    }
  }

  /**
   * The capability a command should act on. A capability node acts on
   * every agent at once, matching `tuff update <id>` with no `--agent`;
   * an agent node narrows to that one.
   */
  private resolve(
    node: Node | undefined,
  ): { capability: Capability; installation?: Installation } | undefined {
    if (!node) {
      return undefined;
    }
    if (node.kind === "capability") {
      const only =
        node.capability.installations.length === 1 ? node.capability.installations[0] : undefined;
      return { capability: node.capability, ...(only ? { installation: only } : {}) };
    }
    if (node.kind === "installation") {
      return { capability: node.capability, installation: node.installation };
    }
    return undefined;
  }

  /** Where an installation lives on disk, or undefined if unresolvable. */
  private locate(capability: Capability, installation: Installation): vscode.Uri | undefined {
    const folder = this.folder();
    if (!folder) {
      return undefined;
    }
    // `tuff list` prefixes a global row's path with `~/`, since it is
    // recorded against the home directory rather than the project.
    if (installation.path.startsWith("~/")) {
      return vscode.Uri.file(path.join(os.homedir(), installation.path.slice(2)));
    }
    if (capability.scope === "global") {
      return vscode.Uri.file(path.join(os.homedir(), installation.path));
    }
    return vscode.Uri.joinPath(folder.uri, installation.path);
  }

  private apply(
    capabilities: Capability[],
    hasProject: boolean,
    cliMissing: boolean,
    empty: boolean,
  ): void {
    this.capabilities = capabilities;
    this.groups = groupByType(capabilities);
    this.hasProject = hasProject;
    if (!hasProject) {
      this.updatesChecked = false;
    }
    void vscode.commands.executeCommand("setContext", "tuff.cliMissing", cliMissing);
    void vscode.commands.executeCommand("setContext", "tuff.hasProject", hasProject);
    void vscode.commands.executeCommand("setContext", "tuff.empty", empty);
    this.changed.fire(undefined);
    this.summaryChanged.fire();
  }

  private report(error: unknown): void {
    if (error instanceof TuffNotFoundError) {
      void vscode.window
        .showErrorMessage(
          `Tuff: could not run '${error.binary}'. Install the CLI or set tuff.path.`,
          "Open Settings",
        )
        .then((choice) => {
          if (choice === "Open Settings") {
            void vscode.commands.executeCommand("tuff.openSettings");
          }
        });
      return;
    }
    if (error instanceof TuffCliError) {
      this.output.appendLine(`error: ${error.message}`);
      if (error.hint) {
        this.output.appendLine(`hint: ${error.hint}`);
      }
      void vscode.window
        .showErrorMessage(`Tuff: ${error.message}`, ...(error.hint ? ["Show Output"] : []))
        .then((choice) => {
          if (choice === "Show Output") {
            this.output.show(true);
          }
        });
      return;
    }
    const message = error instanceof Error ? error.message : String(error);
    this.output.appendLine(`error: ${message}`);
    void vscode.window.showErrorMessage(`Tuff: ${message}`);
  }

  getTreeItem(node: Node): vscode.TreeItem {
    switch (node.kind) {
      case "group":
        return groupItem(node.group);
      case "capability":
        return this.capabilityItem(node.capability);
      case "installation":
        return this.installationItem(node.capability, node.installation);
    }
  }

  getChildren(node?: Node): Node[] {
    if (!node) {
      return this.groups.map((group) => ({ kind: "group", group }));
    }
    if (node.kind === "group") {
      return node.group.capabilities.map((capability) => ({ kind: "capability", capability }));
    }
    if (node.kind === "capability" && node.capability.installations.length > 1) {
      return node.capability.installations.map((installation) => ({
        kind: "installation",
        capability: node.capability,
        installation,
      }));
    }
    return [];
  }

  private capabilityItem(capability: Capability): vscode.TreeItem {
    const many = capability.installations.length > 1;
    const item = new vscode.TreeItem(
      capability.id,
      many
        ? vscode.TreeItemCollapsibleState.Collapsed
        : vscode.TreeItemCollapsibleState.None,
    );
    const update = describeUpdate(capability.update);
    const agents = many
      ? `${capability.installations.length} agents`
      : (capability.installations[0]?.target ?? "");
    item.description = [versionLabel(capability), agents, update]
      .filter((part) => part)
      .join("  ");
    item.iconPath = statusIcon(capability.status, capability.update?.status);
    item.contextValue = "tuff.capability";
    item.tooltip = this.tooltip(capability);
    if (!many) {
      const installation = capability.installations[0];
      if (installation) {
        const uri = this.locate(capability, installation);
        if (uri) {
          item.resourceUri = uri;
          item.command = {
            command: "tuff.open",
            title: "Open",
            arguments: [uri],
          };
        }
      }
    }
    return item;
  }

  private installationItem(
    capability: Capability,
    installation: Installation,
  ): vscode.TreeItem {
    const item = new vscode.TreeItem(
      installation.target,
      vscode.TreeItemCollapsibleState.None,
    );
    item.description = [installation.status, describeUpdate(installation.update)]
      .filter((part) => part)
      .join("  ");
    item.iconPath = statusIcon(installation.status, installation.update?.status);
    item.contextValue = "tuff.installation";
    item.tooltip = new vscode.MarkdownString(
      `\`${capability.id}\` for **${installation.target}**\n\n${installation.path}`,
    );
    const uri = this.locate(capability, installation);
    if (uri) {
      item.resourceUri = uri;
      item.command = { command: "tuff.open", title: "Open", arguments: [uri] };
    }
    return item;
  }

  private tooltip(capability: Capability): vscode.MarkdownString {
    const lines = [
      `**${capability.id}** — ${capability.type}`,
      "",
      `Version \`${capability.version}\`, ${versionExplanation(capability.versionScheme)}.`,
      `Scope: ${capability.scope}.`,
      `Status: ${capability.status}.`,
    ];
    const update = describeUpdate(capability.update);
    if (update) {
      lines.push(`Update: ${update}.`);
    } else if (!this.updatesChecked) {
      lines.push("Updates not checked yet.");
    }
    for (const installation of capability.installations) {
      lines.push("", `- \`${installation.target}\`: ${installation.path}`);
    }
    return new vscode.MarkdownString(lines.join("\n"));
  }
}

function groupItem(group: TypeGroup): vscode.TreeItem {
  const item = new vscode.TreeItem(
    typeLabel(group.type),
    vscode.TreeItemCollapsibleState.Expanded,
  );
  item.description = String(group.capabilities.length);
  item.contextValue = "tuff.group";
  return item;
}

/**
 * The icon for a row. Drift wins over staleness: a capability edited by
 * hand is a fact about this machine, while an available update is a fact
 * about somewhere else, and the first is the one to act on.
 */
function statusIcon(drift: string, update: string | undefined): vscode.ThemeIcon {
  switch (drift) {
    case "missing":
      return new vscode.ThemeIcon("error", new vscode.ThemeColor("charts.red"));
    case "error":
      return new vscode.ThemeIcon("warning", new vscode.ThemeColor("charts.red"));
    case "modified":
      return new vscode.ThemeIcon("circle-filled", new vscode.ThemeColor("charts.yellow"));
    default:
      break;
  }
  switch (update ? updateKind(update) : "unknown") {
    case "integrity":
      return new vscode.ThemeIcon("alert", new vscode.ThemeColor("charts.red"));
    case "outdated":
      return new vscode.ThemeIcon("arrow-circle-up", new vscode.ThemeColor("charts.blue"));
    case "error":
      return new vscode.ThemeIcon("question", new vscode.ThemeColor("charts.yellow"));
    default:
      return new vscode.ThemeIcon("pass", new vscode.ThemeColor("charts.green"));
  }
}

/**
 * Open what a row points at. A capability is a directory, which an editor
 * cannot open, so its entry file is opened when there is one and the
 * Explorer reveals the directory otherwise.
 */
async function openTarget(uri: vscode.Uri): Promise<void> {
  let stat: vscode.FileStat;
  try {
    stat = await vscode.workspace.fs.stat(uri);
  } catch {
    void vscode.window.showWarningMessage(`Tuff: ${uri.fsPath} is not on disk.`);
    return;
  }
  if (stat.type === vscode.FileType.File) {
    await vscode.window.showTextDocument(uri, { preview: true });
    return;
  }
  for (const name of ENTRY_FILES) {
    const candidate = vscode.Uri.joinPath(uri, name);
    try {
      await vscode.workspace.fs.stat(candidate);
      await vscode.window.showTextDocument(candidate, { preview: true });
      return;
    } catch {
      continue;
    }
  }
  await vscode.commands.executeCommand("revealInExplorer", uri);
}
