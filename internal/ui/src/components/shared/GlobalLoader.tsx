import { useAppStore } from '@/store/appStore'
import { LoadingSpinner } from './LoadingSpinner'

export function GlobalLoader() {
  const isGlobalLoading = useAppStore((state) => state.isGlobalLoading)

  if (!isGlobalLoading) return null

  return (
    <div className="fixed bottom-4 right-4 z-[100] bg-[#161a18]/95 text-[#b9fb63] backdrop-blur-md px-3 py-2 rounded-lg border border-white/10 shadow-2xl animate-in fade-in zoom-in duration-200 pointer-events-none flex items-center gap-2">
      <LoadingSpinner size="sm" />
      <span className="font-mono text-[9px] uppercase tracking-[0.16em]">Working</span>
    </div>
  )
}
