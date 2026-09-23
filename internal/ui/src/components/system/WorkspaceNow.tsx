import { useCallback, useEffect, useLayoutEffect, useRef, useState } from 'react';
import { Link } from 'react-router-dom';
import { api } from '@/api/client';
import { useAppStore } from '@/store/appStore';
import { usePageRefresh } from '../layout/WorkspaceRefresh';
import { MarkdownContent } from '../wiki/WikiMarkdown';
import { SessionTaskProgress, WorkEmpty, WorkNotice, WorkSection } from '../shared/EngineeringUI';

interface Item { id: string; title: string; href: string; owner?: string; updated_at?: string; status?: string; lease_expires_at?: string; progress?: string; next_step?: string; completed_tasks?: number; total_tasks?: number }
interface Section { items: Item[]; total: number; has_more: boolean; error?: string }
export interface NowSnapshot { project_id: string; generated_at: string; sections: Record<string, Section> }
const sections = [['sessions', 'Active sessions', 'No session is currently claimed.'], ['tasks', 'Active tasks', 'No task is currently claimed.'], ['memories', 'Recent memory', 'No project memories yet.'], ['knowledge', 'Recently maintained knowledge', 'No indexed project documents yet.']] as const;
function dateLabel(value: string) { return /^\d{4}-\d{2}-\d{2}$/.test(value) ? value : new Date(value).toLocaleString(); }

export function WorkspaceNow() {
  const { activeProjectDir: project, activeAgent: agent } = useAppStore();
  const scope = JSON.stringify([project, agent]);
  const scopeRef = useRef(scope); useLayoutEffect(() => { scopeRef.current = scope; }, [scope]);
  const request = useRef<{ scope: string; controller: AbortController; promise: Promise<void> } | null>(null);
  const [snapshot, setSnapshot] = useState<{ scope: string; data: NowSnapshot } | null>(null);
  const [error, setError] = useState('');
  const [refreshing, setRefreshing] = useState(false);
  const load = useCallback(() => {
    if (!project) return Promise.resolve();
    if (request.current?.scope === scope) return request.current.promise;
    request.current?.controller.abort();
    const controller = new AbortController();
    setRefreshing(true);
    const promise = (async () => {
      try {
        const data = await api.get<NowSnapshot>('/api/workspace/now?' + new URLSearchParams({ project_dir: project }), {
          signal: controller.signal,
          cache: 'no-store',
        });
        if (!controller.signal.aborted && scopeRef.current === scope) { setSnapshot({ scope, data }); setError(''); }
      } catch (e) {
        if (!controller.signal.aborted && scopeRef.current === scope) setError(e instanceof Error ? e.message : 'Activity unavailable');
      } finally {
        if (request.current?.controller === controller) { request.current = null; setRefreshing(false); }
      }
    })();
    request.current = { scope, controller, promise };
    return promise;
  }, [scope, project]);
  const [loadedScope, setLoadedScope] = useState(scope);
  if (loadedScope !== scope) { setLoadedScope(scope); setError(''); setRefreshing(false); }
  useEffect(() => {
    let active = true;
    queueMicrotask(() => { if (active) void load(); });
    const timer = setInterval(() => void load(), 5000);
    return () => { active = false; clearInterval(timer); request.current?.controller.abort(); request.current = null; };
  }, [load]);
  usePageRefresh(load);
  const data = snapshot?.scope === scope ? snapshot.data : null;
  if (!project) return <WorkEmpty title="Select a project">Choose a project in the header to follow its work.</WorkEmpty>;
  return <div className="workspace-now">
    <div className="now-status" role="status">
      <span className="now-live-dot" /> <strong>Project activity · all contributors</strong>
      <span>{refreshing ? 'Updating…' : 'Updates every 5 seconds'}{data && ` · Last received ${dateLabel(data.generated_at)}`}</span>
    </div>
    <p className="text-sm text-muted-foreground">Active work has a current owner and an unexpired lease. Responsible identities come from the records; the agent selector does not identify an individual contributor.</p>
    {error && <WorkNotice title={data ? 'Activity may be out of date' : 'Activity unavailable'} tone="error">{error}. Automatic refresh will retry.</WorkNotice>}
    {!data && !error && <p role="status">Loading project activity…</p>}
    {data && <div className="now-grid">{sections.map(([key, title, empty]) => {
      const section = data.sections[key];
      return <WorkSection key={key} title={title} description={section ? `${section.total} records${section.has_more ? ' · showing the latest 20' : ''}` : undefined}>
        {!section || section.error ? <WorkNotice title="This source is unavailable" tone="error">{section?.error || 'No response from this source.'}</WorkNotice> : !section.items.length ? <p className="text-sm text-muted-foreground">{empty}</p> : section.items.map(item => <article className="now-record" key={item.id}>
          <Link className="record-title" to={item.href}>{item.title || item.id}</Link>
          <div className="now-record-meta"><code>{item.id}</code>{item.owner && <span>Responsible: {item.owner}</span>}{item.updated_at && <time dateTime={item.updated_at}>{dateLabel(item.updated_at)}</time>}</div>
          {key === 'sessions' && <SessionTaskProgress completed={item.completed_tasks ?? 0} total={item.total_tasks ?? 0} />}
          {item.progress && <div className="markdown-preview"><MarkdownContent content={item.progress} /></div>}
          {item.next_step && <details><summary>Next action</summary><div className="markdown-preview"><MarkdownContent content={item.next_step} /></div></details>}
        </article>)}
      </WorkSection>;
    })}</div>}
  </div>;
}
