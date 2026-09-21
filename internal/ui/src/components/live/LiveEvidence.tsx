import { useEffect, useId, useRef, useState, type ReactNode } from 'react';
import { api } from '@/api/client';
import { WikiMarkdown, compactWikiLabel } from '@/components/wiki/WikiMarkdown';
import { WorkNotice, WorkTabs } from '@/components/shared/EngineeringUI';

interface Page {
  title: string; path: string; content: string; context: string;
  artifact_id?: string; version?: string;
}
interface SourceTab {
  id: string; target: string; context: string; aliases: string[];
  title: string; loading: boolean; page?: Page; candidates?: Page[]; error?: string;
}
const sourceKey = (context: string, target: string) => JSON.stringify([context, target]);

/** Own source readers at the investigation boundary, independently of answer turns. */
export function LiveEvidence({ sessionId, value, onChange, output, activity }: {
  sessionId: string; value: string; onChange: (value: string) => void;
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
  }, [sessionId]); // Session owns all readers; callbacks do not change that boundary.

  const load = async (tab: SourceTab) => {
    requests.current.get(tab.id)?.abort();
    const controller = new AbortController();
    requests.current.set(tab.id, controller);
    update(current => current.map(item => item.id === tab.id ? { ...item, loading: true, error: undefined, candidates: undefined } : item));
    try {
      const result = await api.get<Page | { candidates: Page[] }>(
        `/api/live/sessions/${encodeURIComponent(sessionId)}/knowledge/page?` + new URLSearchParams({ page: tab.target, context: tab.context }),
        { signal: controller.signal },
      );
      if (controller.signal.aborted || requests.current.get(tab.id) !== controller) return;
      if ('candidates' in result) {
        update(current => current.map(item => item.id === tab.id ? { ...item, loading: false, candidates: result.candidates } : item));
      } else {
        const canonical = sourceKey(result.context, result.path);
        const existing = tabsRef.current.find(item => item.id !== tab.id && item.page && sourceKey(item.page.context, item.page.path) === canonical);
        if (existing) {
          update(current => current.filter(item => item.id !== tab.id).map(item => item.id === existing.id ? { ...item, aliases: [...new Set([...item.aliases, ...tab.aliases, canonical])] } : item));
          if (active.current === tab.id) onChange(existing.id);
        } else {
          update(current => current.map(item => item.id === tab.id ? { ...item, page: result, title: result.title || compactWikiLabel(result.path), loading: false, aliases: [...new Set([...item.aliases, canonical])] } : item));
        }
      }
    } catch (error) {
      if (!controller.signal.aborted && requests.current.get(tab.id) === controller) {
        update(current => current.map(item => item.id === tab.id ? { ...item, loading: false, error: error instanceof Error ? error.message : String(error) } : item));
      }
    } finally {
      if (requests.current.get(tab.id) === controller) requests.current.delete(tab.id);
    }
  };
  const open = (target: string, context = '', replaceId?: string) => {
    const key = sourceKey(context, target);
    const existing = tabsRef.current.find(item => item.aliases.includes(key));
    if (existing) {
      if (replaceId && replaceId !== existing.id) close(replaceId, false);
      onChange(existing.id);
      return;
    }
    const tab: SourceTab = { id: replaceId || `source-${++nextId.current}`, target, context, title: compactWikiLabel(target), aliases: [key], loading: true };
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
    ? `${tab.title} · ${tab.page?.context || tab.context || tab.target}` : tab.title;

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
            <div><dt>Context</dt><dd>{tab.page.context}</dd></div>
            {tab.page.artifact_id && tab.page.artifact_id !== tab.page.context && <div><dt>Artifact</dt><dd>{tab.page.artifact_id}</dd></div>}
            {tab.page.version && <div><dt>Version</dt><dd>{tab.page.version}</dd></div>}
          </dl>}
        </div>
      </header>
      {tab.loading ? <p role="status">Opening source…</p> : tab.error ? <WorkNotice title="Source unavailable" tone="error">
        {tab.error}<button className="work-button" onClick={() => void load(tab)}>Try again</button>
      </WorkNotice> : tab.candidates ? <>
        <p>Choose the source context for this page.</p>
        {tab.candidates.length ? tab.candidates.map(candidate => <button className="record-link" key={sourceKey(candidate.context, candidate.path)} onClick={() => open(candidate.path, candidate.context, tab.id)}>{candidate.title} · {candidate.context}</button>) : <p>No matching source is available in this investigation.</p>}
      </> : tab.page && <WikiMarkdown content={tab.page.content} onLink={target => open(target, tab.page!.context)} compactWikiLinks />}
    </section>)}
  </div>;
}
