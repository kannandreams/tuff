/**
 * Running the `tuff` binary and reading what it says.
 *
 * The extension bundles no binary. It shells out to the CLI the user
 * already installed, exactly as the Claude Code plugin does, so there is
 * one Tuff on the machine and one place versions come from.
 */

import { execFile } from "node:child_process";

import type { CheckOutcome, ListRow, OutdatedRow } from "./model";

/** The `--json` failure envelope every command shares. */
export interface CliErrorEnvelope {
  error: {
    kind: string;
    message: string;
    hint?: string;
  };
}

/** A command that failed, carrying the CLI's own kind and hint. */
export class TuffCliError extends Error {
  constructor(
    message: string,
    readonly kind: string,
    readonly hint?: string,
  ) {
    super(message);
    this.name = "TuffCliError";
  }
}

/** The CLI could not be run at all: not installed, or the path is wrong. */
export class TuffNotFoundError extends Error {
  constructor(readonly binary: string) {
    super(`could not run '${binary}'`);
    this.name = "TuffNotFoundError";
  }
}

export interface CliOptions {
  binary: string;
  cwd: string;
}

interface RawResult {
  code: number;
  stdout: string;
  stderr: string;
}

/**
 * Commands can take a while: `outdated` reaches the network and `update`
 * clones. The cap is generous so a slow network reports a timeout rather
 * than a mystery, and small enough that a hung child does not wedge the
 * view for good.
 */
const TIMEOUT_MS = 120_000;
const MAX_BUFFER = 32 * 1024 * 1024;

function run(args: string[], options: CliOptions): Promise<RawResult> {
  return new Promise((resolve, reject) => {
    execFile(
      options.binary,
      args,
      {
        cwd: options.cwd,
        timeout: TIMEOUT_MS,
        maxBuffer: MAX_BUFFER,
        // A login shell is not involved, so PATH is the editor's. That is
        // also what the user's other tooling sees, so a `tuff` the editor
        // cannot find is a real problem worth reporting, not one to paper
        // over by guessing at shell configuration.
        env: process.env,
      },
      (error, stdout, stderr) => {
        if (error && (error as NodeJS.ErrnoException).code === "ENOENT") {
          reject(new TuffNotFoundError(options.binary));
          return;
        }
        const code =
          error && typeof (error as { code?: unknown }).code === "number"
            ? ((error as { code: number }).code)
            : error
              ? 1
              : 0;
        if (error && !("code" in error)) {
          reject(error);
          return;
        }
        resolve({ code, stdout, stderr });
      },
    );
  });
}

/** Read the `--json` error envelope out of stderr, if that is what it holds. */
export function parseErrorEnvelope(stderr: string): CliErrorEnvelope["error"] | undefined {
  const line = stderr
    .split("\n")
    .map((candidate) => candidate.trim())
    .filter((candidate) => candidate.startsWith("{"))
    .pop();
  if (!line) {
    return undefined;
  }
  try {
    const parsed = JSON.parse(line) as Partial<CliErrorEnvelope>;
    if (parsed.error && typeof parsed.error.message === "string") {
      return parsed.error;
    }
  } catch {
    // Not the envelope: fall through to the raw text.
  }
  return undefined;
}

/**
 * Run a `--json` command and parse its output.
 *
 * Standard output is tried first whatever the exit code, because a
 * non-zero exit is not always a failure to report: `tuff check` exits 1
 * when it finds drift and still prints a complete result on stdout.
 */
async function runJson<T>(args: string[], options: CliOptions): Promise<T> {
  const result = await run(args, options);
  const stdout = result.stdout.trim();
  if (stdout) {
    try {
      return JSON.parse(stdout) as T;
    } catch (error) {
      if (result.code === 0) {
        throw new TuffCliError(
          `could not read the output of 'tuff ${args.join(" ")}': ${(error as Error).message}`,
          "corrupt",
        );
      }
    }
  }
  const envelope = parseErrorEnvelope(result.stderr);
  if (envelope) {
    throw new TuffCliError(envelope.message, envelope.kind, envelope.hint);
  }
  throw new TuffCliError(
    result.stderr.trim() || `'tuff ${args.join(" ")}' failed with exit code ${result.code}`,
    "internal",
  );
}

/** Output of a plain, non-JSON command, for the output channel. */
export interface TextResult {
  code: number;
  output: string;
}

export async function runText(args: string[], options: CliOptions): Promise<TextResult> {
  const result = await run(args, options);
  const output = [result.stdout, result.stderr].filter((part) => part.trim()).join("\n");
  return { code: result.code, output };
}

export async function version(options: CliOptions): Promise<string> {
  const result = await run(["--version"], options);
  return result.stdout.trim() || result.stderr.trim();
}

export function list(options: CliOptions, scope: string): Promise<ListRow[]> {
  const args = ["list", "--json"];
  if (scope !== "all") {
    args.push("--scope", scope);
  }
  return runJson<ListRow[]>(args, options);
}

export function outdated(options: CliOptions): Promise<OutdatedRow[]> {
  return runJson<OutdatedRow[]>(["outdated", "--json"], options);
}

export function check(options: CliOptions): Promise<CheckOutcome> {
  return runJson<CheckOutcome>(["check", "--json"], options);
}
