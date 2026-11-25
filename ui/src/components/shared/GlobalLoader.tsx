import { useAppStore } from '@/store/appStore'
import { LoadingSpinner } from './LoadingSpinner'

export function GlobalLoader() {
  const isGlobalLoading = useAppStore((state) => state.isGlobalLoading)

  if (!isGlobalLoading) return null

  return (
    <div role="status" aria-live="polite" aria-atomic="true" className="fixed bottom-4 right-4 z-[100] max-w-[calc(100vw-2rem)] bg-card text-card-foreground px-3 py-2 rounded-lg border border-border shadow-sm pointer-events-none flex items-center gap-2 text-sm font-medium">
      <div aria-hidden="true"><LoadingSpinner size="sm" /></div>
      <span>Working</span>
    </div>
  )
}
