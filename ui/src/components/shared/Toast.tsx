import { useToast } from '@/hooks/useToast'
import { cn } from '@/lib/utils'
import { CheckCircle, XCircle, Info, X } from 'lucide-react'

export function ToastContainer() {
  const { toasts, removeToast } = useToast()

  const icons = {
    success: <CheckCircle className="w-4 h-4 shrink-0" />,
    error: <XCircle className="w-4 h-4 shrink-0" />,
    info: <Info className="w-4 h-4 shrink-0" />,
  }

  const colors = {
    success: 'border-success/30 text-success',
    error: 'border-destructive/30 text-destructive',
    info: 'border-info/30 text-info',
  }

  return (
    <div className="fixed top-4 right-4 z-[9999] flex flex-col gap-2 pointer-events-none" role="status" aria-live="polite">
      {toasts.map((toast) => (
        <div
          key={toast.id}
          className={cn(
            'pointer-events-auto flex items-center gap-2.5 px-4 py-3 rounded-lg bg-popover border text-sm font-medium',
            'shadow-2xl max-w-sm animate-slide-in-right',
            colors[toast.type],
          )}
        >
          {icons[toast.type]}
          <span className="flex-1">{toast.message}</span>
          <button
            onClick={() => removeToast(toast.id)}
            className="opacity-70 hover:opacity-100 transition-opacity"
            aria-label="Dismiss notification"
          >
            <X className="w-3.5 h-3.5" />
          </button>
        </div>
      ))}
    </div>
  )
}
