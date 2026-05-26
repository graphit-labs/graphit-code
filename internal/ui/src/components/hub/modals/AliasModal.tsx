import { useRef, useEffect, useState } from 'react'
import { X } from 'lucide-react'
import { cn } from '@/lib/utils'

interface AliasModalProps {
  open: boolean
  artifactId: string
  requireAlias: boolean
  onConfirm: (alias: string | null) => void
  onCancel: () => void
}

export function AliasModal({ open, artifactId, requireAlias, onConfirm, onCancel }: AliasModalProps) {
  const [alias, setAlias] = useState('')
  const inputRef = useRef<HTMLInputElement>(null)

  useEffect(() => {
    if (open) {
      setAlias('')
      setTimeout(() => inputRef.current?.focus(), 50)
    }
  }, [open])

  if (!open) return null

  const handleConfirm = () => {
    if (requireAlias && !alias.trim()) return
    onConfirm(alias.trim() || null)
  }

  return (
    <div className="fixed inset-0 z-[9000] flex items-center justify-center backdrop-blur-md bg-black/40 animate-fade-in">
      <div
        className="glass-panel bg-card/80 border border-border/50 rounded-2xl p-6 w-full max-w-md shadow-2xl relative overflow-hidden"
        onClick={(e) => e.stopPropagation()}
      >
        {}
        <div className="absolute -top-12 -left-12 w-24 h-24 bg-primary/10 rounded-full blur-2xl pointer-events-none" />

        <div className="flex items-center justify-between mb-2 relative z-10">
          <h2 className="text-lg font-heading font-bold text-foreground">Install with Alias</h2>
          <button onClick={onCancel} className="p-1.5 rounded-xl hover:bg-accent/50 text-muted-foreground hover:text-foreground transition-all">
            <X className="w-4 h-4" />
          </button>
        </div>
        <p className="text-sm text-muted-foreground mb-4 relative z-10 leading-relaxed">
          {requireAlias
            ? `The name "${artifactId.split('/').pop()}" is already taken. Please provide a unique alias:`
            : 'Enter an optional alias for this installation:'}
        </p>
        <input
          ref={inputRef}
          type="text"
          value={alias}
          onChange={(e) => setAlias(e.target.value)}
          onKeyDown={(e) => e.key === 'Enter' && handleConfirm()}
          placeholder="Enter alias..."
          className="w-full px-3.5 py-2.5 rounded-xl border border-border/50 bg-background/50 backdrop-blur-sm text-sm text-foreground outline-none focus:ring-2 focus:ring-primary/20 focus:border-primary/80 transition-all duration-200 relative z-10"
        />
        <div className="flex justify-end gap-2 mt-6 relative z-10">
          <button
            onClick={onCancel}
            className="px-4 py-2 rounded-xl border border-border/50 text-sm font-semibold hover:bg-accent/40 transition-colors"
          >
            Cancel
          </button>
          <button
            onClick={handleConfirm}
            disabled={requireAlias && !alias.trim()}
            className={cn(
              'px-4 py-2 rounded-xl text-sm font-semibold transition-all duration-250',
              'bg-foreground text-background hover:opacity-90 active:scale-[0.98]',
              'disabled:opacity-40 disabled:cursor-not-allowed disabled:active:scale-100',
            )}
          >
            Confirm
          </button>
        </div>
      </div>
    </div>
  )
}
