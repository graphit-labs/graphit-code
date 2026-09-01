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
    success: 'border-[#b9fb63]/50 text-[#d9ffad]',
    error: 'border-red-400/50 text-red-200',
    info: 'border-cyan-300/50 text-cyan-100',
  }

  return (
    <div className="fixed top-4 right-4 z-[9999] flex flex-col gap-2 pointer-events-none" role="status" aria-live="polite">
      {toasts.map((toast) => (
        <div
          key={toast.id}
          className={cn(
            'pointer-events-auto flex items-center gap-2.5 px-4 py-3 rounded-lg bg-[#161a18]/95 backdrop-blur-xl border text-sm font-semibold',
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
