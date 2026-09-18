import { useEffect, useRef, useState } from 'react'
import { Check, ChevronDown, FolderRoot } from 'lucide-react'

import { cn } from '@/lib/utils'
import { useAppStore } from '@/store/appStore'

// The explorer routes render outside the AppShell, so the sidebar's ProjectSwitcher is not
// available on them. This compact picker gives every explorer the same project switch, using
// the shared theme tokens instead of the sidebar's fixed dark palette.
export function ProjectPicker({ className, label = 'Workspace' }: { className?: string; label?: string }) {
  const { projects, activeProjectDir, projectName, switchProject } = useAppStore()
  const [open, setOpen] = useState(false)
  const ref = useRef<HTMLDivElement>(null)

  useEffect(() => {
    const close = (event: MouseEvent) => {
      if (ref.current && !ref.current.contains(event.target as Node)) setOpen(false)
    }
    document.addEventListener('mousedown', close)
    return () => document.removeEventListener('mousedown', close)
  }, [])

  if (projects.length === 0) return null

  const active = projects.find(project => project.dir === activeProjectDir)
  const current = active?.name || projectName || 'Select project'

  return (
    <div ref={ref} className={cn('relative min-w-0', className)}>
      <button
        type="button"
        aria-label="Switch project"
        aria-haspopup="listbox"
        aria-expanded={open}
        onClick={() => setOpen(value => !value)}
        className="flex w-full items-center gap-2 rounded-xl border border-border/40 bg-background/65 px-3 py-2 text-xs font-semibold text-foreground transition-colors hover:bg-accent/45"
      >
        <FolderRoot className="h-3.5 w-3.5 shrink-0 text-primary" />
        <span className="min-w-0 flex-1 truncate text-left">
          <span className="text-muted-foreground">{label}</span> {current}
        </span>
        <ChevronDown className={cn('h-3.5 w-3.5 shrink-0 text-muted-foreground transition-transform', open && 'rotate-180')} />
      </button>
      {open && (
        <div role="listbox" aria-label="Projects" className="absolute left-0 right-0 top-full z-50 mt-1.5 max-h-[320px] overflow-y-auto rounded-xl border border-border/50 bg-card shadow-2xl animate-in fade-in slide-in-from-top-2 duration-150">
          {projects.map(project => (
            <button
              key={project.dir}
              type="button"
              role="option"
              aria-selected={project.dir === activeProjectDir}
              title={project.dir}
              onClick={() => { switchProject(project.dir); setOpen(false) }}
              className={cn('flex w-full items-center gap-3 px-3.5 py-2.5 text-left text-sm font-medium text-foreground transition-colors hover:bg-accent/60', project.dir === activeProjectDir && 'bg-primary/10')}
            >
              <span className="min-w-0 flex-1 truncate">{project.name}</span>
              {project.dir === activeProjectDir && <Check className="h-3.5 w-3.5 shrink-0 text-primary" />}
            </button>
          ))}
        </div>
      )}
    </div>
  )
}
