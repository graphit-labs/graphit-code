import { useAppStore } from '@/store/appStore'
import { LoadingSpinner } from './LoadingSpinner'

export function GlobalLoader() {
  const isGlobalLoading = useAppStore((state) => state.isGlobalLoading)

  if (!isGlobalLoading) return null

  return (
    <div className="fixed top-4 right-4 z-[100] bg-card/80 backdrop-blur-md p-2 rounded-full border border-border shadow-lg animate-in fade-in zoom-in duration-200 pointer-events-none">
      <LoadingSpinner size="sm" />
    </div>
  )
}
