import { useState } from 'react'
import { useAppStore } from '@/store/appStore'
import { hubApi, type GlobalProject } from '@/api/hub'
import {
  Globe,
  Folder,
  Calendar,
  Tag,
  Plus,
  X,
  Trash2,
  ExternalLink,
  Copy,
  Check,
  Search,
  RefreshCw,
  Info,
} from 'lucide-react'
import { showToast } from '@/hooks/useToast'

export default function EcosystemDashboard() {
  const {
    projects,
    activeProjectDir,
    switchProject,
    loadProjects,
    projectsLoaded,
  } = useAppStore()

  const [loading, setLoading] = useState(false)
  const [searchQuery, setSearchQuery] = useState('')
  const [copiedIndex, setCopiedIndex] = useState<number | null>(null)
  
  // Track which project is currently showing the "add label" inputs
  const [activeAddLabelProject, setActiveAddLabelProject] = useState<string | null>(null)
  const [newLabelKey, setNewLabelKey] = useState('')
  const [newLabelValue, setNewLabelValue] = useState('')

  const [scopeFilter, setScopeFilter] = useState<'all' | 'siblings'>('all')
  const [labelKeyFilter, setLabelKeyFilter] = useState<string>('')

  const activeProject = projects.find((p) => p.dir === activeProjectDir)
  const activeLabelKeys = activeProject?.cluster ? Object.keys(activeProject.cluster) : []

  const effectiveLabelKeyFilter = labelKeyFilter && activeLabelKeys.includes(labelKeyFilter) ? labelKeyFilter : ''

  const handleRefresh = async (silent = false) => {
    if (!silent) setLoading(true)
    try {
      await loadProjects()
    } catch {
      showToast('Failed to refresh projects list', 'error')
    } finally {
      if (!silent) setLoading(false)
    }
  }

  const handleCopyPath = (path: string, index: number) => {
    navigator.clipboard.writeText(path)
    setCopiedIndex(index)
    showToast('Path copied to clipboard', 'success')
    setTimeout(() => setCopiedIndex(null), 2000)
  }

  const handleSwitchProject = (project: GlobalProject) => {
    switchProject(project.dir)
    showToast(`Switched active project to ${project.name}`, 'success')
  }

  const handleUnregisterProject = async (project: GlobalProject) => {
    if (
      !window.confirm(
        `Are you sure you want to unregister "${project.name}"? This removes it from the managed ecosystem list but does not delete files.`
      )
    ) {
      return
    }

    setLoading(true)
    try {
      const res = await hubApi.unregisterProject(project.id, project.dir)
      if (res.success) {
        showToast(`Successfully unregistered ${project.name}`, 'success')
        await loadProjects()
      } else {
        showToast(res.error || 'Failed to unregister project', 'error')
      }
    } catch {
      showToast('Error unregistering project', 'error')
    } finally {
      setLoading(false)
    }
  }

  const handleAddClusterLabel = async (project: GlobalProject) => {
    if (!newLabelKey.trim() || !newLabelValue.trim()) {
      showToast('Key and Value are required', 'info')
      return
    }

    try {
      const res = await hubApi.setClusterLabel(
        project.id,
        project.dir,
        newLabelKey.trim(),
        newLabelValue.trim()
      )
      if (res.success) {
        showToast(`Added label ${newLabelKey}:${newLabelValue}`, 'success')
        setNewLabelKey('')
        setNewLabelValue('')
        setActiveAddLabelProject(null)
        await loadProjects()
      } else {
        showToast(res.error || 'Failed to add cluster label', 'error')
      }
    } catch {
      showToast('Error adding cluster label', 'error')
    }
  }

  const handleRemoveClusterLabel = async (project: GlobalProject, key: string) => {
    if (!window.confirm(`Remove cluster label "${key}"?`)) {
      return
    }

    try {
      const res = await hubApi.unsetClusterLabel(project.id, project.dir, key)
      if (res.success) {
        showToast(`Removed label "${key}"`, 'success')
        await loadProjects()
      } else {
        showToast(res.error || 'Failed to remove cluster label', 'error')
      }
    } catch {
      showToast('Error removing cluster label', 'error')
    }
  }

  const formatDate = (dateStr?: string) => {
    if (!dateStr) return 'N/A'
    return new Date(dateStr).toLocaleDateString(undefined, {
      year: 'numeric',
      month: 'short',
      day: 'numeric',
    })
  }

  const filteredProjects = projects.filter((p) => {
    // 1. Search Query filter (matches Name, Path, or any cluster tag key/value)
    const query = searchQuery.toLowerCase()
    const matchSearch =
      !query ||
      p.name.toLowerCase().includes(query) ||
      p.dir.toLowerCase().includes(query) ||
      Object.entries(p.cluster || {}).some(([k, vals]) =>
        k.toLowerCase().includes(query) || vals.some((v) => v.toLowerCase().includes(query))
      )

    if (!matchSearch) return false

    // 2. Scope Filter (All vs Cluster Siblings)
    if (scopeFilter === 'siblings') {
      if (!activeProject) return true // default to showing all if no active project

      const activeHasLabels = activeProject.cluster && Object.keys(activeProject.cluster).length > 0
      const candidateHasLabels = p.cluster && Object.keys(p.cluster).length > 0

      // If an effectiveLabelKeyFilter is selected, we filter by that specific label key sharing
      if (effectiveLabelKeyFilter) {
        const activeVals = activeProject.cluster?.[effectiveLabelKeyFilter]
        const candidateVals = p.cluster?.[effectiveLabelKeyFilter]
        if (!activeVals || !candidateVals) return false
        const shares = activeVals.some(av => candidateVals.includes(av))
        if (!shares) return false
      } else {
        // Standard cluster sibling resolution
        if (!activeHasLabels) {
          // If active project has no cluster, candidate must also have no cluster (global fallback)
          if (candidateHasLabels) return false
        } else {
          // If active project has cluster, candidate must share at least one value for any key
          if (!candidateHasLabels) return false
          let sharesAny = false
          for (const [key, activeVals] of Object.entries(activeProject.cluster || {})) {
            const candidateVals = p.cluster?.[key]
            if (candidateVals && activeVals.some(av => candidateVals.includes(av))) {
              sharesAny = true
              break
            }
          }
          if (!sharesAny) return false
        }
      }
    }

    return true
  })

  return (
    <div className="w-full max-w-7xl mx-auto px-4 md:px-8 py-10 animate-in fade-in duration-300">
      <div className="flex flex-col md:flex-row md:items-center justify-between gap-4 mb-8 pb-6 border-b border-border/40">
        <div className="flex items-center gap-4">
          <div className="w-12 h-12 rounded-2xl bg-primary/10 border border-primary/20 flex items-center justify-center shrink-0">
            <Globe className="w-6 h-6 text-primary" />
          </div>
          <div>
            <h1 className="text-3xl font-heading font-bold tracking-tight text-foreground">
              Ecosystem
            </h1>
            <p className="text-[14px] text-muted-foreground mt-1">
              Explore and manage local projects in your Graphit development ecosystem
            </p>
          </div>
        </div>

        <div className="flex items-center gap-2.5">
          <div className="relative">
            <Search className="absolute left-3.5 top-1/2 -translate-y-1/2 w-4 h-4 text-muted-foreground/60" />
            <input
              type="text"
              placeholder="Search projects or labels..."
              value={searchQuery}
              onChange={(e) => setSearchQuery(e.target.value)}
              className="pl-10 pr-4 py-2 w-64 rounded-xl border border-border/50 bg-background/50 text-sm focus:outline-none focus:border-primary transition-colors glass-panel"
            />
            {searchQuery && (
              <button
                onClick={() => setSearchQuery('')}
                className="absolute right-3 top-1/2 -translate-y-1/2 text-muted-foreground hover:text-foreground"
              >
                <X className="w-3.5 h-3.5" />
              </button>
            )}
          </div>

          <button
            onClick={() => handleRefresh()}
            className="flex items-center justify-center p-2.5 rounded-xl border border-border/50 hover:bg-accent/50 transition-all hover:scale-[1.02] glass-panel"
            title="Refresh"
            disabled={loading}
          >
            <RefreshCw
              className={`w-4 h-4 text-muted-foreground transition-transform duration-500 ${
                loading ? 'animate-spin' : 'hover:rotate-180'
              }`}
            />
          </button>
        </div>
      </div>

      {}
      {projectsLoaded && !loading && (
        <div className="flex flex-wrap items-center gap-4 mb-6 p-4 rounded-2xl border border-border/30 bg-muted/10 glass-panel">
          <div className="flex items-center gap-2">
            <span className="text-xs font-semibold text-muted-foreground uppercase tracking-wider">Scope:</span>
            <div className="flex rounded-xl bg-background/50 border border-border/50 p-0.5">
              <button
                onClick={() => {
                  setScopeFilter('all')
                  setLabelKeyFilter('')
                }}
                className={`px-3 py-1.5 rounded-lg text-xs font-medium transition-all ${
                  scopeFilter === 'all'
                    ? 'bg-primary text-primary-foreground shadow-sm'
                    : 'text-muted-foreground hover:text-foreground'
                }`}
              >
                All Projects
              </button>
              <button
                onClick={() => setScopeFilter('siblings')}
                className={`px-3 py-1.5 rounded-lg text-xs font-medium transition-all ${
                  scopeFilter === 'siblings'
                    ? 'bg-primary text-primary-foreground shadow-sm'
                    : 'text-muted-foreground hover:text-foreground'
                }`}
              >
                Same Cluster
              </button>
            </div>
          </div>

          {scopeFilter === 'siblings' && activeLabelKeys.length > 0 && (
            <div className="flex items-center gap-2 animate-in fade-in slide-in-from-left-2 duration-200">
              <span className="text-xs font-semibold text-muted-foreground uppercase tracking-wider">Filter by Key:</span>
              <div className="flex flex-wrap gap-1.5">
                <button
                  onClick={() => setLabelKeyFilter('')}
                  className={`px-3 py-1.5 rounded-xl text-xs font-medium transition-all border ${
                    effectiveLabelKeyFilter === ''
                      ? 'bg-accent/80 text-foreground border-accent-foreground/20'
                      : 'border-border/50 text-muted-foreground hover:text-foreground hover:bg-accent/30'
                  }`}
                >
                  All Siblings
                </button>
                {activeLabelKeys.map((key) => (
                  <button
                    key={key}
                    onClick={() => setLabelKeyFilter(key)}
                    className={`px-3 py-1.5 rounded-xl text-xs font-medium transition-all border ${
                      effectiveLabelKeyFilter === key
                        ? 'bg-accent/80 text-foreground border-accent-foreground/20'
                        : 'border-border/50 text-muted-foreground hover:text-foreground hover:bg-accent/30'
                    }`}
                  >
                    {key}
                  </button>
                ))}
              </div>
            </div>
          )}

          {scopeFilter === 'siblings' && !activeProject?.cluster && (
            <span className="text-xs text-muted-foreground/80 italic flex items-center gap-1">
              <Info className="w-3.5 h-3.5 animate-pulse text-primary" /> No cluster tags set on active project (showing global/no-cluster projects)
            </span>
          )}
        </div>
      )}

      {!projectsLoaded || loading ? (
        <div className="flex items-center justify-center min-h-[300px]">
          <div className="flex flex-col items-center gap-3">
            <RefreshCw className="w-8 h-8 text-primary animate-spin" />
            <p className="text-muted-foreground text-sm">Loading ecosystem projects...</p>
          </div>
        </div>
      ) : filteredProjects.length === 0 ? (
        <div className="glass-panel rounded-2xl p-12 text-center max-w-lg mx-auto mt-10">
          <Folder className="w-12 h-12 text-muted-foreground/40 mx-auto mb-4" />
          <h3 className="text-lg font-bold font-heading mb-2">No projects found</h3>
          <p className="text-sm text-muted-foreground leading-relaxed">
            {searchQuery
              ? 'No projects in your ecosystem match your search criteria.'
              : 'There are no projects managed in this ecosystem yet. Run graphit init on a folder to register it.'}
          </p>
        </div>
      ) : (
        <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-6">
          {filteredProjects.map((project, idx) => {
            const isCurrent = project.dir === activeProjectDir
            const labelEntries = Object.entries(project.cluster || {})

            return (
              <div
                key={project.id}
                className={`flex flex-col rounded-2xl border transition-all duration-300 relative overflow-hidden h-full ${
                  isCurrent
                    ? 'border-primary/40 bg-primary/5 shadow-[0_0_20px_rgba(59,130,246,0.08)]'
                    : 'border-border/40 hover:border-border/80 bg-card/40 hover:shadow-lg'
                }`}
              >
                {}
                {isCurrent && (
                  <div className="absolute top-0 right-0 bg-primary text-primary-foreground text-[9px] font-bold px-3 py-1 rounded-bl-xl tracking-wider uppercase shadow-sm">
                    Active
                  </div>
                )}

                {}
                <div className="p-6 flex-1 flex flex-col">
                  <div className="flex items-start gap-3 mb-3 pr-10">
                    <Folder
                      className={`w-5 h-5 mt-0.5 shrink-0 ${
                        isCurrent ? 'text-primary' : 'text-muted-foreground'
                      }`}
                    />
                    <div className="min-w-0">
                      <h3 className="font-heading font-bold text-lg text-foreground truncate">
                        {project.name}
                      </h3>
                      <p className="text-[10px] text-muted-foreground/60 font-mono mt-0.5 select-all truncate">
                        ID: {project.id}
                      </p>
                    </div>
                  </div>

                  <p className="text-sm text-muted-foreground leading-relaxed mb-4 flex-1 line-clamp-2">
                    {project.description || 'No description provided.'}
                  </p>

                  <div className="space-y-2 border-t border-border/30 pt-4 mt-auto">
                    {}
                    <div className="flex items-center justify-between text-xs">
                      <span className="text-muted-foreground flex items-center gap-1.5">
                        <Calendar className="w-3.5 h-3.5 text-muted-foreground/60" /> Registered
                      </span>
                      <span className="font-medium text-foreground/80">
                        {formatDate(project.registered_at)}
                      </span>
                    </div>

                    {}
                    <div className="flex items-center justify-between text-xs gap-3">
                      <span className="text-muted-foreground flex items-center gap-1.5 shrink-0">
                        <Folder className="w-3.5 h-3.5 text-muted-foreground/60" /> Directory
                      </span>
                      <div className="flex items-center gap-1.5 min-w-0">
                        <span
                          className="font-mono text-[10px] truncate text-foreground/60 select-all"
                          title={project.dir}
                        >
                          {project.dir}
                        </span>
                        <button
                          onClick={() => handleCopyPath(project.dir, idx)}
                          className="p-1 rounded hover:bg-accent text-muted-foreground hover:text-foreground transition-colors shrink-0"
                          title="Copy path"
                        >
                          {copiedIndex === idx ? (
                            <Check className="w-3 h-3 text-success" />
                          ) : (
                            <Copy className="w-3 h-3" />
                          )}
                        </button>
                      </div>
                    </div>
                  </div>

                  {}
                  <div className="mt-4 border-t border-border/30 pt-4">
                    <div className="flex items-center justify-between mb-2">
                      <span className="text-xs font-semibold text-foreground/80 flex items-center gap-1.5">
                        <Tag className="w-3.5 h-3.5 text-muted-foreground/60" /> Cluster Labels
                      </span>
                      {activeAddLabelProject !== project.id && (
                        <button
                          onClick={() => setActiveAddLabelProject(project.id)}
                          className="text-[10px] text-primary hover:text-primary-hover font-bold flex items-center gap-0.5 hover:underline"
                        >
                          <Plus className="w-3 h-3" /> Add
                        </button>
                      )}
                    </div>

                    {}
                    {activeAddLabelProject === project.id && (
                      <div className="flex flex-col gap-2 p-2.5 rounded-xl bg-accent/40 border border-border/50 mb-3 animate-in slide-in-from-top duration-250">
                        <div className="grid grid-cols-2 gap-2">
                          <input
                            type="text"
                            placeholder="Key"
                            value={newLabelKey}
                            onChange={(e) => setNewLabelKey(e.target.value)}
                            className="px-2 py-1 text-xs rounded border border-border/50 bg-background focus:outline-none focus:border-primary"
                          />
                          <input
                            type="text"
                            placeholder="Value"
                            value={newLabelValue}
                            onChange={(e) => setNewLabelValue(e.target.value)}
                            className="px-2 py-1 text-xs rounded border border-border/50 bg-background focus:outline-none focus:border-primary"
                          />
                        </div>
                        <div className="flex justify-end gap-1.5">
                          <button
                            onClick={() => {
                              setActiveAddLabelProject(null)
                              setNewLabelKey('')
                              setNewLabelValue('')
                            }}
                            className="px-2 py-1 text-[10px] rounded hover:bg-accent text-muted-foreground transition-colors"
                          >
                            Cancel
                          </button>
                          <button
                            onClick={() => handleAddClusterLabel(project)}
                            className="px-2 py-1 text-[10px] rounded bg-primary text-primary-foreground font-semibold hover:bg-primary-hover transition-colors"
                          >
                            Save
                          </button>
                        </div>
                      </div>
                    )}

                    {}
                    <div className="flex flex-wrap gap-1.5">
                      {labelEntries.length > 0 ? (
                        labelEntries.flatMap(([key, values]) =>
                          values.map((val) => (
                            <span
                              key={`${key}-${val}`}
                              className="inline-flex items-center gap-1 pl-2.5 pr-1.5 py-0.5 rounded-full text-[10px] font-semibold bg-accent border border-border/40 text-foreground/80 group"
                            >
                              <span>
                                {key}:<span className="text-primary font-bold ml-0.5">{val}</span>
                              </span>
                              <button
                                onClick={() => handleRemoveClusterLabel(project, key)}
                                className="p-0.5 rounded-full hover:bg-muted-foreground/20 text-muted-foreground hover:text-foreground opacity-50 group-hover:opacity-100 transition-all"
                              >
                                <X className="w-2.5 h-2.5" />
                              </button>
                            </span>
                          ))
                        )
                      ) : (
                        <span className="text-[10px] text-muted-foreground/50 italic">
                          No cluster tags defined
                        </span>
                      )}
                    </div>
                  </div>
                </div>

                {}
                <div className="px-6 py-4 bg-muted/20 border-t border-border/30 flex gap-2 justify-end">
                  {!isCurrent && (
                    <>
                      <button
                        onClick={() => handleUnregisterProject(project)}
                        disabled={loading}
                        className="flex items-center justify-center p-2 rounded-xl text-destructive hover:bg-destructive/10 transition-colors border border-transparent disabled:opacity-50"
                        title="Unregister project"
                      >
                        <Trash2 className="w-4 h-4" />
                      </button>
                      <button
                        onClick={() => handleSwitchProject(project)}
                        disabled={loading}
                        className="flex items-center justify-center gap-1.5 px-4 py-2 rounded-xl border border-border/50 hover:bg-accent/50 text-foreground font-medium text-xs transition-all hover:scale-[1.02] disabled:opacity-50 shrink-0"
                      >
                        <ExternalLink className="w-3.5 h-3.5" />
                        Switch Project
                      </button>
                    </>
                  )}
                  {isCurrent && (
                    <div className="flex items-center gap-1.5 text-xs text-primary font-bold px-3 py-2">
                      <Info className="w-3.5 h-3.5" /> Currently Active Project
                    </div>
                  )}
                </div>
              </div>
            )
          })}
        </div>
      )}
    </div>
  )
}
