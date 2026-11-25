import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import { useLocation, useNavigate } from 'react-router-dom'
import { nativeRecordRead } from '@/api/nativeRecordRead'
import { encodeProjectLocator } from '@/api/nativeRecordNavigation'
import type { TaskContext } from '@/api/task'
import { useTaskWorkspace, taskContextKey } from '@/components/task/taskWorkspaceState'
import { usePageRefresh } from '@/components/layout/WorkspaceRefresh'
import { MarkdownContent } from '@/components/wiki/WikiMarkdown'
import { FactList, WorkBadge, WorkEmpty, WorkHeader, WorkNotice, WorkPage, WorkSearch, WorkSection } from '@/components/shared/EngineeringUI'
import { StyledSelect } from '@/components/shared/StyledSelect'
import { LoadingSpinner } from '@/components/shared/LoadingSpinner'

interface LocalProject {
  project_id: string
  name: string
  description?: string
  cluster?: Record<string, string[]>
  online: boolean
  versions: string[]
  checkouts: { dir: string; exists: boolean | null; online: boolean; current_version: string | null }[]
}
interface ProjectPage { projects: LocalProject[]; next_cursor: string | null }
interface LabelReceipt { status: 'ok' | 'partial' }

function labels(project: LocalProject) {
  return Object.entries(project.cluster ?? {}).flatMap(([key, values]) =>
    values.map(value => ({ key, value, id: JSON.stringify([key, value]) })))
}

export function LocalProjectsDashboard() {
  const { contexts, selection, select } = useTaskWorkspace()
  const navigate = useNavigate(), location = useLocation()
  const [projects, setProjects] = useState<LocalProject[] | null>(null)
  const [inspected, setInspected] = useState('')
  const [query, setQuery] = useState('')
  const [label, setLabel] = useState('')
  const [siblings, setSiblings] = useState(false)
  const [error, setError] = useState('')
  const [loading, setLoading] = useState(true)
  const request = useRef<AbortController | null>(null)
  const refresh = useCallback(async () => {
    request.current?.abort()
    const controller = new AbortController()
    request.current = controller
    setLoading(true); setError('')
    try {
      const rows: LocalProject[] = [], seen = new Set<string>()
      let cursor: string | null = null
      do {
        const page: ProjectPage = await nativeRecordRead('/projects/local/list', cursor === null ? {} : { cursor }, controller.signal)
        if (controller.signal.aborted) return
        rows.push(...page.projects)
        cursor = page.next_cursor
        if (cursor !== null && seen.has(cursor)) throw new Error('The project catalogue repeated its page cursor.')
        if (cursor !== null) seen.add(cursor)
      } while (cursor !== null)
      setProjects(rows)
    } catch (cause) {
      if (!controller.signal.aborted) setError(cause instanceof Error ? cause.message : 'Unable to read local projects.')
    } finally {
      if (!controller.signal.aborted) setLoading(false)
    }
  }, [])
  useEffect(() => { void refresh(); return () => request.current?.abort() }, [refresh])
  usePageRefresh(refresh)
  const active = projects?.find(project => project.project_id === selection?.project_id)
  const selected = projects?.find(project => project.project_id === inspected) ?? active
  const activeLabels = new Set(active ? labels(active).map(entry => entry.id) : [])
  const availableLabels = useMemo(() => [...new Map((projects ?? []).flatMap(labels).map(entry => [entry.id, entry])).values()]
    .sort((a, b) => a.key.localeCompare(b.key) || a.value.localeCompare(b.value)), [projects])
  const effectiveLabel = availableLabels.some(entry => entry.id === label) ? label : ''
  const text = query.trim().toLowerCase()
  const visible = (projects ?? []).filter(project => {
    const projectLabels = labels(project)
    if (effectiveLabel && !projectLabels.some(entry => entry.id === effectiveLabel)) return false
    if (siblings && !projectLabels.some(entry => activeLabels.has(entry.id))) return false
    return !text || [project.project_id, project.name, project.description ?? '', ...project.checkouts.map(checkout => checkout.dir),
      ...projectLabels.flatMap(entry => [entry.key, entry.value, entry.key + '=' + entry.value])].some(value => typeof value === 'string' && value.toLowerCase().includes(text))
  })
  const choose = (context: TaskContext) => {
    select(taskContextKey(context))
    const params = new URLSearchParams(location.search)
    params.set('selection', encodeProjectLocator(context))
    navigate(location.pathname + '?' + params, { replace: true })
  }
  return <WorkPage>
    <WorkHeader title="Project ecosystem" description="Find registered local projects, inspect their labels, and select an active checkout." />
    <div className="work-toolbar">
      <WorkSearch label="Search local projects" value={query} onChange={setQuery} placeholder="Search projects, paths or labels" />
      <StyledSelect aria-label="Filter projects by label" value={effectiveLabel} onChange={event => setLabel(event.target.value)}>
        <option value="">All labels</option>{availableLabels.map(entry => <option key={entry.id} value={entry.id}>{entry.key} = {entry.value}</option>)}
      </StyledSelect>
      <button type="button" className="work-button" aria-pressed={siblings} disabled={!activeLabels.size && !siblings} onClick={() => setSiblings(value => !value)}>Same Cluster</button>
    </div>
    {error && <WorkNotice title="Unable to read local projects" tone="error">{error}<button className="work-button" onClick={() => void refresh()}>Retry project catalogue</button></WorkNotice>}
    {!projects && loading ? <LoadingSpinner label="Loading local projects…" /> : projects && <div className="ecosystem-layout">
      <div className="work-table-wrap" aria-label="Local project directory">
        <table className="work-table"><thead><tr><th>Project</th><th>Labels</th><th>Presence</th></tr></thead>
          <tbody>{visible.map(project => <tr key={project.project_id} className={selected?.project_id === project.project_id ? 'selected' : undefined}>
            <td><button type="button" className="record-title" onClick={() => setInspected(project.project_id)}>{project.name || project.project_id}</button><small><code>{project.project_id}</code></small></td>
            <td>{labels(project).map(entry => <small key={entry.id}>{entry.key} = {entry.value}</small>)}</td>
            <td><WorkBadge tone={project.online ? 'info' : undefined}>{project.online ? 'Online' : 'Registered'}</WorkBadge></td>
          </tr>)}</tbody></table>
        {!visible.length && <WorkEmpty title={projects.length ? 'No projects match' : 'No registered local projects'} />}
      </div>
      <aside className="work-panel">
        {selected ? <>
          <WorkSection title={selected.name || selected.project_id}>
            {selected.description && <MarkdownContent content={selected.description} />}
            <FactList items={[[ 'Project ID', <code>{selected.project_id}</code> ], [ 'Retained versions', selected.versions.join(', ') || 'None' ]]} />
          </WorkSection>
          <WorkSection title="Checkouts">
            {!selected.checkouts.length && <p>No checkout is registered for this project.</p>}
            {selected.checkouts.map(checkout => {
              const available = contexts.filter(context => context.project_id === selected.project_id && context.project_dir === checkout.dir)
              const preferred = available.find(context => context.project_version === checkout.current_version) ?? available[0]
              const current = selection?.project_dir === checkout.dir && selection?.project_id === selected.project_id
              return <div className="cluster-label-row" key={checkout.dir}>
                <span><strong>{checkout.dir}</strong><small>{current ? 'Current checkout' : checkout.exists === false ? 'Directory unavailable' : preferred ? 'Ready to select' : 'Registered · offline'}</small></span>
                {preferred && !current && <button className="work-button" onClick={() => choose(preferred)}>Use checkout</button>}
              </div>
            })}
          </WorkSection>
          <LocalProjectLabels project={selected}
            key={selected.project_id + ':' + taskContextKey(selection)}
            context={selection?.project_id === selected.project_id ? selection : null}
            filter={id => { setLabel(id); setSiblings(false) }} refresh={refresh} />
        </> : <WorkEmpty title="Inspect a project">Select a project to see its labels, versions and checkouts.</WorkEmpty>}
      </aside>
    </div>}
  </WorkPage>
}

