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
    <div className={cn('flex flex-col items-center justify-center py-20 px-8 text-center animate-in fade-in duration-300', className)}>
      <div className="w-16 h-16 rounded-2xl bg-primary/8 border border-primary/15 flex items-center justify-center mb-4">
        <Icon className="w-7 h-7 text-primary/80" />
      </div>
      <h3 className="text-base font-semibold text-foreground mb-1.5">{title}</h3>
      {description && (
        <p className="text-sm text-muted-foreground max-w-sm leading-relaxed mb-4">{description}</p>
      )}
      {action}
    </div>
  )
}
