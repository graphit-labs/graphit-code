import { useCallback, useState } from 'react'

export interface Toast {
  id: string
  message: string
  type: 'success' | 'error' | 'info'
}

let _addToast: ((t: Toast) => void) | null = null

export function showToast(message: string, type: Toast['type'] = 'info') {
  if (_addToast) {
    _addToast({ id: crypto.randomUUID(), message, type })
  }
}

export function useToast() {
  const [toasts, setToasts] = useState<Toast[]>([])

  const addToast = useCallback((toast: Toast) => {
    setToasts((prev) => [...prev, toast])
    setTimeout(() => {
      setToasts((prev) => prev.filter((t) => t.id !== toast.id))
    }, 4000)
  }, [])

  _addToast = addToast

  const removeToast = useCallback((id: string) => {
    setToasts((prev) => prev.filter((t) => t.id !== id))
  }, [])

  return { toasts, addToast, removeToast }
}
