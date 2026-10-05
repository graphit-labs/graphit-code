import type { ProjectVersion, ProjectVersionsResponse } from '@/api/projectVersions';

const SOURCE_NAMES: Record<string, string> = {
  ast: 'code index', knowledge: 'docs', task: 'tasks', memory: 'memory', dream: 'Dream runs',
};

function listPhrase(items: string[]): string {
  if (items.length <= 1) return items.join('');
  return `${items.slice(0, -1).join(', ')} and ${items[items.length - 1]}`;
}

function lastSegment(dir: string): string {
  return dir.split(/[\\/]/).filter(Boolean).pop() || dir;
}

/** One plain sentence about what a version is and holds, for the option and its tooltip. */
export function describeVersion(version: ProjectVersion, response: Pick<ProjectVersionsResponse, 'remote' | 'git'>): string {
  const parts: string[] = [];
  if (version.default) parts.push(response.remote ? "The project's main line" : 'Default branch');
  const checkouts = version.checkouts ?? [];
  if (checkouts.length === 1) parts.push(`checked out in ${lastSegment(checkouts[0])}`);
  else if (checkouts.length > 1) parts.push(`checked out in ${checkouts.length} worktrees`);
  const data = version.sources.filter(source => source !== 'checkout').map(source => SOURCE_NAMES[source] ?? source);
  if (data.length) parts.push(`has ${listPhrase(data)}`);
  else if (!version.default) parts.push('no Graphit data yet');
  if (response.git && !response.remote && !version.in_git) parts.push('branch no longer in Git');
  const sentence = parts.join(', ');
  return sentence.charAt(0).toUpperCase() + sentence.slice(1) + '.';
}

/**
 * The exact published artifact to read for a Hub project's selected version: the version
 * published from that branch (`branch/<version>`) when there is one, otherwise the latest.
 * The default version ("") always reads the latest.
 */
export function publishedForVersion(
  entry: { id: string; versions?: string[]; qualified_latest?: string },
  version: string,
): { context: string; matched: boolean } {
  const published = `branch/${version}`;
  if (version && entry.versions?.includes(published)) return { context: `${entry.id}@${published}`, matched: true };
  return { context: entry.qualified_latest || '', matched: !version };
}
