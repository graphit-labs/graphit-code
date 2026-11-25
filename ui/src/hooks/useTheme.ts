import { useEffect, useState } from 'react'

type Theme = 'dark' | 'light'
const STORAGE_KEY = 'graphit-theme'
const CHANGE_EVENT = 'graphit-theme-change'
function readTheme(): Theme {
  try {
    const stored = localStorage.getItem(STORAGE_KEY)
    if (stored === 'light' || stored === 'dark') return stored
  } catch { /* Optional persistence. */ }
  return document.documentElement.classList.contains('light') ? 'light' : document.documentElement.classList.contains('dark') ? 'dark' : window.matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light'
}
export function useTheme() {
  const [theme, setTheme] = useState<Theme>(readTheme)
  useEffect(() => {
    document.documentElement.classList.remove('dark', 'light')
    document.documentElement.classList.add(theme)
  }, [theme])
  useEffect(() => {
    const sync = () => setTheme(readTheme())
    window.addEventListener(CHANGE_EVENT, sync)
    window.addEventListener('storage', sync)
    return () => { window.removeEventListener(CHANGE_EVENT, sync); window.removeEventListener('storage', sync) }
  }, [])
  const toggle = () => {
    const next = readTheme() === 'dark' ? 'light' : 'dark'
    document.documentElement.classList.remove('dark', 'light')
    document.documentElement.classList.add(next)
    try { localStorage.setItem(STORAGE_KEY, next) } catch { /* Apply for this document. */ }
    window.dispatchEvent(new Event(CHANGE_EVENT))
  }
  return { theme, toggle }
}
