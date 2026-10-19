import { useEffect, useId, useRef, useState, type ReactNode } from 'react';
import { getLiveKnowledgePage, LiveError, type LiveArtifactSelection, type LiveKnowledgeCandidate, type LiveKnowledgePage, type LiveKnowledgeReference, type LiveKnowledgeReferences } from '@/api/live';
import { artifactKey } from '@/api/liveCatalog';
import { WikiMarkdown } from '@/components/wiki/WikiMarkdown';
import { compactWikiLabel } from '@/lib/utils';
import { WorkNotice, WorkSection, WorkTabs } from '@/components/shared/EngineeringUI';

interface SourceTab {
  id: string; target: string; context?: LiveArtifactSelection; aliases: string[];
  reference?: LiveKnowledgeReference;
  title: string; loading: boolean; page?: LiveKnowledgePage; candidates?: LiveKnowledgeCandidate[]; error?: string;
}
const referenceKey = (reference: LiveKnowledgeReference) => {
  const { scope, context } = reference.namespace;
  return ['project' in scope ? ['project',scope.project] : ['user',scope.user], context, reference.witness];
};
const sourceKey = (context: LiveArtifactSelection | undefined, target: string, reference?: LiveKnowledgeReference) => JSON.stringify([context ? artifactKey(context) : null, target, reference ? referenceKey(reference) : null]);
const contextLabel = (context: LiveArtifactSelection) => [context.source, context.project_id, context.instance_id, context.project_kind, context.type, context.id, context.version].filter(value => value !== null).join(' · ');

