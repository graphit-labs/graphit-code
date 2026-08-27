import { useEffect, useState } from 'react'
import { X, Plus, Trash2, Globe, Folder } from 'lucide-react'
import { cn, bumpPatch } from '@/lib/utils'
import type { InstalledArtifact } from '@/api/hub'

const TYPES = ['agent', 'rule', 'skill', 'command', 'knowledge', 'ast', 'mcp', 'power', 'language', 'framework']

interface Dep { type: string; id: string; version: string }

interface SubmitModalProps {
  open: boolean
  artifact: InstalledArtifact | null
  activeProjectId: string
  gitAuthor: string
  onSubmit: (payload: Record<string, unknown>) => Promise<void>
  onClose: () => void
}

export function SubmitModal({
  open,
  artifact,
  activeProjectId: _activeProjectId,
  gitAuthor,
  onSubmit,
  onClose,
}: SubmitModalProps) {
  const [scope, setScope] = useState<'global' | 'project'>('project')
  const [name, setName] = useState('')
  const [version, setVersion] = useState('1.0.0')
  const [description, setDescription] = useState('')
  const [tags, setTags] = useState('')
  const [author, setAuthor] = useState('')
  const [deps, setDeps] = useState<Dep[]>([])
  const [loading, setLoading] = useState(false)

  const isUpdate = !!(artifact?.published)
  const existingScope = (artifact as unknown as Record<string, unknown>)?.project_id ? 'project' : 'global'

  useEffect(() => {
    if (!open || !artifact) return
    queueMicrotask(() => {
      setScope(isUpdate ? existingScope : 'project')
      setName(artifact.registry_name || artifact.local_id || '')
      setVersion(isUpdate ? bumpPatch(artifact.registry_version || '1.0.0') : '1.0.0')
      setDescription(artifact.registry_description || '')
      setTags((artifact.registry_tags || []).join(', '))
      setAuthor(artifact.registry_author || gitAuthor)
      setDeps((artifact.registry_dependencies || []).map((d) => ({ ...d })))
    })
    // Deliberately keyed on open/artifact only: this seeds the form when the modal
    // opens. Adding isUpdate/existingScope/gitAuthor would re-run it while the modal
    // is open and overwrite whatever the user has typed.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open, artifact])

  if (!open || !artifact) return null

  const handleSubmit = async () => {
    setLoading(true)
    try {
      await onSubmit({
        id: artifact.local_id,
        type: artifact.type,
        version,
        name,
        description,
        tags,
        author: author || undefined,
        path: artifact.path || undefined,
        global: scope === 'global',
        dependencies: deps.filter((d) => d.id),
      })
    } finally {
      setLoading(false)
    }
  }

  const addDep = () => setDeps((d) => [...d, { type: '', id: '', version: 'latest' }])
  const removeDep = (i: number) => setDeps((d) => d.filter((_, idx) => idx !== i))
  const updateDep = (i: number, field: keyof Dep, value: string) =>
    setDeps((d) => d.map((dep, idx) => (idx === i ? { ...dep, [field]: value } : dep)))

  return (
    <div className="fixed inset-0 z-[9000] flex items-center justify-center backdrop-blur-md bg-black/40 animate-fade-in overflow-y-auto py-8">
      <div
        className="glass-panel bg-card/85 border border-border/50 rounded-2xl p-6 md:p-7 w-full max-w-lg shadow-2xl relative overflow-hidden my-auto"
        onClick={(e) => e.stopPropagation()}
      >
        {}
        <div className="absolute -top-12 -left-12 w-24 h-24 bg-primary/10 rounded-full blur-2xl pointer-events-none" />

        <div className="flex items-center justify-between mb-2 relative z-10">
          <h2 className="text-lg font-heading font-bold text-foreground">
            {isUpdate ? 'Update on Hub' : 'Submit to Hub'}
          </h2>
          <button onClick={onClose} className="p-1.5 rounded-xl hover:bg-accent/50 text-muted-foreground hover:text-foreground transition-all">
            <X className="w-4 h-4" />
          </button>
        </div>
        <p className="text-sm text-muted-foreground mb-5 relative z-10 leading-relaxed">
          {isUpdate
            ? `Update "${artifact.local_id}" (${artifact.type}) in the registry.`
            : `Publish "${artifact.local_id}" (${artifact.type}) to the registry.`}
        </p>

        {}
        <div className="mb-4 relative z-10">
          <label className="block text-xs font-semibold text-muted-foreground mb-1.5">Scope</label>
          <div className="flex gap-2.5">
            <button
              onClick={() => setScope('global')}
              className={cn(
                'flex-1 py-2.5 rounded-xl text-xs font-semibold border transition-all duration-200 flex items-center justify-center gap-1.5 hover:scale-[1.01]',
                scope === 'global'
                  ? 'bg-primary/10 text-primary border-primary/30 shadow-sm'
                  : 'bg-background/40 hover:bg-accent/45 border-border/40 text-muted-foreground hover:text-foreground',
              )}
            >
              <Globe className="w-3.5 h-3.5" /> Global Registry
            </button>
            <button
              onClick={() => setScope('project')}
              className={cn(
                'flex-1 py-2.5 rounded-xl text-xs font-semibold border transition-all duration-200 flex items-center justify-center gap-1.5 hover:scale-[1.01]',
                scope === 'project'
                  ? 'bg-primary/10 text-primary border-primary/30 shadow-sm'
                  : 'bg-background/40 hover:bg-accent/45 border-border/40 text-muted-foreground hover:text-foreground',
              )}
            >
              <Folder className="w-3.5 h-3.5" /> Current Project
            </button>
          </div>
        </div>

        <div className="grid grid-cols-2 gap-3 mb-3 relative z-10">
          <FormField label="Display Name">
            <input
              value={name}
              onChange={(e) => setName(e.target.value)}
              placeholder="My Artifact"
              className={inputCls}
            />
          </FormField>
          <FormField label="Version">
            <input
              value={version}
              onChange={(e) => setVersion(e.target.value)}
              placeholder="1.0.0"
              className={inputCls}
            />
          </FormField>
        </div>

        <FormField label="Description" className="mb-3 relative z-10">
          <textarea
            value={description}
            onChange={(e) => setDescription(e.target.value)}
            placeholder="A short description..."
            rows={2}
            className={cn(inputCls, 'resize-y')}
          />
        </FormField>

        <FormField label="Tags (comma-separated)" className="mb-3 relative z-10">
          <input
            value={tags}
            onChange={(e) => setTags(e.target.value)}
            placeholder="typescript, best-practices"
            className={inputCls}
          />
        </FormField>

        <div className="mb-3 relative z-10">
          <FormField label="Author">
            <input
              value={author}
              onChange={(e) => setAuthor(e.target.value)}
              placeholder="johndoe"
              className={inputCls}
            />
          </FormField>
        </div>

        <div className="mb-5 relative z-10">
          <label className="block text-xs font-semibold text-muted-foreground mb-1.5">Dependencies</label>
          <div className="space-y-1.5 max-h-32 overflow-y-auto pr-1">
            {deps.map((dep, i) => (
              <div key={i} className="grid grid-cols-[100px_1fr_80px_28px] gap-1.5 items-center animate-fade-in">
                <select
                  value={dep.type}
                  onChange={(e) => updateDep(i, 'type', e.target.value)}
                  className={cn(inputCls, 'text-xs py-1.5')}
                >
                  <option value="">type</option>
                  {TYPES.map((t) => <option key={t} value={t}>{t}</option>)}
                </select>
                <input
                  value={dep.id}
                  onChange={(e) => updateDep(i, 'id', e.target.value)}
                  placeholder="artifact-id"
                  className={cn(inputCls, 'text-xs py-1.5')}
                />
                <input
                  value={dep.version}
                  onChange={(e) => updateDep(i, 'version', e.target.value)}
                  placeholder="latest"
                  className={cn(inputCls, 'text-xs py-1.5')}
                />
                <button onClick={() => removeDep(i)} className="p-1 rounded-lg text-destructive hover:bg-destructive/5 flex items-center justify-center">
                  <Trash2 className="w-3.5 h-3.5" />
                </button>
              </div>
            ))}
          </div>
          <button
            onClick={addDep}
            className="flex items-center gap-1.5 text-xs text-muted-foreground hover:text-foreground mt-2 px-2.5 py-1 rounded-xl border border-dashed border-border/80 hover:border-foreground/50 transition-colors"
          >
            <Plus className="w-3 h-3" /> Add Dependency
          </button>
        </div>

        <div className="flex justify-end gap-2 relative z-10 pt-2">
          <button
            onClick={onClose}
            className="px-4 py-2 rounded-xl border border-border/50 text-sm font-semibold hover:bg-accent/40 transition-colors"
          >
            Cancel
          </button>
          <button
            onClick={handleSubmit}
            disabled={loading}
            className="px-4 py-2 rounded-xl text-sm font-semibold hover:scale-[1.01] transition-all btn-premium"
          >
            {loading ? 'Submitting...' : isUpdate ? 'Update' : 'Submit'}
          </button>
        </div>
      </div>
    </div>
  )
}

const inputCls =
  'w-full px-3.5 py-2.5 rounded-xl border border-border/50 bg-background/50 backdrop-blur-sm text-sm text-foreground outline-none focus:ring-2 focus:ring-primary/20 focus:border-primary/80 transition-all duration-200'

function FormField({
  label,
  children,
  className,
}: {
  label: string
  children: React.ReactNode
  className?: string
}) {
  return (
    <div className={className}>
      <label className="block text-xs font-semibold text-muted-foreground mb-1.5">{label}</label>
      {children}
    </div>
  )
}
