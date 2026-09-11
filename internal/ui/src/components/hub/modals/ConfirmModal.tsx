import { AlertTriangle, X } from 'lucide-react'
import { cn } from '@/lib/utils'

interface ConfirmModalProps {
  open: boolean
  title: string
  message: string
  warning?: string
  confirmLabel?: string
  variant?: 'danger' | 'default'
  onConfirm: () => void
  onCancel: () => void
}

export function ConfirmModal({
  open,
  title,
  message,
  warning = 'This action cannot be undone.',
  confirmLabel = 'Confirm',
  variant = 'danger',
  onConfirm,
  onCancel,
}: ConfirmModalProps) {
  if (!open) return null

  return (
    <div className="fixed inset-0 z-[9000] flex items-center justify-center backdrop-blur-md bg-black/40 animate-fade-in">
      <div
        className="glass-panel bg-card/80 border border-border/50 rounded-2xl p-6 w-full max-w-md shadow-2xl relative overflow-hidden"
        onClick={(e) => e.stopPropagation()}
      >
        {}
        <div className="absolute -top-12 -left-12 w-24 h-24 bg-primary/10 rounded-full blur-2xl pointer-events-none" />

        <div className="flex items-center justify-between mb-2 relative z-10">
          <h2 className="text-lg font-heading font-bold text-foreground">{title}</h2>
          <button onClick={onCancel} className="p-1.5 rounded-xl hover:bg-accent/50 text-muted-foreground hover:text-foreground transition-all">
            <X className="w-4 h-4" />
          </button>
        </div>
        <p className="text-sm text-muted-foreground mb-4 relative z-10 leading-relaxed">{message}</p>

        <div className="flex items-start gap-2.5 p-3.5 rounded-xl bg-amber-500/10 border border-amber-500/20 mb-5 relative z-10">
          <AlertTriangle className="w-4 h-4 text-amber-500 shrink-0 mt-0.5 animate-pulse" />
          <p className="text-xs text-amber-700 dark:text-amber-400 font-medium leading-relaxed">{warning}</p>
        </div>

        <div className="flex justify-end gap-2 relative z-10">
          <button
            onClick={onCancel}
            className="px-4 py-2 rounded-xl border border-border/50 text-sm font-semibold hover:bg-accent/40 transition-colors"
          >
            Cancel
          </button>
          <button
            onClick={onConfirm}
            className={cn(
              'px-4 py-2 rounded-xl text-sm font-semibold transition-all duration-200 active:scale-[0.98]',
              variant === 'danger'
                ? 'bg-red-500 text-white hover:bg-red-600 shadow-[0_2px_10px_rgba(239,68,68,0.2)]'
                : 'bg-foreground text-background hover:opacity-90',
            )}
          >
            {confirmLabel}
          </button>
        </div>
      </div>
    </div>
  )
}