/** Own source readers at the investigation boundary, independently of answer turns. */
export function LiveEvidence({ sessionId, generation, value, onChange, output, activity }: {
  sessionId: string; generation: string; value: string; onChange: (value: string) => void;
  output: (onLink: (target: string) => void) => ReactNode; activity: ReactNode;
}) {
  const [tabs, setTabs] = useState<SourceTab[]>([]);
  const idPrefix = useId();
  const tabsRef = useRef<SourceTab[]>([]);
  const requests = useRef(new Map<string, AbortController>());
  const nextId = useRef(0);
  const active = useRef(value); active.current = value;
  const container = useRef<HTMLDivElement>(null);
  const selectedTitle = tabs.find(tab => tab.id === value)?.title;
  useEffect(() => {
    const tab = container.current?.querySelector<HTMLButtonElement>('[role="tab"][aria-selected="true"]');
    const row = tab?.closest<HTMLElement>('[role="tablist"]');
    if (!tab || !row) return;
    const bounds = (tab.closest('.work-tab-closable') || tab).getBoundingClientRect(), viewport = row.getBoundingClientRect();
    if (bounds.right > viewport.right) row.scrollLeft += bounds.right - viewport.right;
    else if (bounds.left < viewport.left) row.scrollLeft += bounds.left - viewport.left;
  }, [value, selectedTitle]);
  useEffect(() => {
    if (value.startsWith('source-')) container.current?.querySelector<HTMLButtonElement>('[role="tab"][aria-selected="true"]')?.focus();
  }, [value]);
  const update = (fn: (current: SourceTab[]) => SourceTab[]) => {
    tabsRef.current = fn(tabsRef.current);
    setTabs(tabsRef.current);
  };
  useEffect(() => {
    update(() => []);
    onChange('output');
    return () => {
      requests.current.forEach(controller => controller.abort());
      requests.current.clear();
    };
  }, [sessionId, generation]); // Session generation owns all readers; callbacks do not change that boundary.

  const load = async (tab: SourceTab) => {
    requests.current.get(tab.id)?.abort();
    const controller = new AbortController();
    requests.current.set(tab.id, controller);
    update(current => current.map(item => item.id === tab.id ? { ...item, loading: true, error: undefined, candidates: undefined } : item));
    try {
      const result = await getLiveKnowledgePage(sessionId, generation, tab.target, {...(tab.context ? {context:tab.context} : {}),...(tab.reference ? {reference:tab.reference} : {})}, controller.signal);
      if (controller.signal.aborted || requests.current.get(tab.id) !== controller) return;
      if ('candidates' in result) {
        update(current => current.map(item => item.id === tab.id ? { ...item, loading: false, candidates: result.candidates } : item));
      } else {
        const canonical = sourceKey(result.context, result.path, tab.reference);
        const existing = tabsRef.current.find(item => item.id !== tab.id && item.page && sourceKey(item.page.context, item.page.path, item.reference) === canonical);
        if (existing) {
          update(current => current.filter(item => item.id !== tab.id).map(item => item.id === existing.id ? { ...item, target:tab.target, context:tab.context, reference:tab.reference, page:result, title:result.title || compactWikiLabel(result.path), loading:false, error:undefined, candidates:undefined, aliases: [...new Set([...item.aliases, ...tab.aliases, canonical])] } : item));
          if (active.current === tab.id) onChange(existing.id);
        } else {
          update(current => current.map(item => item.id === tab.id ? { ...item, page: result, title: result.title || compactWikiLabel(result.path), loading: false, aliases: [...new Set([...item.aliases, canonical])] } : item));
        }
      }
    } catch (error) {
      if (!controller.signal.aborted && requests.current.get(tab.id) === controller) {
        const message = error instanceof Error ? error.message : String(error);
        update(current => current.map(item => item.id === tab.id ? { ...item, loading: false, error: message + (error instanceof LiveError && error.journalEffect ? ' · History effect: ' + error.journalEffect : '') } : item));
      }
    } finally {
      if (requests.current.get(tab.id) === controller) requests.current.delete(tab.id);
    }
  };
  const open = (target: string, context?: LiveArtifactSelection, replaceId?: string, reference?: LiveKnowledgeReference) => {
    const key = sourceKey(context, target, reference);
    const existing = tabsRef.current.find(item => item.aliases.includes(key));
    if (existing) {
      if (replaceId && replaceId !== existing.id) close(replaceId, false);
      onChange(existing.id);
      return;
    }
    const tab: SourceTab = { id: replaceId || `source-${++nextId.current}`, target, context, reference, title: compactWikiLabel(target), aliases: [key], loading: true };
    update(current => replaceId ? current.map(item => item.id === replaceId ? tab : item) : [...current, tab]);
    onChange(tab.id);
    void load(tab);
  };
  const close = (id: string, restoreFocus = true) => {
    requests.current.get(id)?.abort();
    requests.current.delete(id);
    update(current => current.filter(item => item.id !== id));
    if (active.current === id && restoreFocus) {
      onChange('output');
      container.current?.querySelector<HTMLButtonElement>('[role="tab"]')?.focus();
    } else if (restoreFocus) {
      container.current?.querySelector<HTMLButtonElement>('[role="tab"][aria-selected="true"]')?.focus();
    }
  };
  const title = (tab: SourceTab) => tabs.some(other => other.id !== tab.id && other.title === tab.title)
    ? `${tab.title} · ${tab.page ? contextLabel(tab.page.context) : tab.context ? contextLabel(tab.context) : tab.target}` : tab.title;

  return <div ref={container} className="live-evidence">
    <div className="live-evidence-tabs">
      <WorkTabs label="Run evidence" idPrefix={idPrefix} value={value} onChange={onChange} onClose={close} closableIds={tabs.map(tab => tab.id)} items={[
        ['output', 'Agent output'], ['activity', 'Execution activity'], ...tabs.map(tab => [tab.id, title(tab)] as [string, string]),
      ]} />
    </div>
    <div role="tabpanel" id={`${idPrefix}-panel-output`} aria-labelledby={`${idPrefix}-tab-output`} hidden={value !== 'output'} className="live-evidence-panel">{output(target => open(target))}</div>
    <div role="tabpanel" id={`${idPrefix}-panel-activity`} aria-labelledby={`${idPrefix}-tab-activity`} hidden={value !== 'activity'} className="live-evidence-panel">{activity}</div>
    {tabs.map(tab => <section key={tab.id} role="tabpanel" id={`${idPrefix}-panel-${tab.id}`} aria-labelledby={`${idPrefix}-tab-${tab.id}`} hidden={value !== tab.id} className="live-evidence-panel live-source-reader">
      <header className="live-source-header">
        <div>{!tab.page && <h2>{tab.title}</h2>}
          {tab.page && <dl className="live-source-origin">
            <div><dt>Origin</dt><dd>{tab.page.context.source}</dd></div>
            <div><dt>Artifact</dt><dd>{tab.page.context.id}</dd></div>
            <div><dt>Version</dt><dd>{tab.page.context.version}</dd></div>
            {tab.page.context.project_id !== null && <div><dt>Project</dt><dd>{tab.page.context.project_id}</dd></div>}
            {tab.page.context.instance_id !== null && <div><dt>Instance</dt><dd>{tab.page.context.instance_id}</dd></div>}
            {tab.page.context.project_kind !== null && <div><dt>Project kind</dt><dd>{tab.page.context.project_kind}</dd></div>}
            <div><dt>Page</dt><dd>{tab.page.path}</dd></div>
            <div><dt>Source path</dt><dd>{tab.page.source_path}</dd></div>
          </dl>}
        </div>
      </header>
      {tab.loading ? <p role="status">Opening source…</p> : tab.error ? <WorkNotice title="Source unavailable" tone="error">
        {tab.error}<button className="work-button" onClick={() => void load(tab)}>Try again</button>
      </WorkNotice> : tab.candidates ? <>
        <p>Choose the source context for this page.</p>
        {tab.candidates.length ? tab.candidates.map(candidate => <button className="record-link" key={sourceKey(candidate.context, candidate.path, tab.reference)} onClick={() => open(candidate.path, candidate.context, tab.id, tab.reference)}>{candidate.title} · {contextLabel(candidate.context)}</button>) : <p>No matching source is available in this investigation.</p>}
      </> : tab.page && <><WikiMarkdown content={tab.page.content} onLink={target => open(target, tab.page!.context)} compactWikiLinks /><PageReferences refs={tab.page.refs} onLink={(target, reference) => open(target, undefined, undefined, reference)} /></>}
    </section>)}
  </div>;
}

