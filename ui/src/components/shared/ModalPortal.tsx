import { useEffect, useLayoutEffect, useRef, type ReactNode } from 'react'
import { createPortal } from 'react-dom'

/** Keep modal controls above the workspace chrome and background controls inert. */
export function ModalPortal({ children, onClose }: { children: ReactNode; onClose: () => void }) {
  const container = useRef<HTMLDivElement>(null)
  const previousFocus = useRef(document.activeElement as HTMLElement | null)
  const closeRef = useRef(onClose)
  useLayoutEffect(() => { closeRef.current = onClose }, [onClose])
  useEffect(() => {
    const root = document.getElementById('root')
    const wasInert = root?.inert ?? false
    const overflow = document.body.style.overflow
    const returnFocus = previousFocus.current
    if (root) root.inert = true
    document.body.style.overflow = 'hidden'
    const controls = () => [...(container.current?.querySelectorAll<HTMLElement>('button:not(:disabled), a[href], input:not(:disabled), textarea:not(:disabled), select:not(:disabled), [tabindex="0"]') ?? [])]
    if (!container.current?.contains(document.activeElement)) controls()[0]?.focus()
    const handleKey = (event: KeyboardEvent) => {
      // A portaled select owns its keyboard interaction even though it sits
      // outside the modal DOM. Inspect the event path, which survives unmount.
      if (event.defaultPrevented || event.composedPath().some(node => node instanceof Element && node.getAttribute('role') === 'listbox')) return
      if (event.key === 'Escape') { event.preventDefault(); closeRef.current(); return }
      if (event.key !== 'Tab') return
      const elements = controls(), first = elements[0], last = elements[elements.length - 1]
      if (event.shiftKey && document.activeElement === first) { event.preventDefault(); last?.focus() }
      else if (!event.shiftKey && document.activeElement === last) { event.preventDefault(); first?.focus() }
    }
    document.addEventListener('keydown', handleKey)
    return () => {
      if (root) root.inert = wasInert
      document.body.style.overflow = overflow
      document.removeEventListener('keydown', handleKey)
      if (returnFocus?.isConnected) returnFocus.focus()
    }
  }, [])
  return createPortal(<div ref={container}>{children}</div>, document.body)
}
