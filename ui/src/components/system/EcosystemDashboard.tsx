import { MarkdownContent } from '@/components/wiki/WikiMarkdown'
import { StyledSelect } from '@/components/shared/StyledSelect'
import {
  FactList, WorkBadge, WorkEmpty, WorkHeader, WorkPage, WorkSearch, WorkSection, WorkTabs,
} from '@/components/shared/EngineeringUI'
import { LoadingSpinner } from '@/components/shared/LoadingSpinner'
import { useState } from 'react'
import { useAppStore, type LogicalProject } from '@/store/appStore'
import { hubApi, type GlobalProject } from '@/api/hub'
import { showToast } from '@/hooks/useToast'

function projectTargetKey(project: LogicalProject, origin: 'workspace' | 'hub') {
  return origin === 'hub'
    ? `hub:${project.id}`
    : `workspace:${project.id}:${project.workspace?.dir ?? ''}`
}

export default function EcosystemDashboard() {
  const {
    projectCatalog, activeProjectId, activeProjectOrigin, loadProjects, projectsLoaded,
    switchProjectTarget, workspaceProjectsError, hubProjectsError,
  } = useAppStore()
  const [selectedId, setSelectedId] = useState('')
  const [loading, setLoading] = useState(false)
  const [searchQuery, setSearchQuery] = useState('')
  const [copied, setCopied] = useState(false)
  const [activeAddLabelProject, setActiveAddLabelProject] = useState<string | null>(null)
  const [newLabelKey, setNewLabelKey] = useState('')
  const [newLabelValue, setNewLabelValue] = useState('')
  const [scopeFilter, setScopeFilter] = useState<'all' | 'siblings'>('all')
  const [labelKeyFilter, setLabelKeyFilter] = useState('')

  const activeProject = projectCatalog.find(project => project.id === activeProjectId)
  const inspected = projectCatalog.find(project => project.id === selectedId) || activeProject
  const activeLabelKeys = Object.keys(activeProject?.cluster ?? {})
  const effectiveLabelKeyFilter = activeLabelKeys.includes(labelKeyFilter) ? labelKeyFilter : ''

  const copyPath = async (path: string) => {
    await navigator.clipboard.writeText(path)
    setCopied(true)
    showToast('Path copied to clipboard', 'success')
    window.setTimeout(() => setCopied(false), 2000)
  }

  const unregister = async (project: GlobalProject) => {
    if (!window.confirm(`Unregister "${project.name}" from this workspace? Source files stay on disk.`)) return
    setLoading(true)
    try {
      const result = await hubApi.unregisterProject(project.id, project.dir)
      if (!result.success) throw new Error(result.error || 'Failed to unregister project')
      showToast(`Unregistered ${project.name}`, 'success')
      setSelectedId('')
      await loadProjects()
    } catch (error) {
      showToast(error instanceof Error ? error.message : 'Error unregistering project', 'error')
    } finally {
      setLoading(false)
    }
  }

  const addClusterLabel = async (project: GlobalProject) => {
    if (!newLabelKey.trim() || !newLabelValue.trim()) {
      showToast('Key and value are required', 'info')
      return
    }
    const result = await hubApi.setClusterLabel(project.id, project.dir, newLabelKey.trim(), newLabelValue.trim())
    if (!result.success) {
      showToast(result.error || 'Failed to add cluster label', 'error')
      return
    }
    setNewLabelKey('')
    setNewLabelValue('')
    setActiveAddLabelProject(null)
    showToast('Cluster label saved to the project', 'success')
    await loadProjects()
  }

  const removeClusterLabel = async (project: GlobalProject, key: string) => {
    if (!window.confirm(`Remove cluster label "${key}"?`)) return
    const result = await hubApi.unsetClusterLabel(project.id, project.dir, key)
    if (!result.success) {
      showToast(result.error || 'Failed to remove cluster label', 'error')
      return
    }
    showToast(`Removed label "${key}"`, 'success')
    await loadProjects()
  }

  const filteredProjects = projectCatalog.filter(project => {
    const query = searchQuery.trim().toLowerCase()
    if (query && ![
      project.name, project.id, project.workspace?.dir,
      ...Object.entries(project.cluster).flatMap(([key, values]) => [key, ...values]),
    ].some(value => value?.toLowerCase().includes(query))) return false
    if (scopeFilter !== 'siblings' || !activeProject) return true
    const activeCluster = activeProject.cluster
    const candidateCluster = project.cluster
    if (effectiveLabelKeyFilter) {
      return (activeCluster[effectiveLabelKeyFilter] ?? []).some(value => candidateCluster[effectiveLabelKeyFilter]?.includes(value))
    }
    const activeEntries = Object.entries(activeCluster)
    if (!activeEntries.length) return !Object.keys(candidateCluster).length
    return activeEntries.some(([key, values]) => values.some(value => candidateCluster[key]?.includes(value)))
  })

  return <WorkPage>
    <WorkHeader
      title="Project ecosystem"
      description="Choose one project identity, then decide whether its working context comes from this workspace or the Hub."
    />
    <WorkTabs label="Project scope" value={scopeFilter} onChange={(id) => {
      setScopeFilter(id as 'all' | 'siblings')
      setLabelKeyFilter('')
    }} items={[["all", "All Projects"], ["siblings", "Same Cluster"]]} />

    {(workspaceProjectsError || hubProjectsError) && <div className="work-notice" role="status">
      <strong>Partial project catalogue</strong>
      <span>{workspaceProjectsError || `Workspace is available. ${hubProjectsError}`}</span>
    </div>}

    <div className="work-toolbar">
      <WorkSearch label="Search projects, IDs, paths or labels" value={searchQuery} onChange={setSearchQuery} />
      {scopeFilter === 'siblings' && <StyledSelect aria-label="Cluster label key" value={effectiveLabelKeyFilter} onChange={event => setLabelKeyFilter(event.target.value)}>
        <option value="">Any shared label</option>
        {activeLabelKeys.map(key => <option key={key}>{key}</option>)}
      </StyledSelect>}
      <small>{filteredProjects.length} project identities</small>
    </div>

    {!projectsLoaded || loading ? <LoadingSpinner label="Loading projects…" /> : <div className="ecosystem-layout">
      <section className="work-table-wrap" aria-label="Project directory">
        <table className="work-table">
          <thead><tr><th>Project</th><th>Cluster labels</th><th>Presence</th><th>Context</th></tr></thead>
          <tbody>{filteredProjects.map(project => <tr key={project.id} className={inspected?.id === project.id ? 'selected' : ''}>
            <td><button className="record-title" onClick={() => { setSelectedId(project.id); setActiveAddLabelProject(null) }}>{project.name}</button><small>{project.id}</small></td>
            <td>{Object.entries(project.cluster).length ? Object.entries(project.cluster).map(([key, values]) => <small key={key}>{key}: {values.join(', ')}</small>) : <small>No labels</small>}</td>
            <td><div className="project-presence">
              {project.workspace && <WorkBadge>Workspace</WorkBadge>}
              {project.hub && <WorkBadge tone="info">Hub</WorkBadge>}
            </div></td>
            <td>{project.id === activeProjectId ? <span>{activeProjectOrigin === 'hub' ? 'Active · Hub' : 'Active · workspace'}</span> : <span>Available</span>}</td>
          </tr>)}</tbody>
        </table>
        {!filteredProjects.length && <WorkEmpty title={projectCatalog.length ? 'No projects match' : 'No projects available'}>
          {projectCatalog.length ? 'Try another label or search term.' : 'Refresh when a workspace or Hub project becomes available.'}
        </WorkEmpty>}
      </section>

      <aside className="work-panel" aria-label="Project dossier">
        {inspected ? <>
          <WorkSection title={inspected.name}>
            <div className="project-presence mb-4">
              {inspected.workspace && <WorkBadge>Workspace</WorkBadge>}
              {inspected.hub && <WorkBadge tone="info">Hub · remote</WorkBadge>}
              {inspected.id === activeProjectId && <WorkBadge tone="success">Selected</WorkBadge>}
            </div>
            {inspected.description ? <div className="markdown-preview"><MarkdownContent content={inspected.description} /></div> : <p>A Graphit project identity available as a working context.</p>}
            <FactList items={[
              ['Project ID', inspected.id],
              ['Workspace', inspected.workspace ? inspected.workspace.dir : 'Not present on this machine'],
              ['Hub', inspected.hub ? `Published · revision ${inspected.hub.revision}` : 'Not published or not visible'],
            ]} />
            <div className="work-actions mt-5">
              {inspected.workspace && <button className="work-button primary" onClick={() => switchProjectTarget(projectTargetKey(inspected, 'workspace'))}>Use workspace</button>}
              {inspected.hub && <button className="work-button primary" onClick={() => switchProjectTarget(projectTargetKey(inspected, 'hub'))}>Use Hub context</button>}
            </div>
          </WorkSection>

          {inspected.hub && <WorkSection title="Remote context" description="The selected Hub target follows this identity across Operations, Engineering and Project Contexts.">
            <FactList items={[
              ['Task', 'Live project store'], ['Memory', 'Live project store'],
              ['Knowledge', 'Published artifact · selectable version'], ['AST', 'Published artifact · selectable version'],
            ]} />
            <p className="text-xs text-muted-foreground mt-4">Knowledge and AST resolve latest to an exact <code>id@version</code> before reading.</p>
          </WorkSection>}

          <WorkSection title="Cluster relationships" description={inspected.workspace ? 'Labels are saved in the project lock and synchronized to Hub metadata.' : 'Remote labels come from Hub project metadata.'}>
            {Object.entries(inspected.cluster).map(([key, values]) => <div className="cluster-label-row" key={key}>
              <span><strong>{key}</strong><small>{values.join(', ')}</small></span>
              {inspected.workspace && <button className="work-button danger" aria-label={`Remove label ${key}`} onClick={() => void removeClusterLabel(inspected.workspace!, key)}>Remove</button>}
            </div>)}
            {!Object.keys(inspected.cluster).length && <p className="text-xs text-muted-foreground">No cluster labels configured.</p>}
            {inspected.workspace && (activeAddLabelProject === inspected.id ? <form className="work-form mt-4" onSubmit={event => { event.preventDefault(); void addClusterLabel(inspected.workspace!) }}>
              <label className="work-field"><span>Label key</span><input value={newLabelKey} onChange={event => setNewLabelKey(event.target.value)} autoFocus /></label>
              <label className="work-field"><span>Label value</span><input value={newLabelValue} onChange={event => setNewLabelValue(event.target.value)} /></label>
              <div className="work-actions"><button className="work-button primary">Save label</button><button type="button" className="work-button" onClick={() => setActiveAddLabelProject(null)}>Cancel</button></div>
            </form> : <button className="work-button mt-4" onClick={() => setActiveAddLabelProject(inspected.id)}>Add cluster label</button>)}
          </WorkSection>

          {inspected.workspace && <details className="work-disclosure"><summary>Workspace registration</summary><div>
            <p className="text-xs text-muted-foreground mb-4">Copy its checkout path or unregister this local presence. Hub metadata and source files are not deleted.</p>
            <div className="work-actions"><button className="work-button" onClick={() => void copyPath(inspected.workspace!.dir)}>{copied ? 'Copied' : 'Copy path'}</button><button className="work-button danger" onClick={() => void unregister(inspected.workspace!)}>Unregister workspace</button></div>
          </div></details>}
        </> : <WorkEmpty title="Inspect a project">Choose a project to see its identity, presences and working-context options.</WorkEmpty>}
      </aside>
    </div>}
  </WorkPage>
}
