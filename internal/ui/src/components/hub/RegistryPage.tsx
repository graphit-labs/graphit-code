import { useEffect, useState, useCallback } from 'react'
import { Search, RefreshCw, LogOut, Compass } from 'lucide-react'
import { hubApi, type RegistryEntry, type InstalledArtifact } from '@/api/hub'
import { useAppStore } from '@/store/appStore'
import { showToast } from '@/hooks/useToast'
import { ArtifactCard } from './ArtifactCard'
import { AliasModal } from './modals/AliasModal'
import { ConfirmModal } from './modals/ConfirmModal'
import { SubmitModal } from './modals/SubmitModal'
import { EmptyState } from '@/components/shared/EmptyState'
import { LoadingSpinner } from '@/components/shared/LoadingSpinner'
import { cn } from '@/lib/utils'
import { PackageOpen } from 'lucide-react'

export default function RegistryPage() {
  const {
    typeFilter, projectFilter, search, setSearch,
    activeIde, setActiveIde, setActiveProjectId, activeProjectId, webMode,
    activeProjectDir, projectName,
  } = useAppStore()

  const [entries, setEntries] = useState<RegistryEntry[]>([])
  const [installed, setInstalled] = useState<InstalledArtifact[]>([])
  const [projects, setProjects] = useState<Array<{ name: string; remote_id: string }>>([])
  const [gitAuthor, setGitAuthor] = useState('')
  const [loading, setLoading] = useState(true)
  const [projectDisplay, setProjectDisplay] = useState('')
  const [clusterLabels, setClusterLabels] = useState<Record<string, string>>({})

  const [aliasModal, setAliasModal] = useState<{ open: boolean; id: string; type: string; require: boolean; version?: string }>({
    open: false, id: '', type: '', require: false,
  })
  const [confirmModal, setConfirmModal] = useState<{ open: boolean; entry: RegistryEntry | null; installed: InstalledArtifact | null }>({
    open: false, entry: null, installed: null,
  })
  const [submitModal, setSubmitModal] = useState<{ open: boolean; artifact: InstalledArtifact | null }>({
    open: false, artifact: null,
  })

  const loadData = useCallback(async () => {
    setLoading(true)
    try {
      const data = await hubApi.getRegistry(activeProjectDir, activeIde)
      setEntries(data.entries ?? [])
      setInstalled(data.installed ?? [])
      if (data.projects) setProjects(data.projects)
      if (data.ide) setActiveIde(data.ide)
      if (data.active_project_id) setActiveProjectId(data.active_project_id)
      setClusterLabels(data.project_cluster ?? {})
      const name = webMode ? `@${window.__WEB_USER__}` : (data.active_project_name || data.active_project || projectName || 'Global')
      setProjectDisplay(name)
      document.title = `Graphit Hub — ${name}`
    } catch {
      showToast('Failed to connect to Hub API', 'error')
    } finally {
      setLoading(false)
    }
  }, [activeProjectDir, activeIde, webMode, setActiveIde, setActiveProjectId, projectName])

  useEffect(() => { queueMicrotask(loadData) }, [loadData])

  useEffect(() => {
    if (!webMode) {
      hubApi.getGitAuthor().then((d) => setGitAuthor(d.author)).catch(() => {})
    }
  }, [webMode])

  const filteredEntries = entries.filter((e) => {
    const matchType = typeFilter === 'all' || e.type === typeFilter
    const matchProject = projectFilter === 'all' || e.project_id === projectFilter
    const q = search.toLowerCase()
    const matchSearch = !q || e.name.toLowerCase().includes(q) || (e.description || '').toLowerCase().includes(q)
    return matchType && matchProject && matchSearch
  })

  const groups = filteredEntries.reduce<Record<string, RegistryEntry[]>>((acc, e) => {
    const grp = e.type || 'other'
    if (!acc[grp]) acc[grp] = []
    acc[grp].push(e)
    return acc
  }, {})

  const handleInstall = async (id: string, type: string, withAlias = false, version?: string) => {
    if (webMode) return
    const nameCollision = installed.some((d) => (d.alias || d.local_id) === id.split('/').pop() && d.remote_id !== id)
    if (withAlias || nameCollision) {
      setAliasModal({ open: true, id, type, require: nameCollision, version })
      return
    }
    await doInstall(id, type, null, version)
  }

  const doInstall = async (id: string, type: string, alias: string | null, version?: string) => {
    setAliasModal((m) => ({ ...m, open: false }))
    try {
      const res = await hubApi.install(id, alias, activeIde, type, activeProjectDir, version)
      if (res.success) {
        showToast(`Installed ${id}`, 'success')
        await loadData()
      } else {
        showToast(`Install failed: ${res.error ?? 'unknown error'}`, 'error')
      }
    } catch {
      showToast('Connection error!', 'error')
    }
  }

  const handleUninstall = (entry: RegistryEntry, inst: InstalledArtifact) => {
    setConfirmModal({ open: true, entry, installed: inst })
  }

  const doUninstall = async () => {
    const { entry, installed: inst } = confirmModal
    if (!entry || !inst) return
    setConfirmModal((m) => ({ ...m, open: false }))
    try {
      const res = await hubApi.uninstall(entry.id, inst.local_id, activeIde, entry.type, activeProjectDir)
      if (res.success) {
        showToast(`Removed ${inst.local_id || entry.id}`, 'success')
        await loadData()
      } else {
        showToast('Uninstall failed', 'error')
      }
    } catch {
      showToast('Connection error!', 'error')
    }
  }

  const handleSubmit = async (payload: Record<string, unknown>) => {
    try {
      const res = await hubApi.submit({ ...payload, project_dir: activeProjectDir })
      if (res.success) {
        showToast(`Published ${payload.name} v${payload.version}!`, 'success')
        setSubmitModal({ open: false, artifact: null })
        await loadData()
      } else {
        showToast(`Submit failed: ${res.error ?? 'unknown'}`, 'error')
      }
    } catch {
      showToast('Connection error!', 'error')
    }
  }

  return (
    <div className="w-full max-w-7xl mx-auto px-4 md:px-8 py-10 animate-in fade-in duration-300">
      {}
      <div className="flex flex-col md:flex-row md:items-center justify-between gap-4 mb-8 pb-6 border-b border-border/40">
        <div className="flex items-center gap-4">
          <div className="w-12 h-12 rounded-2xl bg-primary/10 border border-primary/20 flex items-center justify-center shrink-0">
            <Compass className="w-6 h-6 text-primary" />
          </div>
          <div>
            <h1 className="text-3xl font-heading font-bold tracking-tight text-foreground">Registry</h1>
            <p className="text-[14px] text-muted-foreground mt-1">
              Community artifacts — skills, rules, agents and more
            </p>
            <div className="flex items-center gap-2 mt-2.5 glass-pill px-3 py-1.5 w-fit">
              <span className="text-xs text-muted-foreground uppercase tracking-widest font-semibold">Active Target:</span>
              <span className="text-xs font-mono font-medium">{projectDisplay || 'Loading...'}</span>
            </div>
          </div>
        </div>

        <div className="flex items-center gap-2.5 flex-wrap">
          {webMode && window.__WEB_USER__ && (
            <button
              onClick={() => { window.location.href = window.__LOGOUT_URL__ ?? '/logout' }}
              className="flex items-center gap-1.5 px-4 py-2.5 rounded-xl border border-border/50 text-sm font-medium hover:bg-accent/50 transition-all hover:scale-[1.02] glass-panel"
            >
              <LogOut className="w-4 h-4" /> Logout
            </button>
          )}
          {}
          <div className="relative">
            <Search className="absolute left-3.5 top-1/2 -translate-y-1/2 w-4 h-4 text-muted-foreground pointer-events-none" />
            <input
              type="text"
              placeholder="Search artifacts…"
              value={search}
              onChange={(e) => setSearch(e.target.value)}
              className="pl-10 pr-4 py-2.5 rounded-xl border border-border/50 bg-background/50 backdrop-blur-sm text-sm outline-none focus:ring-2 focus:ring-primary/20 focus:border-primary transition-all w-56 md:w-72 shadow-sm"
            />
          </div>
          <button onClick={loadData} className="p-2.5 rounded-xl border border-border/50 hover:bg-accent/50 transition-all hover:scale-[1.02] glass-panel" title="Refresh">
            <RefreshCw className="w-4 h-4 text-muted-foreground hover:rotate-180 transition-transform duration-500" />
          </button>
        </div>
      </div>

      {}
      {loading ? (
        <LoadingSpinner size="lg" label="Loading registry..." className="py-20" />
      ) : filteredEntries.length === 0 ? (
        <EmptyState
          icon={PackageOpen}
          title="No artifacts found"
          description="No entries match your current filters."
        />
      ) : (
        Object.entries(groups).map(([cat, items]) => (
          <div key={cat} className="mb-12">
            <div className="flex items-center gap-3 mb-6 border-b border-border/40 pb-2">
              <h3 className="text-xl font-heading font-semibold capitalize text-foreground/90">
                {cat.charAt(0).toUpperCase() + cat.slice(1)}
              </h3>
              <span className="px-2 py-0.5 rounded-md bg-muted/50 text-[11px] font-mono text-muted-foreground">{items.length}</span>
            </div>
            <div className="grid grid-cols-1 lg:grid-cols-2 xl:grid-cols-3 gap-6">
              {items.map((entry) => {
                const inst = installed.find((i) => i.remote_id === entry.id && i.type === entry.type) ?? null
                return (
                  <ArtifactCard
                    key={`${entry.type}/${entry.id}`}
                    variant="registry"
                    entry={entry}
                    installedInfo={inst}
                    webMode={webMode}
                    activeProjectId={activeProjectId}
                    clusterLabels={['knowledge', 'ast'].includes(entry.type) ? clusterLabels : undefined}
                    onInstall={handleInstall}
                    onUninstall={handleUninstall}
                    onSubmit={(art) => setSubmitModal({ open: true, artifact: art })}
                  />
                )
              })}
            </div>
          </div>
        ))
      )}

      {}
      <AliasModal
        open={aliasModal.open}
        artifactId={aliasModal.id}
        requireAlias={aliasModal.require}
        onConfirm={(alias) => doInstall(aliasModal.id, aliasModal.type, alias, aliasModal.version)}
        onCancel={() => setAliasModal((m) => ({ ...m, open: false }))}
      />
      <ConfirmModal
        open={confirmModal.open}
        title="Remove Artifact"
        message={`Remove "${confirmModal.installed?.local_id || confirmModal.entry?.id}" from your project?`}
        confirmLabel="Remove"
        onConfirm={doUninstall}
        onCancel={() => setConfirmModal((m) => ({ ...m, open: false }))}
      />
      <SubmitModal
        open={submitModal.open}
        artifact={submitModal.artifact}
        activeProjectId={activeProjectId}
        gitAuthor={gitAuthor}
        onSubmit={handleSubmit}
        onClose={() => setSubmitModal({ open: false, artifact: null })}
      />
    </div>
  )
}
