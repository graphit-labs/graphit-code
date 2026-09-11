import { useState, useRef, useCallback, useEffect } from 'react'
import { Sidebar, MobileSidebar } from './Sidebar'
import { ToastContainer } from '@/components/shared/Toast'

const LS = {
  get<T>(key: string, fallback: T): T {
    try { const v = localStorage.getItem(key); return v ? JSON.parse(v) : fallback } catch { return fallback }
  },
  set(key: string, value: unknown) {
    try { localStorage.setItem(key, JSON.stringify(value)) } catch { /* ignored */ }
  },
}

function useResizable(
  initial: number,
  min: number,
  max: number,
  direction: 'right' | 'left' = 'right',
  storageKey?: string,
) {
  const [size, setSize] = useState(() =>
    storageKey ? LS.get<number>(storageKey, initial) : initial
  )
  const dragging = useRef(false)
  const startX = useRef(0)
  const startSize = useRef(size)

  const onMouseDown = useCallback((e: React.MouseEvent) => {
    dragging.current = true
    startX.current = e.clientX
    startSize.current = size
    document.body.style.cursor = 'col-resize'
    document.body.style.userSelect = 'none'

    const onMove = (ev: MouseEvent) => {
      if (!dragging.current) return
      const delta = direction === 'right' ? ev.clientX - startX.current : startX.current - ev.clientX
      const next = Math.max(min, Math.min(max, startSize.current + delta))
      setSize(next)
    }
    const onUp = () => {
      dragging.current = false
      document.body.style.cursor = ''
      document.body.style.userSelect = ''
      if (storageKey) LS.set(storageKey, sizeRef.current)
      window.removeEventListener('mousemove', onMove)
      window.removeEventListener('mouseup', onUp)
    }
    window.addEventListener('mousemove', onMove)
    window.addEventListener('mouseup', onUp)
  }, [size, min, max, direction, storageKey])

  const sizeRef = useRef(size)
  // eslint-disable-next-line react-hooks/immutability
  useEffect(() => { sizeRef.current = size }, [size])

  return { size, onMouseDown }
}

interface AppShellProps {
  children: React.ReactNode
}

export function AppShell({ children }: AppShellProps) {
  const sidebar = useResizable(272, 236, 420, 'right', 'graphit_main_sidebar_width')

  return (
    <div
      className="app-frame flex min-h-screen bg-background text-foreground relative overflow-hidden"
      style={{ '--sidebar-width': `${sidebar.size}px` } as React.CSSProperties}
    >
      <div className="workspace-grid" aria-hidden="true" />
      <div className="workspace-beacon" aria-hidden="true" />

      <aside className="hidden md:flex shrink-0 flex-col fixed inset-y-0 left-0 z-30 w-[var(--sidebar-width)]">
        <Sidebar />
        <div
          className="absolute right-0 top-0 h-full w-1 cursor-col-resize hover:bg-[#b9fb63]/60 transition-colors z-40 group"
          onMouseDown={sidebar.onMouseDown}
          role="separator"
          aria-orientation="vertical"
          aria-label="Resize navigation"
        >
          <div className="absolute right-0 top-1/2 -translate-y-1/2 w-1 h-10 rounded-full bg-white/10 group-hover:bg-[#b9fb63] opacity-0 group-hover:opacity-100 transition-opacity" />
        </div>
      </aside>

      <MobileSidebar />

      <main className="flex-1 min-h-screen flex flex-col relative z-10 md:ml-[var(--sidebar-width)]">
        <div className="flex-1 w-full max-w-[1680px] mx-auto px-3 sm:px-5 lg:px-8">
          {children}
        </div>
      </main>

      <ToastContainer />
    </div>
  )
}
