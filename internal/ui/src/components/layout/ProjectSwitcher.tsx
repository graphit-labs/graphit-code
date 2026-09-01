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
    <div className="px-3.5 py-3 border-b border-white/[0.08] space-y-1.5 relative z-20">
      <div ref={projectRef} className="relative">
        <button
          onClick={() => { setProjectOpen((v) => !v); setIdeOpen(false) }}
          className="flex items-center gap-2.5 w-full px-3 py-2.5 rounded-lg bg-white/[0.045] hover:bg-white/[0.08] border border-white/[0.08] transition-all duration-200 group"
          aria-expanded={projectOpen}
        >
          <FolderRoot className="w-4 h-4 text-[#b9fb63] shrink-0" />
          <div className="flex-1 min-w-0 text-left">
            <p className="font-mono text-[8px] uppercase tracking-[0.16em] text-white/30 font-semibold leading-none mb-1">Workspace</p>
            <p className="text-[13px] font-bold text-white truncate leading-tight">{projectName || 'Select project'}</p>
          </div>
          <ChevronDown className={cn(
            'w-3.5 h-3.5 text-white/32 transition-transform duration-200 shrink-0',
            projectOpen && 'rotate-180',
          )} />
        </button>

        {projectOpen && (
          <div className="absolute left-0 right-0 top-full mt-1.5 z-50 bg-[#202521] border border-white/10 rounded-lg shadow-2xl overflow-hidden animate-in fade-in slide-in-from-top-2 duration-150 max-h-[300px] overflow-y-auto scrollbar-none">
            {projects.map((p) => (
              <button
                key={p.dir}
                onClick={() => {
                  switchProject(p.dir)
                  setProjectOpen(false)
                }}
                className={cn(
                  'flex items-center gap-3 w-full px-3.5 py-3 text-left transition-all duration-150 hover:bg-white/[0.07]',
                  p.dir === activeProjectDir && 'bg-[#b9fb63]/10',
                )}
              >
                <div className={cn(
                  'w-2 h-2 rounded-full shrink-0 transition-colors',
                  p.dir === activeProjectDir ? 'bg-[#b9fb63] shadow-sm shadow-[#b9fb63]/30' : 'bg-white/15',
                )} />
                <div className="flex-1 min-w-0">
                  <p className="text-sm font-semibold text-white truncate">{p.name}</p>
                </div>
                {p.dir === activeProjectDir && (
                  <Check className="w-3.5 h-3.5 text-[#b9fb63] shrink-0" />
                )}
              </button>
            ))}
          </div>
        )}
      </div>

      <div ref={ideRef} className="relative">
        <button
          onClick={() => { setIdeOpen((v) => !v); setProjectOpen(false) }}
          className="flex items-center gap-2 w-full px-3 py-2 rounded-lg hover:bg-white/[0.055] border border-transparent hover:border-white/[0.06] transition-all duration-200"
          aria-expanded={ideOpen}
        >
          <Monitor className="w-3.5 h-3.5 text-white/30 shrink-0" />
          <span className="text-[11px] text-white/38 font-medium flex-1 text-left">IDE <span className="text-white/72 font-semibold ml-1">{currentIdeLabel}</span></span>
          <ChevronDown className={cn(
            'w-3 h-3 text-white/24 transition-transform duration-200 shrink-0',
            ideOpen && 'rotate-180',
          )} />
        </button>

        {ideOpen && (
          <div className="absolute left-0 right-0 top-full mt-1.5 z-50 bg-[#202521] border border-white/10 rounded-lg shadow-2xl overflow-hidden animate-in fade-in slide-in-from-top-2 duration-150">
            {supportedIdes.map((ideId) => (
              <button
                key={ideId}
                onClick={() => {
                  setActiveIde(ideId)
                  setIdeOpen(false)
                }}
                className={cn(
                  'flex items-center gap-3 w-full px-3.5 py-2.5 text-left transition-all duration-150 hover:bg-white/[0.07]',
                  ideId === activeIde && 'bg-[#b9fb63]/10',
                )}
              >
                <span className="text-sm font-medium text-white">{capitalizeIde(ideId)}</span>
                {ideId === activeIde && (
                  <Check className="w-3.5 h-3.5 text-[#b9fb63] ml-auto shrink-0" />
                )}
              </button>
            ))}
          </div>
        )}
      </div>
    </div>
  )
}
