import { useCallback, useEffect, useLayoutEffect, useRef, useState } from 'react';
import { Link } from 'react-router-dom';
import { readLocalNow, readNow, NowReadError, type LocalNowUpdate, type NowUpdate } from '@/api/now';
import { NativeRecordReadError } from '@/api/nativeRecordRead';
import { useProviderMode } from '@/store/providerMode';
import { projectRecordHref, type ProjectRecordKind } from '@/api/nativeRecordNavigation';
import { useTaskWorkspace, taskContextKey } from '@/components/task/taskWorkspaceState';
import { usePageRefresh } from '../layout/WorkspaceRefresh';
import { SessionTaskProgress, WorkEmpty, WorkNotice, WorkSection } from '../shared/EngineeringUI';

const sections = [['sessions', 'Active sessions', 'No session is currently claimed.', 'session'], ['tasks', 'Active tasks', 'No task is currently claimed.', 'task'], ['memories', 'Recent memory', 'No project memories yet.', 'memory'], ['knowledge', 'Recently maintained knowledge', 'No indexed project documents yet.', 'knowledge']] as const;
function dateLabel(value: string) {
 if (/^-?\d+$/.test(value)) { const milliseconds = Number(BigInt(value) / 1000000n); return Math.abs(milliseconds) <= 8640000000000000 ? new Date(milliseconds).toLocaleString() : value; }
 return new Date(value).toLocaleString();
}
export function WorkspaceNow() {
 const { selection, phase } = useTaskWorkspace();
 const provider = useProviderMode();
 const local = provider === 'local';
 const project = phase === 'ready' && provider !== 'loading' ? selection?.project_id : undefined;
 const scope = taskContextKey(selection) + ':' + provider;
 const scopeRef = useRef(scope); useLayoutEffect(() => { scopeRef.current = scope; }, [scope]);
 const connection = useRef<AbortController | null>(null);
 const retry = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
 const [snapshot, setSnapshot] = useState<{ scope: string; data: NowUpdate | LocalNowUpdate } | null>(null);
 const [status, setStatus] = useState<'connecting' | 'live' | 'reconnecting' | 'error'>('connecting');
 const [error, setError] = useState('');
 const connect = useCallback(function connectStream(): Promise<void> {
  if (!project) return Promise.resolve();
  clearTimeout(retry.current);
  connection.current?.abort();
  const controller = new AbortController();
  let received!: () => void;
  const ready = new Promise<void>(resolve => { received = resolve; });
  controller.signal.addEventListener('abort', received, { once: true });
  connection.current = controller;
  setStatus(previous => local && previous === 'live' ? previous : 'connecting');
  const update = (data: NowUpdate | LocalNowUpdate) => {
   if (controller.signal.aborted || scopeRef.current !== scope) return;
   setSnapshot({ scope, data }); setStatus('live'); setError(''); received();
  };
  const read = local ? readLocalNow(selection!, controller.signal).then(update) : readNow(selection!, controller.signal, update);
  void read.then(() => {
   if (controller.signal.aborted) return;
   if (!local) throw new NowReadError(502, 'The activity connection ended.');
   retry.current = setTimeout(() => void connectStream(), 15000);
  }).catch(cause => {
   if (controller.signal.aborted || scopeRef.current !== scope) return;
   setSnapshot(null);
   const retryable = cause instanceof NativeRecordReadError ? ![400, 401, 403, 404].includes(cause.status) : !(cause instanceof NowReadError) || cause.retryable;
   setError(cause instanceof Error ? cause.message : 'The activity connection was interrupted.');
   setStatus(retryable ? 'reconnecting' : 'error');
   if (retryable) retry.current = setTimeout(() => { if (!controller.signal.aborted && scopeRef.current === scope) void connectStream(); }, 2000);
  }).finally(() => { controller.signal.removeEventListener('abort', received); received(); });
  return ready;
 }, [project, selection, scope, local]);
 useEffect(() => {
  void connect();
  return () => { clearTimeout(retry.current); connection.current?.abort(); connection.current = null; };
 }, [connect]);
 usePageRefresh(connect);
 const data = snapshot?.scope === scope ? snapshot.data : null;
 const observations = data ? ('instances' in data.snapshot
   ? data.snapshot.instances.map(instance => ({ id: instance.instance_id, title: <>Instance <code>{instance.instance_id}</code> · {instance.version}</>, now: instance.now }))
   : [{ id: 'local', title: <>Local checkout · {data.selection.project_version}</>, now: data.snapshot }]) : [];
 if (provider === 'loading') return <p role="status">Checking the workspace provider…</p>;
 if (!project) return <WorkEmpty title="Select a project">Choose a project in the header to follow its work.</WorkEmpty>;
 return <div className="workspace-now">
  <div className="now-status" role="status">
   <span className="now-live-dot" /> <strong>{local ? 'Local project activity' : 'Project activity · all contributors'}</strong>
   <span>{status === 'live' ? local ? 'Refreshes every 15 seconds' : 'Live updates' : status === 'reconnecting' ? 'Reconnecting…' : status === 'error' ? 'Disconnected' : 'Connecting…'}{data && ` · Version ${data.selection.project_version}`}</span>
  </div>
  <p className="text-sm text-muted-foreground">Active work has a current owner and an unexpired lease.</p>
  {error && <WorkNotice title="Activity unavailable" tone="error">{error}{status === 'reconnecting' ? ' Reconnecting automatically.' : ' Use the header refresh to retry.'}</WorkNotice>}
  {!data && !error && <p role="status">Loading project activity…</p>}
  {data && !observations.length && <WorkEmpty title="No online instances">Activity appears when an authorized instance publishes this project version.</WorkEmpty>}
  {data && observations.map(instance => <section key={instance.id} aria-label={local ? 'Local checkout' : `Instance ${instance.id}`}>
   <h2>{instance.title}</h2>
   <p className="text-sm text-muted-foreground">Observed at {dateLabel(instance.now.generated_at)}</p>
   <div className="now-grid">{sections.map(([key, title, empty, kind]) => {
    const section = instance.now.sections[key];
    return <WorkSection key={key} title={title} description={section?.total != null ? `${section.total} records${section.has_more ? ' · showing the latest 20' : ''}` : undefined}>
     {!section || section.availability === 'failed' ? <WorkNotice title="This source is unavailable" tone="error">{section?.error === 'forbidden' ? 'Access to this source is not permitted.' : 'No response from this source.'}</WorkNotice> : !section.items.length ? <p className="text-sm text-muted-foreground">{empty}</p> : section.items.map(item => {
      const id = item.id ?? item.slug!;
      return <article className="now-record" key={id}>
       <Link className="record-title" to={projectRecordHref(kind as ProjectRecordKind, id, data.selection)}>{item.title || id}</Link>
       <div className="now-record-meta"><code>{id}</code>{(item.owner || item.updated_by) && <span>Responsible: {item.owner || item.updated_by}</span>}{item.updated_at && <time dateTime={item.updated_at}>{dateLabel(item.updated_at)}</time>}{item.lease_expires_at && <span>Lease until {dateLabel(item.lease_expires_at)}</span>}</div>
       {key === 'sessions' && <SessionTaskProgress completed={item.completed_tasks ?? 0} total={item.total_tasks ?? 0} />}
      </article>;
     })}
    </WorkSection>;
   })}</div>
  </section>)}
 </div>;
}
