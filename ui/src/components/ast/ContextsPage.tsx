import { useTaskWorkspace, taskContextKey } from '@/components/task/taskWorkspaceState'
import { encodeProjectLocator, projectReadModule, requestedRecordSelection, importedAstHref } from '@/api/nativeRecordNavigation'
import { stringifyNativeJson } from '@/api/nativeJson'
import { usePageRefresh } from '@/components/layout/WorkspaceRefresh'
import { useMemo, useState } from 'react'
import { useNavigate, useLocation } from 'react-router-dom'
import type { AstInstalledContext } from '@/api/nativeAst'
import type { TaskContext } from '@/api/task'
import { LoadingSpinner } from '@/components/shared/LoadingSpinner'
import { WorkPage, WorkHeader, WorkSection, WorkSearch, WorkEmpty, WorkNotice, FactList } from '@/components/shared/EngineeringUI'
import { useAstImports } from './useAstImports'

type CatalogueEntry = { key: string; name: string; kind: 'project'; project: TaskContext } | { key: string; name: string; kind: 'import'; source: AstInstalledContext }
export default function ContextsPage() {
  const navigate = useNavigate(), location = useLocation(), workspace = useTaskWorkspace()
  const routeBinding = useMemo(() => {
    try { return { selection: requestedRecordSelection(location.search), error: '' } }
    catch (cause) { return { selection: null, error: cause instanceof Error ? cause.message : 'The working context is invalid.' } }
  }, [location.search])
  const nativeActive = projectReadModule(location.pathname, location.search) === 'ast'
  const nativeContexts = nativeActive && workspace.module === 'ast' && workspace.phase === 'ready' ? workspace.contexts : []
  const installed = useAstImports(!routeBinding.error)
  const [query, setQuery] = useState(''), [selected, setSelected] = useState('')
  const available: CatalogueEntry[] = [
    ...nativeContexts.map(project => ({ key: 'project:' + taskContextKey(project), name: project.project_id + ' · ' + project.project_version, kind: 'project' as const, project })),
    ...installed.items.map(source => ({ key: 'import:' + stringifyNativeJson([source.id, source.version]), name: source.id + ' · ' + source.version, kind: 'import' as const, source })),
  ]
  const visible = available.filter(entry => (entry.name + ' ' + (entry.kind === 'project' ? entry.project.project_dir || '' : entry.source.context)).toLowerCase().includes(query.toLowerCase()))
  const current = available.find(entry => entry.key === selected)
  usePageRefresh(installed.refresh)
  if (routeBinding.error) return <WorkNotice tone="error" title="Working context unavailable">{routeBinding.error}</WorkNotice>
  const investigate = (entry: CatalogueEntry) => {
    if (entry.kind === 'import') return navigate(importedAstHref(entry.source))
    workspace.select(taskContextKey(entry.project))
    navigate('/ast/explorer/project?' + new URLSearchParams({ selection: encodeProjectLocator(entry.project) }))
  }
  return <WorkPage>
    <WorkHeader title="Indexed code contexts" description="Choose the source boundary for an investigation. Project and imported indexes remain distinct." />
    <div className="work-toolbar"><WorkSearch label="Find an AST context" value={query} onChange={setQuery} /><span className="text-xs text-muted-foreground">{available.length} available contexts</span></div>
    {installed.error && <WorkNotice tone="error" title="Contexts unavailable">{installed.error}<button className="work-button" onClick={() => void installed.refresh()}>Retry catalogue</button></WorkNotice>}
    {installed.loading && !available.length ? <LoadingSpinner label="Loading contexts…" /> : !available.length && !installed.error ? <WorkEmpty title="No contexts available">Index a project or install an AST artifact to investigate it.</WorkEmpty> : <div className="work-split">
      <section><div className="work-table-wrap"><table className="work-table"><thead><tr><th>Context</th><th>Source</th><th>Version</th></tr></thead><tbody>{visible.map(entry => <tr key={entry.key} className={selected === entry.key ? 'selected' : ''}><td><button className="record-title" onClick={() => setSelected(entry.key)}>{entry.name}</button><small>{entry.kind === 'project' ? 'Project index' : 'Installed index'}</small></td><td>{entry.kind === 'project' ? entry.project.project_dir || 'Native path' : entry.source.backend}</td><td>{entry.kind === 'project' ? entry.project.project_version : entry.source.version}</td></tr>)}</tbody></table></div>{!visible.length && available.length > 0 && <WorkEmpty title="No matching contexts">Try another name or source.</WorkEmpty>}</section>
      <aside className="work-panel">{current ? <><WorkSection title={current.name} description={current.kind === 'project' ? 'The indexed implementation of this project.' : 'An installed code index with its own read admission.'}><FactList items={current.kind === 'project' ? [['Project ID', current.project.project_id], ['Version', current.project.project_version], ['Source', current.project.project_dir]] : [['Artifact ID', current.source.id], ['Version', current.source.version], ['Exact context', current.source.context], ['Backend', current.source.backend]]} /></WorkSection><div className="work-actions"><button className="work-button primary" onClick={() => investigate(current)}>Investigate this context</button></div></> : <WorkEmpty title="Choose a source boundary">Select a context to inspect its origin and open an investigation.</WorkEmpty>}</aside>
    </div>}
  </WorkPage>
}
