// The facts shown on the GitHub repository card in the site header.
//
// Read once per build and shared by every page. The version comes from the
// workspace Cargo.toml, so it is always present and needs no network. Star
// and fork counts come from the GitHub API; that call is best effort, and a
// build offline or rate-limited leaves the counts out rather than failing.
// The card's script refreshes all three in the browser afterwards.
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';

export const REPO = 'kannandreams/tuff';
export const REPO_URL = `https://github.com/${REPO}`;

export interface RepoStats {
  version: string | null;
  stars: number | null;
  forks: number | null;
}

function workspaceVersion(): string | null {
  try {
    // Astro runs from website/, one level below the workspace root.
    const toml = readFileSync(resolve(process.cwd(), '../Cargo.toml'), 'utf8');
    const match = toml.match(/^\[workspace\.package\][^[]*?^version\s*=\s*"([^"]+)"/ms);
    return match ? `v${match[1]}` : null;
  } catch {
    return null;
  }
}

async function counts(): Promise<Pick<RepoStats, 'stars' | 'forks'>> {
  const none = { stars: null, forks: null };
  try {
    const headers: Record<string, string> = {
      Accept: 'application/vnd.github+json',
      'User-Agent': 'tuffcli.dev',
    };
    if (process.env.GITHUB_TOKEN) headers.Authorization = `Bearer ${process.env.GITHUB_TOKEN}`;
    const response = await fetch(`https://api.github.com/repos/${REPO}`, {
      headers,
      signal: AbortSignal.timeout(5000),
    });
    if (!response.ok) return none;
    const body = await response.json();
    return {
      stars: typeof body.stargazers_count === 'number' ? body.stargazers_count : null,
      forks: typeof body.forks_count === 'number' ? body.forks_count : null,
    };
  } catch {
    return none;
  }
}

let stats: Promise<RepoStats> | undefined;

export function repoStats(): Promise<RepoStats> {
  stats ??= counts().then((c) => ({ version: workspaceVersion(), ...c }));
  return stats;
}

/** `1234` as `1.2k`, the way GitHub abbreviates counts. */
export function formatCount(n: number): string {
  return n >= 1000 ? `${(n / 1000).toFixed(1).replace(/\.0$/, '')}k` : String(n);
}