function PageReferences({ refs, onLink }: { refs: LiveKnowledgeReferences; onLink: (path: string, reference: LiveKnowledgeReference) => void }) {
  const { data } = refs;
  const partial = data.warnings.length > 0 || [data.outgoing, data.incoming].some(page => page && (!page.complete || page.next_cursor !== null));
  return <WorkSection title="Page references">
    {partial && <WorkNotice title="Partial reference view">Reference information is incomplete. The available references are shown below.</WorkNotice>}
    {(['outgoing', 'incoming'] as const).map(direction => {
      const page = data[direction];
      return <div key={direction} className="record-references">
        <h3>{direction === 'outgoing' ? 'References' : 'Referenced by'}</h3>
        {!page ? <p>This reference direction is unavailable in this page.</p> : <>
          <p>{String(page.total)} references. {page.complete ? 'Projection complete.' : 'Projection incomplete.'}{page.next_cursor !== null && ' More pages are available.'}</p>
          {page.results.map((edge, index) => {
            const payload = 'authored' in edge.payload ? edge.payload.authored : edge.payload.wiki_link;
            const destination = direction === 'outgoing' ? payload.target : {target_kind:'knowledge',namespace:edge.source.namespace,id:edge.source.slug};
            const namespace = destination.namespace;
            const label = direction === 'incoming' ? edge.source.title || compactWikiLabel(edge.source.slug) : edge.target_observation === 'unresolved' ? compactWikiLabel(payload.target.id) : edge.target_observation.knowledge_head.title || compactWikiLabel(payload.target.id);
            const description = <span><strong>{label}</strong><small>{edge.provenance}{'relation' in payload ? ' · ' + payload.relation : ''}</small></span>;
            return destination.target_kind === 'knowledge' && namespace
              ? <button key={index} className="record-link" onClick={() => onLink(destination.id, {namespace,witness:direction === 'incoming' ? edge.source.witness : null})}>{description}</button>
              : <div key={index} className="record-link">{description}<small>This reference has no Knowledge page destination.</small></div>;
          })}
          {!page.results.length && <p>{page.complete && page.next_cursor === null ? 'No references in this direction.' : 'No rows returned; the reference view is incomplete.'}</p>}
        </>}
      </div>;
    })}
  </WorkSection>;
}
