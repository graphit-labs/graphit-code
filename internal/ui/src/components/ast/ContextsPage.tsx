import { useEffect, useState, useCallback } from 'react'
import { useNavigate } from 'react-router-dom'
import { astApi, type Context } from '@/api/ast'
import { showToast } from '@/hooks/useToast'
import { LoadingSpinner } from '@/components/shared/LoadingSpinner'
import { EmptyState } from '@/components/shared/EmptyState'
import { ConfirmModal } from '@/components/hub/modals/ConfirmModal'
import { useAppStore } from '@/store/appStore'
import { Database, Network, Trash2, ArrowRight, Code2, RefreshCw, Layers } from 'lucide-react'
import { cn, formatCount } from '@/lib/utils'

function ContextCard({
  ctx,
  onExplore,
  onDelete,
}: {
  ctx: Context
  onExplore: (id: string) => void
  onDelete: (id: string) => void
}) {
  const isProject = ctx.type === 'project'
  const gradient = isProject ? "from-violet-500 to-indigo-500" : "from-blue-500 to-cyan-500"

  const Icon = isProject ? Code2 : Database

  return (
    <div className="glass-panel rounded-2xl p-5 flex flex-col gap-4 relative overflow-hidden group transition-all duration-300 hover:border-primary/45 hover:shadow-xl hover:-translate-y-0.5">
      {}
      <div className={cn(
        "absolute -right-8 -top-8 w-24 h-24 rounded-full blur-2xl opacity-0 group-hover:opacity-10 dark:group-hover:opacity-15 transition-all duration-500 scale-75 group-hover:scale-100 pointer-events-none z-0",
        isProject ? "bg-violet-500" : "bg-blue-500"
      )} />

      {}
      <div className="flex items-start justify-between gap-3 relative z-10">
        <div className="flex items-center gap-3">
          <div className={cn("w-10 h-10 rounded-xl flex items-center justify-center shrink-0 shadow-inner bg-gradient-to-tr", gradient, "text-white p-0.5")}>
            <div className="w-full h-full rounded-[10px] bg-background flex items-center justify-center">
              <Icon className="w-5 h-5 text-foreground group-hover:scale-110 transition-transform" />
            </div>
          </div>
          <div className="flex flex-col min-w-0 flex-1 justify-center min-h-10">
            <h3 className="font-heading font-semibold text-base text-foreground leading-tight truncate transition-colors group-hover:text-primary">
              {ctx.name}
            </h3>
          </div>
        </div>

        {!isProject && (
          <button
            onClick={() => onDelete(ctx.id)}
            className="w-8 h-8 rounded-xl border border-border/50 text-muted-foreground bg-background/50 flex items-center justify-center hover:bg-red-500/10 hover:text-red-500 transition-all hover:scale-[1.02]"
            title="Remove context"
          >
            <Trash2 className="w-3.5 h-3.5" />
          </button>
        )}
      </div>

      {}
      <div className="flex flex-wrap gap-1.5 pl-13 relative z-10">
        <span className={cn(
          "inline-flex items-center px-2 py-0.5 rounded-md text-[10px] font-semibold uppercase tracking-widest border",
          isProject
            ? "bg-violet-500/10 text-violet-500 border-violet-500/20"
            : "bg-blue-500/10 text-blue-500 border-blue-500/20"
        )}>
          {isProject ? 'project' : 'imported'}
        </span>
      </div>

      {}
      {(ctx.db_path || ctx.path) && (
        <p className="text-[11px] text-muted-foreground/60 font-mono truncate pl-13 relative z-10" title={ctx.db_path || ctx.path}>
          {ctx.db_path || ctx.path}
        </p>
      )}

      {}
      <div className="pl-13 space-y-3 relative z-10 flex-1">
        <div className="flex items-center gap-5 bg-accent/20 dark:bg-accent/10 border border-border/30 rounded-xl p-3">
          <div className="flex flex-col flex-1">
            <span className="text-[10px] uppercase font-bold tracking-wider text-muted-foreground">Nodes</span>
            <span className="text-lg font-heading font-bold text-foreground mt-0.5">{formatCount(ctx.node_count ?? 0)}</span>
          </div>
          <div className="w-px h-8 bg-border/40" />
          <div className="flex flex-col flex-1">
            <span className="text-[10px] uppercase font-bold tracking-wider text-muted-foreground">Edges</span>
            <span className="text-lg font-heading font-bold text-foreground mt-0.5">{formatCount(ctx.edge_count ?? 0)}</span>
          </div>
        </div>
      </div>

      {}
      <div className="flex items-center justify-between pt-3 border-t border-border/40 mt-auto relative z-10 pl-13">
        <div className="flex items-center gap-1.5 text-xs text-muted-foreground/80 font-medium">
          <Network className="w-3.5 h-3.5 text-primary/75" />
          <span>AST Graph</span>
        </div>
        <button
          onClick={() => onExplore(ctx.id)}
          className="flex items-center gap-1.5 px-4 py-1.5 rounded-xl text-[12px] font-semibold btn-premium"
        >
          Explore
          <ArrowRight className="w-3.5 h-3.5 group-hover:translate-x-0.5 transition-transform" />
        </button>
      </div>
    </div>
  )
}

