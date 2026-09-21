import { useEffect, useRef, useState } from 'react';
import { api } from '@/api/client';
import { WikiMarkdown } from '@/components/wiki/WikiMarkdown';
import { ModalPortal } from '@/components/shared/ModalPortal';
import { WorkNotice } from '@/components/shared/EngineeringUI';
interface Page { title: string; path: string; content: string; context: string; artifact_id?: string; version?: string }
export function LiveAnswer({ sessionId, content }: { sessionId: string; content: string }) {
  const [page, setPage] = useState<Page | null>(null), [candidates, setCandidates] = useState<Page[]>([]);
  const [open, setOpen] = useState(false), [loading, setLoading] = useState(false), [error, setError] = useState('');
  const request = useRef(0), abort = useRef<AbortController | null>(null);
  useEffect(() => { setOpen(false); setPage(null); return () => { request.current++; abort.current?.abort(); }; }, [sessionId]);
  const close = () => { request.current++; abort.current?.abort(); setOpen(false); };
  const read = async (target: string, context = '') => {
    const id = ++request.current; abort.current?.abort(); const controller = new AbortController(); abort.current = controller;
    setOpen(true); setLoading(true); setError(''); setCandidates([]);
    try {
      const result = await api.get<Page | { candidates: Page[] }>(`/api/live/sessions/${encodeURIComponent(sessionId)}/knowledge/page?` + new URLSearchParams({ page: target, context }), { signal: controller.signal });
      if (id !== request.current || controller.signal.aborted) return;
      if ('candidates' in result) { setCandidates(result.candidates); setPage(null); } else setPage(result);
    } catch (e) { if (id === request.current && !controller.signal.aborted) setError((e as Error).message); }
    finally { if (id === request.current) setLoading(false); }
  };
  return <>
    <WikiMarkdown content={content} onLink={target => void read(target)} />
    {open && <ModalPortal onClose={close}><div className="fixed inset-0 z-[100] bg-black/50 flex items-center justify-center p-6"><section role="dialog" aria-modal="true" aria-label="Investigation source" className="bg-background border border-border rounded-lg w-full max-w-5xl max-h-[90vh] overflow-auto p-6">
      <div className="work-actions justify-between"><h2>{page?.title || 'Investigation source'}</h2><button className="work-button" onClick={close}>Close source</button></div>
      {loading ? <p role="status">Opening source…</p> : error ? <WorkNotice title="Source unavailable" tone="error">{error}</WorkNotice> : candidates.length ? <><p>Choose the source context for this page.</p>{candidates.map(candidate => <button className="record-link" key={candidate.context} onClick={() => void read(candidate.path, candidate.context)}>{candidate.context} · {candidate.title}</button>)}</> : page && <>
        <p className="text-sm text-muted-foreground my-4">{page.context}{page.artifact_id && ` · ${page.artifact_id}`}{page.version && ` @ ${page.version}`}</p>
        <WikiMarkdown content={page.content} onLink={target => void read(target, page.context)} />
      </>}
    </section></div></ModalPortal>}
  </>;
}
