import { useNavigate } from 'react-router-dom'

import { cn } from '@/lib/utils'

export type TaskExplorerMode = 'tasks' | 'sessions'

export function TaskModeToggle({ mode }: { mode: TaskExplorerMode }) {
  const navigate = useNavigate()
  return (
    <div role="tablist" aria-label="Task explorer view" className="mt-3 flex items-center gap-1 rounded-xl border border-border/40 bg-background/45 p-1">
      <button
        type="button"
        role="tab"
        aria-selected={mode === 'tasks'}
        onClick={() => navigate('/task/explorer')}
        className={cn('flex-1 rounded-lg px-3 py-1.5 text-xs font-bold transition-colors', mode === 'tasks' ? 'bg-primary/15 text-primary' : 'text-muted-foreground hover:text-foreground')}
      >
        Tasks
      </button>
      <button
        type="button"
        role="tab"
        aria-selected={mode === 'sessions'}
        onClick={() => navigate('/task/sessions')}
        className={cn('flex-1 rounded-lg px-3 py-1.5 text-xs font-bold transition-colors', mode === 'sessions' ? 'bg-primary/15 text-primary' : 'text-muted-foreground hover:text-foreground')}
      >
        Sessions
      </button>
    </div>
  )
}
