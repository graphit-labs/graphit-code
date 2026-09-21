import { useCallback, useEffect, useRef, useState } from 'react';
import { Link } from 'react-router-dom';
import { api } from '@/api/client';
import { useAppStore } from '@/store/appStore';
import { usePageRefresh } from '../layout/WorkspaceRefresh';
import { WorkSection, WorkNotice } from './EngineeringUI';
export interface ReferenceTarget { kind: string; id: string; title: string; href?: string; scope?: string; context?: string }
export interface ReferenceEdge { source: ReferenceTarget; target: ReferenceTarget; relation: string; fields: string[] }
interface References { outgoing: ReferenceEdge[]; incoming: ReferenceEdge[]; complete: boolean; warnings: string[] }
export function RecordReferences({ kind, id, scope = 'project', context = 'project' }: { kind: string; id: string; scope?: string; context?: string }) {
  const project = useAppStore(s => s.activeProjectDir);
  const [data, setData] = useState<References | null>(null);
  const [error, setError] = useState('');
  const generation = useRef(0);
  const load = useCallback(async () => {
    const request = ++generation.current;
    const qs = new URLSearchParams({ project_dir: project, kind, id, scope, context });
    try {
      const response = await api.get<References>('/api/references?' + qs);
      if (request === generation.current) { setData(response); setError(''); }
    } catch (e) { if (request === generation.current) setError((e as Error).message); }
  }, [project, kind, id, scope, context]);
  const dataScope = JSON.stringify([project, kind, id, scope, context]);
  const [loadedScope, setLoadedScope] = useState(dataScope);
  if (loadedScope !== dataScope) { setLoadedScope(dataScope); setData(null); setError(''); }
  useEffect(() => {
    let active = true;
    queueMicrotask(() => { if (active) void load(); });
    return () => { active = false; generation.current++; };
  }, [load]);
  usePageRefresh(load);
  return <WorkSection title="Record links" description="Persisted relationships in the current project and your knowledge contexts.">
    {error ? <WorkNotice title="References unavailable" tone="error">{error}</WorkNotice> : !data ? <p role="status">Resolving references…</p> : <>
      {!data.complete && <WorkNotice title="Partial reference view">{data.warnings.join(' ')}</WorkNotice>}
      {(['outgoing', 'incoming'] as const).map(direction => <div key={direction} className="record-references">
        <h4>{direction === 'outgoing' ? 'References' : 'Referenced by'}</h4>
        {data[direction].map((edge, i) => {
          const target = direction === 'outgoing' ? edge.target : edge.source;
          const content = <span><strong>{target.title}</strong><small>{target.kind} · {target.id} · {edge.relation}{!target.href && ' · Target unavailable in this context'}</small></span>;
          return target.href ? <Link className="record-link" to={target.href} key={target.href + edge.relation + i}>{content}</Link> : <div className="record-link" key={target.id + edge.relation + i}>{content}</div>;
        })}
        {!data[direction].length && <p>{data.complete ? 'No linked records.' : 'No links found in available sources.'}</p>}
      </div>)}
    </>}
  </WorkSection>;
}
