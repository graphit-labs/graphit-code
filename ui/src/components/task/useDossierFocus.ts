import { useLayoutEffect, useRef } from 'react'
import { useLocation } from 'react-router-dom'

export function useDossierFocus(ready: boolean) {
  const { key, hash } = useLocation()
  const dossier = useRef<HTMLElement>(null)
  const handled = useRef<string | null>(null)
  useLayoutEffect(() => {
    if (!ready || !dossier.current || handled.current === key) return
    const heading = dossier.current.querySelector<HTMLElement>('.dossier-header h2')
    const section = hash && Array.from(dossier.current.querySelectorAll<HTMLElement>('[id]')).find(element => '#' + element.id === hash)
    const target = (section && section.querySelector<HTMLElement>('h2')) || heading
    if (!target) return
    handled.current = key
    target.tabIndex = -1
    target.focus({ preventScroll: true })
    target.scrollIntoView?.({ block: 'start' })
  }, [ready, key, hash])
  return dossier
}
