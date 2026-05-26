import { useState, useRef, useEffect } from 'react'
import { useAppStore } from '@/store/appStore'
import { cn } from '@/lib/utils'
import { ChevronDown, FolderRoot, Monitor, Check } from 'lucide-react'

function capitalizeIde(id: string): string {
  const map: Record<string, string> = {
    'vscode': 'VS Code',
    'claude-code': 'Claude Code',
    'gemini-code': 'Gemini Code',
    'opencode': 'OpenCode',
  }
  return map[id] || id.charAt(0).toUpperCase() + id.slice(1)
}

export function ProjectSwitcher() {
  const {
    projects, activeProjectDir, projectName, activeIde,
    switchProject, setActiveIde, projectsLoaded, supportedIdes,
  } = useAppStore()

  const [projectOpen, setProjectOpen] = useState(false)
  const [ideOpen, setIdeOpen] = useState(false)
  const projectRef = useRef<HTMLDivElement>(null)
  const ideRef = useRef<HTMLDivElement>(null)

  useEffect(() => {
    const handler = (e: MouseEvent) => {
      if (projectRef.current && !projectRef.current.contains(e.target as Node)) {
        setProjectOpen(false)
      }
      if (ideRef.current && !ideRef.current.contains(e.target as Node)) {
        setIdeOpen(false)
      }
    }
    document.addEventListener('mousedown', handler)
    return () => document.removeEventListener('mousedown', handler)
  }, [])

  if (!projectsLoaded || projects.length === 0) return null

  const currentIdeLabel = capitalizeIde(activeIde)

  return (
    <div className="px-4 py-3 border-b border-border/40 space-y-2">
      {}
      <div ref={projectRef} className="relative">
        <button
          onClick={() => { setProjectOpen((v) => !v); setIdeOpen(false) }}
          className="flex items-center gap-2.5 w-full px-3 py-2.5 rounded-xl bg-accent/30 hover:bg-accent/50 border border-border/30 transition-all duration-200 group"
        >
          <FolderRoot className="w-4 h-4 text-primary shrink-0" />
          <div className="flex-1 min-w-0 text-left">
            <p className="text-[10px] uppercase tracking-widest text-muted-foreground/60 font-bold leading-none mb-1">Project</p>
            <p className="text-sm font-semibold text-foreground truncate leading-tight">{projectName || 'Select project'}</p>
          </div>
          <ChevronDown className={cn(
            'w-3.5 h-3.5 text-muted-foreground transition-transform duration-200 shrink-0',
            projectOpen && 'rotate-180',
          )} />
        </button>

        {projectOpen && (
          <div className="absolute left-0 right-0 top-full mt-1 z-50 bg-card border border-border/50 rounded-xl shadow-2xl overflow-hidden animate-in fade-in slide-in-from-top-2 duration-150 max-h-[300px] overflow-y-auto scrollbar-none">
            {projects.map((p) => (
              <button
                key={p.dir}
                onClick={() => {
                  switchProject(p.dir)
                  setProjectOpen(false)
                }}
                className={cn(
                  'flex items-center gap-3 w-full px-3.5 py-3 text-left transition-all duration-150 hover:bg-accent/40',
                  p.dir === activeProjectDir && 'bg-primary/8',
                )}
              >
                <div className={cn(
                  'w-2 h-2 rounded-full shrink-0 transition-colors',
                  p.dir === activeProjectDir ? 'bg-primary shadow-sm shadow-primary/30' : 'bg-muted-foreground/20',
                )} />
                <div className="flex-1 min-w-0">
                  <p className="text-sm font-semibold text-foreground truncate">{p.name}</p>
                </div>
                {p.dir === activeProjectDir && (
                  <Check className="w-3.5 h-3.5 text-primary shrink-0" />
                )}
              </button>
            ))}
          </div>
        )}
      </div>

      {}
      <div ref={ideRef} className="relative">
        <button
          onClick={() => { setIdeOpen((v) => !v); setProjectOpen(false) }}
          className="flex items-center gap-2 w-full px-3 py-2 rounded-xl hover:bg-accent/30 border border-transparent hover:border-border/30 transition-all duration-200"
        >
          <Monitor className="w-3.5 h-3.5 text-muted-foreground/70 shrink-0" />
          <span className="text-xs text-muted-foreground font-medium flex-1 text-left">IDE: <span className="text-foreground font-semibold">{currentIdeLabel}</span></span>
          <ChevronDown className={cn(
            'w-3 h-3 text-muted-foreground/50 transition-transform duration-200 shrink-0',
            ideOpen && 'rotate-180',
          )} />
        </button>

        {ideOpen && (
          <div className="absolute left-0 right-0 top-full mt-1 z-50 bg-card border border-border/50 rounded-xl shadow-2xl overflow-hidden animate-in fade-in slide-in-from-top-2 duration-150">
            {supportedIdes.map((ideId) => (
              <button
                key={ideId}
                onClick={() => {
                  setActiveIde(ideId)
                  setIdeOpen(false)
                }}
                className={cn(
                  'flex items-center gap-3 w-full px-3.5 py-2.5 text-left transition-all duration-150 hover:bg-accent/40',
                  ideId === activeIde && 'bg-primary/8',
                )}
              >
                <span className="text-sm font-medium text-foreground">{capitalizeIde(ideId)}</span>
                {ideId === activeIde && (
                  <Check className="w-3.5 h-3.5 text-primary ml-auto shrink-0" />
                )}
              </button>
            ))}
          </div>
        )}
      </div>
    </div>
  )
}