function LocalProjectLabels({ project, context, filter, refresh }: {
  project: LocalProject; context: TaskContext | null; filter: (id: string) => void; refresh: () => Promise<void>
}) {
  const [key, setKey] = useState('')
  const [value, setValue] = useState('')
  const [working, setWorking] = useState(false)
  const [message, setMessage] = useState<{ text: string; error: boolean } | null>(null)
  const edit = async (operation: 'set' | 'unset', key: string, value: string) => {
    if (!context || working || !key.trim() || !value.trim()) return
    setWorking(true); setMessage(null)
    try {
      const result = await nativeRecordRead<LabelReceipt>('/projects/local/labels', { ...context, operation, key: key.trim(), value: value.trim() })
      if (operation === 'set') { setKey(''); setValue('') }
      setMessage(result.status === 'partial'
        ? { text: 'Checkout labels were saved. The inventory could not be refreshed.', error: true }
        : { text: operation === 'set' ? 'Label added.' : 'Label removed.', error: false })
      await refresh()
    } catch (error) {
      setMessage({ text: error instanceof Error ? error.message : 'Unable to edit the label.', error: true })
    } finally {
      setWorking(false)
    }
  }
  return <WorkSection title="Project labels">
    {!labels(project).length && <p>No labels are set for this project.</p>}
    {labels(project).map(entry => <div className="cluster-label-row" key={entry.id}>
      <button className="work-button" onClick={() => filter(entry.id)}>{entry.key} = {entry.value}</button>
      {context && <button className="work-button danger" disabled={working} aria-label={'Remove label ' + entry.key + ' = ' + entry.value} onClick={() => {
        if (window.confirm(`Remove ${entry.key} = ${entry.value} from this project?`)) void edit('unset', entry.key, entry.value)
      }}>Remove</button>}
    </div>)}
    {context ? <form className="work-form mt-4" onSubmit={event => { event.preventDefault(); void edit('set', key, value) }}>
      <label className="work-field"><span>Label key</span><input value={key} onChange={event => setKey(event.target.value)} required /></label>
      <label className="work-field"><span>Label value</span><input value={value} onChange={event => setValue(event.target.value)} required /></label>
      <div><button className="work-button primary" disabled={working || !key.trim() || !value.trim()}>{working ? 'Saving…' : 'Add label'}</button></div>
    </form> : <p className="work-help">Use an active checkout of this project to edit its labels.</p>}
    {message && <WorkNotice title={message.error ? 'Project label operation' : 'Project labels updated'} tone={message.error ? 'error' : 'success'}>{message.text}</WorkNotice>}
  </WorkSection>
}
