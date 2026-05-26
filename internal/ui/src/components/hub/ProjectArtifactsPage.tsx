import { useEffect, useState, useCallback } from 'react'
import { hubApi, type InstalledArtifact } from '@/api/hub'
import { useAppStore } from '@/store/appStore'
import { showToast } from '@/hooks/useToast'
import { ArtifactCard } from './ArtifactCard'
import { ConfirmModal } from './modals/ConfirmModal'
import { SubmitModal } from './modals/SubmitModal'
import { EmptyState } from '@/components/shared/EmptyState'
import { LoadingSpinner } from '@/components/shared/LoadingSpinner'
import { FolderOpen, CloudUpload, RefreshCw, Download, Package, ArrowUpCircle } from 'lucide-react'
import { cn } from '@/lib/utils'

export default function ProjectArtifactsPage() {
  const { activeIde, webMode, activeProjectDir, projectName, setProjectName } = useAppStore()
  const [projectArtifacts, setProjectArtifacts] = useState<InstalledArtifact[]>([])
  const [importedArtifacts, setImportedArtifacts] = useState<InstalledArtifact[]>([])
  const [projectPath, setProjectPath] = useState('')
  const [clusterLabels, setClusterLabels] = useState<Record<string, string>>({})
  const [gitAuthor, setGitAuthor] = useState('')
  const [loading, setLoading] = useState(true)
  const [confirmModal, setConfirmModal] = useState<{ open: boolean; art: InstalledArtifact | null }>({ open: false, art: null })
  const [submitModal, setSubmitModal] = useState<{ open: boolean; art: InstalledArtifact | null }>({ open: false, art: null })
  const [unpublishModal, setUnpublishModal] = useState<{ open: boolean; id: string; type: string }>({ open: false, id: '', type: '' })
  const [updating, setUpdating] = useState(false)

  const load = useCallback(async () => {
    setLoading(true)
    try {
      const data = await hubApi.getProjectArtifacts(activeProjectDir, activeIde)
      setProjectArtifacts(data.project_artifacts ?? [])
      setImportedArtifacts(data.imported_artifacts ?? [])
      setProjectName(data.project_name ?? '')
      setProjectPath(data.project_path ?? '')
      setClusterLabels(data.project_cluster ?? {})
    } catch { showToast('Failed to load project artifacts', 'error') }
    finally { setLoading(false) }
  }, [activeProjectDir, activeIde, setProjectName])

  useEffect(() => { load() }, [load])
  useEffect(() => { if (!webMode) hubApi.getGitAuthor().then((d) => setGitAuthor(d.author)).catch(() => {}) }, [webMode])

  const handleUnlink = async (art: InstalledArtifact) => {
    try {
      await hubApi.unlinkLocal(art.local_id, art.type, activeIde, projectPath)
      showToast('Unlinked successfully', 'success')
      await load()
    } catch {
      showToast('Failed to unlink', 'error')
    }
  }

  const handleRemove = async () => {
    const art = confirmModal.art
    if (!art) return
    setConfirmModal({ open: false, art: null })
    try {
      const res = await hubApi.uninstall(art.local_id, art.local_id, activeIde, art.type, activeProjectDir)
      if (res.success) { showToast(`Removed ${art.local_id}`, 'success'); await load() }
      else showToast('Remove failed', 'error')
    } catch { showToast('Connection error!', 'error') }
  }

  const handleSubmit = async (payload: Record<string, unknown>) => {
    try {
      const res = await hubApi.submit({ ...payload, project_dir: activeProjectDir })
      if (res.success) { showToast('Published!', 'success'); setSubmitModal({ open: false, art: null }); await load() }
      else showToast(`Submit failed: ${res.error}`, 'error')
    } catch { showToast('Connection error!', 'error') }
  }

  const handleUnpublish = (id: string, type: string) => {
    setUnpublishModal({ open: true, id, type })
  }

  const doUnpublish = async () => {
    const { id, type } = unpublishModal
    setUnpublishModal((m) => ({ ...m, open: false }))
    try {
      await hubApi.unpublish(id, type, activeProjectDir)
      showToast('Unpublished', 'success')
      await load()
    } catch {
      showToast('Unpublish failed', 'error')
    }
  }

  const handleUpdate = async (id: string, type: string) => {
    try {
      const res = await hubApi.updateOne(id, type, activeIde, activeProjectDir)
      if (res.success) {
        showToast(`Updated ${id}`, 'success')
        await load()
      } else {
        showToast(`Update failed: ${res.error ?? 'unknown error'}`, 'error')
      }
    } catch {
      showToast('Connection error!', 'error')
    }
  }

  const handleUpdateAll = async () => {
    setUpdating(true)
    try {
      const res = await hubApi.updateAll(activeIde, activeProjectDir)
      if (res.success) {
        showToast('All artifacts updated!', 'success')
        await load()
      } else {
        showToast('Update failed', 'error')
      }
    } catch {
      showToast('Connection error!', 'error')
    } finally {
      setUpdating(false)
    }
  }

  const linkedArts = importedArtifacts.filter(a => a.origin === 'link')
  const hubArts = importedArtifacts.filter(a => a.origin !== 'link' && a.origin !== 'publish')
  const totalCount = projectArtifacts.length + linkedArts.length + hubArts.length
  const updatesCount = hubArts.filter(a => a.has_update).length
  const typeSummary = new Map<string, number>()
  for (const a of projectArtifacts) {
    typeSummary.set(a.type, (typeSummary.get(a.type) ?? 0) + 1)
  }

  return (
    <div className="w-full max-w-7xl mx-auto px-4 md:px-8 py-10 animate-in fade-in duration-300">
      {}
      <div className="flex items-center justify-between gap-4 mb-8 pb-6 border-b border-border/40">
        <div className="flex items-center gap-4">
          <div className="w-12 h-12 rounded-2xl bg-primary/10 border border-primary/20 flex items-center justify-center shrink-0">
            <Package className="w-6 h-6 text-primary" />
          </div>
          <div>
            <h1 className="text-3xl font-heading font-bold tracking-tight text-foreground">
              Project Artifacts
            </h1>
            <div className="flex flex-col gap-1.5 mt-1">
              <p className="text-[14px] text-muted-foreground">
                Artifacts created or installed in the current project directory.
              </p>
              {projectName && (
                <div className="flex items-center gap-2 glass-pill px-3 py-1.5 w-fit">
                  <span className="text-xs text-muted-foreground uppercase tracking-widest font-semibold">Project:</span>
                  <span className="text-xs font-mono font-medium">{projectName}</span>
                  {projectPath && <span className="ml-1 text-xs opacity-60">({projectPath})</span>}
                </div>
              )}
            </div>
          </div>
        </div>
        <button onClick={load} className="p-2.5 rounded-xl border border-border/50 hover:bg-accent/50 transition-all hover:scale-[1.02] glass-panel" title="Refresh">
          <RefreshCw className="w-4 h-4 text-muted-foreground hover:rotate-180 transition-transform duration-500" />
        </button>
      </div>

      {loading ? (
        <LoadingSpinner size="lg" label="Scanning project artifacts..." className="py-20" />
      ) : totalCount === 0 ? (
        <EmptyState
          icon={FolderOpen}
          title="No project artifacts"
          description="Create artifacts in your IDE directory (rules, skills, agents, etc.) or install them from the Registry."
        />
      ) : (
        <div className="space-y-8">
          {}
          <section>
            <div className="flex items-center gap-3 mb-6 border-b border-border/40 pb-2">
              <h2 className="text-xl font-heading font-semibold text-foreground/90 flex items-center gap-2">
                <CloudUpload className="w-5 h-5 text-muted-foreground" />
                Your Artifacts
              </h2>
              <span className="text-[11px] font-medium px-2 py-0.5 rounded-md bg-blue-500/10 text-blue-400 border border-blue-500/20">
                {projectArtifacts.length}
              </span>
              {typeSummary.size > 0 && (
                <div className="flex items-center gap-1.5 ml-auto">
                  {Array.from(typeSummary.entries()).map(([type, count]) => (
                    <span key={type} className="text-[10px] font-medium px-1.5 py-0.5 rounded-md border border-border/30 bg-muted/50 text-muted-foreground">
                      {type} ({count})
                    </span>
                  ))}
                </div>
              )}
            </div>
            
            <div className="space-y-8">
              {}
              {projectArtifacts.some(a => ['knowledge', 'ast'].includes(a.type)) && (
                <div>
                  <h3 className="text-sm font-semibold text-muted-foreground mb-4 uppercase tracking-wider">Core Context</h3>
                  <div className="grid grid-cols-1 lg:grid-cols-2 xl:grid-cols-3 gap-6">
                    {projectArtifacts.filter(a => ['knowledge', 'ast'].includes(a.type)).map((art) => (
                      <ArtifactCard
                        key={`project/${art.type}/${art.local_id}`}
                        variant="project"
                        installedInfo={art}
                        projectPath={projectPath}
                        clusterLabels={clusterLabels}
                        onSubmit={() => setSubmitModal({ open: true, art })}
                        onUnpublish={handleUnpublish}
                      />
                    ))}
                  </div>
                </div>
              )}

              {}
              <div>
                <h3 className="text-sm font-semibold text-muted-foreground mb-4 uppercase tracking-wider">IDE Artifacts</h3>
                {!projectArtifacts.some(a => !['knowledge', 'ast'].includes(a.type)) ? (
                  <div className="rounded-xl border border-dashed border-border p-6 text-center bg-muted/20">
                    <p className="text-sm text-muted-foreground">
                      No IDE-specific artifacts found. Create rules, skills, agents, or commands in your IDE directory (e.g. <code className="text-xs bg-muted px-1.5 py-0.5 rounded">.agents/</code> or <code className="text-xs bg-muted px-1.5 py-0.5 rounded">.gemini/</code>) to see them here and submit them to the Hub.
                    </p>
                  </div>
                ) : (
                  <div className="grid grid-cols-1 lg:grid-cols-2 xl:grid-cols-3 gap-6">
                    {projectArtifacts.filter(a => !['knowledge', 'ast'].includes(a.type)).map((art) => (
                      <ArtifactCard
                        key={`project/${art.type}/${art.local_id}`}
                        variant="project"
                        installedInfo={art}
                        projectPath={projectPath}
                        onSubmit={() => setSubmitModal({ open: true, art })}
                        onUnpublish={handleUnpublish}
                      />
                    ))}
                  </div>
                )}
              </div>
            </div>
          </section>

          {}
          {linkedArts.length > 0 && (
            <section>
              <div className="flex items-center gap-3 mb-6 border-b border-border/40 pb-2">
                <h2 className="text-xl font-heading font-semibold text-foreground/90 flex items-center gap-2">
                  <Package className="w-5 h-5 text-muted-foreground" />
                  Linked Artifacts
                </h2>
                <span className="text-[11px] font-medium px-2 py-0.5 rounded-md bg-purple-500/10 text-purple-400 border border-purple-500/20">
                  {linkedArts.length}
                </span>
              </div>
              <div className="grid grid-cols-1 lg:grid-cols-2 xl:grid-cols-3 gap-6">
                {linkedArts.map((art) => (
                  <ArtifactCard
                    key={`linked/${art.type}/${art.local_id}`}
                    variant="imported"
                    installedInfo={art}
                    projectPath={projectPath}
                    onUnlink={() => handleUnlink(art)}
                  />
                ))}
              </div>
            </section>
          )}

          {}
          <section>
            <div className="flex items-center gap-3 mb-6 border-b border-border/40 pb-2">
              <h2 className="text-xl font-heading font-semibold text-foreground/90 flex items-center gap-2">
                <Download className="w-5 h-5 text-muted-foreground" />
                Imported Artifacts
              </h2>
              <span className="text-[11px] font-medium px-2 py-0.5 rounded-md bg-emerald-500/10 text-emerald-400 border border-emerald-500/20">
                {hubArts.length}
              </span>
              {updatesCount > 0 && (
                <button
                  onClick={handleUpdateAll}
                  disabled={updating}
                  className="flex items-center gap-1.5 ml-auto px-3.5 py-1.5 rounded-xl border border-orange-400/50 bg-orange-500/10 text-orange-600 dark:text-orange-400 text-xs font-semibold hover:bg-orange-500/20 disabled:opacity-50 transition-all hover:scale-[1.02] shadow-sm"
                >
                  <RefreshCw className={cn('w-3.5 h-3.5', updating && 'animate-spin')} />
                  Update All ({updatesCount})
                </button>
              )}
            </div>
            {hubArts.length === 0 ? (
              <div className="rounded-xl border border-dashed border-border p-6 text-center">
                <p className="text-sm text-muted-foreground">
                  No imported artifacts. Install artifacts from the <a href="/hub/registry" className="text-blue-400 hover:underline">Registry</a> to see them here.
                </p>
              </div>
            ) : (
              <div className="grid grid-cols-1 lg:grid-cols-2 xl:grid-cols-3 gap-6">
                {hubArts.map((art) => (
                  <ArtifactCard
                    key={`imported/${art.type}/${art.local_id}`}
                    variant="imported"
                    installedInfo={art}
                    projectPath={projectPath}
                    onRemove={() => setConfirmModal({ open: true, art })}
                    onUpdate={handleUpdate}
                  />
                ))}
              </div>
            )}
          </section>
        </div>
      )}

      <ConfirmModal
        open={confirmModal.open}
        title="Remove Artifact"
        message={`Remove "${confirmModal.art?.local_id}" from your project?`}
        confirmLabel="Remove"
        onConfirm={handleRemove}
        onCancel={() => setConfirmModal({ open: false, art: null })}
      />
      <ConfirmModal
        open={unpublishModal.open}
        title="Unpublish Artifact"
        message={`Remove "${unpublishModal.id}" from the global registry?`}
        confirmLabel="Unpublish"
        onConfirm={doUnpublish}
        onCancel={() => setUnpublishModal((m) => ({ ...m, open: false }))}
      />
      <SubmitModal
        open={submitModal.open}
        artifact={submitModal.art}
        activeProjectId=""
        gitAuthor={gitAuthor}
        onSubmit={handleSubmit}
        onClose={() => setSubmitModal({ open: false, art: null })}
      />
    </div>
  )
}
