import { useCallback, useEffect, useState } from 'react'
import { useNavigate } from 'react-router-dom'
import { fetchModules, WikiModule } from '@/api/wiki'
import { LoadingSpinner } from '@/components/shared/LoadingSpinner'
import { EmptyState } from '@/components/shared/EmptyState'
import { BookOpen, ArrowRight, FileText, RefreshCw } from 'lucide-react'
import { cn } from '@/lib/utils'
import { useAppStore } from '@/store/appStore'

function WikiContextCard({
  module,
  onExplore,
}: {
  module: WikiModule
  onExplore: (mod: WikiModule) => void
}) {
  const isProject = module.context === 'project' || module.context === 'user'
  const gradient = isProject ? "from-emerald-500 to-teal-500" : "from-blue-500 to-cyan-500"
  const iconColor = isProject ? "text-emerald-500" : "text-blue-500"

  return (
    <div className="glass-panel rounded-2xl p-5 flex flex-col gap-4 relative overflow-hidden group transition-all duration-300 hover:border-primary/45 hover:shadow-xl hover:-translate-y-0.5">
      {}
      <div className={cn(
        "absolute -right-8 -top-8 w-24 h-24 rounded-full blur-2xl opacity-0 group-hover:opacity-10 dark:group-hover:opacity-15 transition-all duration-500 scale-75 group-hover:scale-100 pointer-events-none z-0",
        isProject ? "bg-emerald-500" : "bg-blue-500"
      )} />

      {}
      <div className="flex items-start justify-between gap-3 relative z-10">
        <div className="flex items-center gap-3">
          <div className={cn("w-10 h-10 rounded-xl flex items-center justify-center shrink-0 shadow-inner bg-gradient-to-tr", gradient, "text-white p-0.5")}>
            <div className="w-full h-full rounded-[10px] bg-background flex items-center justify-center">
              <BookOpen className="w-5 h-5 text-foreground group-hover:scale-110 transition-transform" />
            </div>
          </div>
          <div className="flex flex-col min-w-0 flex-1 justify-center min-h-10">
            <h3 className="font-heading font-semibold text-base text-foreground leading-tight truncate transition-colors group-hover:text-primary">
              {module.label}
            </h3>
          </div>
        </div>
      </div>

      {}
      <div className="flex flex-wrap gap-1.5 pl-13 relative z-10">
        <span className={cn(
          "inline-flex items-center px-2 py-0.5 rounded-md text-[10px] font-semibold uppercase tracking-widest border",
          isProject
            ? "bg-emerald-500/10 text-emerald-500 border-emerald-500/20"
            : "bg-blue-500/10 text-blue-500 border-blue-500/20"
        )}>
          {isProject ? 'project' : 'imported'}
        </span>
      </div>

      {}
      {module.path && (
        <p className="text-[11px] text-muted-foreground/60 font-mono truncate pl-13 relative z-10" title={module.path}>
          {module.path}
        </p>
      )}

      {}
      <div className="pl-13 space-y-3 relative z-10 flex-1">
        <div className="flex items-center gap-5 bg-accent/20 dark:bg-accent/10 border border-border/30 rounded-xl p-3">
          <div className="flex flex-col flex-1">
            <span className="text-[10px] uppercase font-bold tracking-wider text-muted-foreground">Pages</span>
            <span className="text-lg font-heading font-bold text-foreground mt-0.5">{module.pages}</span>
          </div>
          {module.hasLog && (
            <>
              <div className="w-px h-8 bg-border/40" />
              <div className="flex flex-col flex-1">
                <span className="text-[10px] uppercase font-bold tracking-wider text-muted-foreground">Logs</span>
                <span className="text-xs font-semibold text-emerald-600 dark:text-emerald-400 mt-1 flex items-center gap-1">
                  <FileText className="w-3.5 h-3.5" />
                  Changelog
                </span>
              </div>
            </>
          )}
        </div>
      </div>

      {}
      <div className="flex items-center justify-between pt-3 border-t border-border/40 mt-auto relative z-10 pl-13">
        <div className="flex items-center gap-1.5 text-xs text-muted-foreground/80 font-medium">
          <BookOpen className="w-3.5 h-3.5 text-primary/75" />
          <span>Wiki Docs</span>
        </div>
        <button
          onClick={() => onExplore(module)}
          className="flex items-center gap-1.5 px-4 py-1.5 rounded-xl text-[12px] font-semibold btn-premium"
        >
          Explore
          <ArrowRight className="w-3.5 h-3.5 group-hover:translate-x-0.5 transition-transform" />
        </button>
      </div>
    </div>
  )
}

