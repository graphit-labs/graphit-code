import { LucideIcon } from 'lucide-react'
import { cn } from '@/lib/utils'

interface EmptyStateProps {
  icon: LucideIcon
  title: string
  description?: string
  action?: React.ReactNode
  className?: string
}

export function EmptyState({ icon: Icon, title, description, action, className }: EmptyStateProps) {
  return (
    <div className={cn('flex flex-col items-center justify-center py-20 px-8 text-center animate-in fade-in duration-300 relative overflow-hidden', className)}>
      <div className="absolute w-52 h-52 rounded-full border border-primary/10 pointer-events-none" aria-hidden="true" />
      <div className="absolute w-36 h-36 rounded-full border border-primary/[0.06] pointer-events-none" aria-hidden="true" />
      <div className="w-14 h-14 rounded-xl bg-foreground border border-foreground flex items-center justify-center mb-5 shadow-[7px_7px_0_hsl(var(--primary)/0.15)] relative z-10 dark:bg-card dark:border-primary/30">
        <Icon className="w-6 h-6 text-primary" />
      </div>
      <p className="font-mono text-[9px] uppercase tracking-[0.2em] text-primary font-semibold mb-2 relative z-10">No signal</p>
      <h3 className="text-base font-bold text-foreground mb-1.5 relative z-10">{title}</h3>
      {description && (
        <p className="text-sm text-muted-foreground max-w-sm leading-relaxed mb-4 relative z-10">{description}</p>
      )}
      <div className="relative z-10">{action}</div>
    </div>
  )
}