export default function ContextsPage() {
  const navigate = useNavigate()
  const { setActiveContextId, activeProjectDir } = useAppStore()
  const [contexts, setContexts] = useState<Context[]>([])
  const [loading, setLoading] = useState(true)
  const [deleteModal, setDeleteModal] = useState<{ open: boolean; id: string; name: string }>({ open: false, id: '', name: '' })

  const load = useCallback(async () => {
    setLoading(true)
    try {
      const data = await astApi.getContexts(activeProjectDir || undefined)
      setContexts(data.contexts ?? [])
      document.title = `Graphit AST — ${data.project_name ?? 'Explorer'}`
    } catch {
      showToast('Failed to connect to AST API', 'error')
    } finally {
      setLoading(false)
    }
  }, [activeProjectDir])

  useEffect(() => { queueMicrotask(load) }, [load])

  const handleExplore = (id: string) => {
    setActiveContextId(id)
    navigate(`/ast/explorer/${encodeURIComponent(id)}`)
  }

  const handleDelete = (id: string) => {
    const ctx = contexts.find((c) => c.id === id)
    setDeleteModal({ open: true, id, name: ctx?.name ?? id })
  }

  const confirmDelete = async () => {
    const { id } = deleteModal
    setDeleteModal({ open: false, id: '', name: '' })
    try {
      await astApi.deleteContext(id)
      showToast('Context removed', 'success')
      await load()
    } catch {
      showToast('Failed to remove context', 'error')
    }
  }

  const projectCtx = contexts.filter((c) => c.type === 'project')
  const importedCtx = contexts.filter((c) => c.type === 'import')

  return (
    <div className="w-full max-w-7xl mx-auto px-1 sm:px-2 lg:px-4 py-8 lg:py-12 animate-in fade-in duration-300">
      {}
      <div className="flex items-center justify-between gap-4 mb-10 pb-6 border-b border-border/40">
        <div className="flex items-center gap-4">
          <div className="w-12 h-12 rounded-2xl bg-primary/10 border border-primary/20 flex items-center justify-center shrink-0">
            <Layers className="w-6 h-6 text-primary" />
          </div>
          <div>
            <p className="font-mono text-[10px] uppercase tracking-[0.18em] text-primary font-semibold mb-1">Code graph / sources</p>
            <h1 className="text-3xl font-heading font-extrabold tracking-tight text-foreground">
              AST Contexts
            </h1>
            <p className="text-[14px] text-muted-foreground mt-1">
              Explore codebase structure, import networks, and call hierarchies visually.
            </p>
          </div>
        </div>
        <button
          onClick={load}
          className="p-2.5 rounded-xl border border-border/40 hover:bg-accent/40 transition-all hover:scale-[1.02] glass-panel"
          title="Refresh"
        >
          <RefreshCw className="w-4 h-4 text-muted-foreground hover:rotate-180 transition-transform duration-500" />
        </button>
      </div>

      {loading ? (
        <LoadingSpinner size="lg" label="Loading contexts..." className="py-20" />
      ) : contexts.length === 0 ? (
        <EmptyState
          icon={Database}
          title="No contexts available"
          description="Index a project first with graphit ast index to create a knowledge graph context."
        />
      ) : (
        <div className="space-y-12">
          {projectCtx.length > 0 && (
            <section>
              <div className="flex items-center gap-2 mb-6">
                <span className="w-1.5 h-1.5 rounded-full bg-primary" />
                <h2 className="text-xs font-bold uppercase tracking-widest text-muted-foreground">
                  Project Context
                </h2>
              </div>
              <div className="grid grid-cols-1 md:grid-cols-2 xl:grid-cols-3 gap-6">
                {projectCtx.map((ctx) => (
                  <ContextCard key={ctx.id} ctx={ctx} onExplore={handleExplore} onDelete={handleDelete} />
                ))}
              </div>
            </section>
          )}

          {importedCtx.length > 0 && (
            <section>
              <div className="flex items-center gap-2 mb-6">
                <span className="w-1.5 h-1.5 rounded-full bg-info" />
                <h2 className="text-xs font-bold uppercase tracking-widest text-muted-foreground">
                  Imported Contexts
                </h2>
              </div>
              <div className="grid grid-cols-1 md:grid-cols-2 xl:grid-cols-3 gap-6">
                {importedCtx.map((ctx) => (
                  <ContextCard key={ctx.id} ctx={ctx} onExplore={handleExplore} onDelete={handleDelete} />
                ))}
              </div>
            </section>
          )}
        </div>
      )}

      <ConfirmModal
        open={deleteModal.open}
        title="Remove Context"
        message={`Remove the context "${deleteModal.name}" from the AST explorer?`}
        warning="This will not delete any source files. The graph data for this context will be removed."
        confirmLabel="Remove"
        onConfirm={confirmDelete}
        onCancel={() => setDeleteModal({ open: false, id: '', name: '' })}
      />
    </div>
  )
}