interface WikiContextsPageProps {
  
  moduleFilter: string
}

export default function WikiContextsPage({ moduleFilter }: WikiContextsPageProps) {
  const navigate = useNavigate()
  const { activeProjectDir } = useAppStore()
  const [modules, setModules] = useState<WikiModule[]>([])
  const [loading, setLoading] = useState(true)

  const title = 'Knowledge Contexts'
  const subtitle = 'Browse indexed knowledge bases — project documentation, API specs, imported references, and shared knowledge.'
  const explorerBase = '/knowledge/explorer'

  const refresh = useCallback(() => {
    setLoading(true)
    fetchModules(activeProjectDir || undefined)
      .then(raw => {
        const all = raw ?? []
        const filtered = all.filter(m => m.id === moduleFilter || m.id.startsWith(moduleFilter + '/'))
        setModules(filtered)
      })
      .catch(console.error)
      .finally(() => setLoading(false))
  }, [moduleFilter, activeProjectDir])

  useEffect(() => {
    const load = async () => {
      setLoading(true)
      try {
        const raw = await fetchModules(activeProjectDir || undefined)
        const all = raw ?? []
        const filtered = all.filter(m => m.id === moduleFilter || m.id.startsWith(moduleFilter + '/'))
        setModules(filtered)
      } catch (e) {
        console.error(e)
      } finally {
        setLoading(false)
      }
    }
    load()
  }, [moduleFilter, activeProjectDir])

  const handleExplore = (mod: WikiModule) => {
    if (mod.context === 'project') {
      navigate(explorerBase)
    } else {
      navigate(`${explorerBase}/${encodeURIComponent(mod.context)}`)
    }
  }

  const projectModules = modules.filter(m => m.context === 'project' || m.context === 'user')
  const importedModules = modules.filter(m => m.context !== 'project' && m.context !== 'user')

  return (
    <div className="w-full max-w-7xl mx-auto px-4 md:px-8 py-10 animate-in fade-in duration-300">
      <div className="flex items-center justify-between gap-4 mb-10 pb-6 border-b border-border/40">
        <div className="flex items-center gap-4">
          <div className="w-12 h-12 rounded-2xl bg-primary/10 border border-primary/20 flex items-center justify-center shrink-0">
            <BookOpen className="w-6 h-6 text-primary" />
          </div>
          <div>
            <h1 className="text-3xl font-heading font-extrabold tracking-tight text-foreground">
              {title}
            </h1>
            <p className="text-[14px] text-muted-foreground mt-1">{subtitle}</p>
          </div>
        </div>
        <button
          onClick={refresh}
          className="p-2.5 rounded-xl border border-border/40 bg-card/60 hover:bg-accent text-muted-foreground hover:text-foreground transition-all hover:scale-[1.02] shadow-sm flex items-center justify-center"
          title="Refresh List"
        >
          <RefreshCw className={cn("w-4 h-4 text-muted-foreground hover:rotate-180 transition-transform duration-500", loading && "animate-spin")} />
        </button>
      </div>

      {loading ? (
        <div className="flex items-center justify-center py-24">
          <LoadingSpinner size="lg" label="Loading contexts..." />
        </div>
      ) : modules.length === 0 ? (
        <div className="py-12 bg-card/40 backdrop-blur-md rounded-2xl border border-border/30 shadow-sm">
          <EmptyState
            icon={BookOpen}
            title="No contexts available"
            description="Index project documentation first with graphit knowledge index docs/"
          />
        </div>
      ) : (
        <div className="space-y-12">
          {projectModules.length > 0 && (
            <section>
              <h2 className="text-[11px] font-bold uppercase tracking-widest text-muted-foreground/80 mb-5 flex items-center gap-2">
                <span className="w-1.5 h-1.5 rounded-full bg-emerald-500" />
                Project Scope
              </h2>
              <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-6">
                {projectModules.map(mod => (
                  <WikiContextCard key={mod.id} module={mod} onExplore={handleExplore} />
                ))}
              </div>
            </section>
          )}

          {importedModules.length > 0 && (
            <section>
              <h2 className="text-[11px] font-bold uppercase tracking-widest text-muted-foreground/80 mb-5 flex items-center gap-2">
                <span className="w-1.5 h-1.5 rounded-full bg-blue-500" />
                Imported Scope
              </h2>
              <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-6">
                {importedModules.map(mod => (
                  <WikiContextCard key={mod.id} module={mod} onExplore={handleExplore} />
                ))}
              </div>
            </section>
          )}
        </div>
      )}
    </div>
  )
}
