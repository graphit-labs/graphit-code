import { useState, useRef, useCallback, useEffect } from 'react'
import { useAppStore } from '@/store/appStore'
import { showToast } from '@/hooks/useToast'
import { hubApi } from '@/api/hub'
import { CloudUpload, Plus, Trash2, Globe, FolderOpen, Wand2 } from 'lucide-react'
import { cn } from '@/lib/utils'

const TYPES = ['rule', 'skill', 'agent', 'command', 'knowledge', 'ast', 'mcp', 'power', 'language', 'framework']

type UploadScope = 'global' | 'project'

interface Dep { type: string; id: string; version: string }

export default function UploadPage() {
  const { webMode, activeIde, activeProjectDir } = useAppStore()

  const [file, setFile] = useState<File | null>(null)
  const [scope, setScope] = useState<UploadScope>('project')
  const [artifactId, setArtifactId] = useState('')
  const [name, setName] = useState('')
  const [version, setVersion] = useState('1.0.0')
  const [type, setType] = useState('')
  const [description, setDescription] = useState('')
  const [tags, setTags] = useState('')
  const [author, setAuthor] = useState('')
  const [deps, setDeps] = useState<Dep[]>([])
  const [loading, setLoading] = useState(false)
  const [dragging, setDragging] = useState(false)
  const fileRef = useRef<HTMLInputElement>(null)

  useEffect(() => {
    if (!webMode) {
      hubApi.getGitAuthor().then((d) => setAuthor(d.author)).catch(() => {})
    }
  }, [webMode])

  const handleFileDrop = useCallback((e: React.DragEvent) => {
    e.preventDefault()
    setDragging(false)
    const f = e.dataTransfer.files[0]
    if (f && f.name.endsWith('.zip')) setFile(f)
    else showToast('Only .zip files are accepted', 'error')
  }, [])

  const handleFileSelect = (e: React.ChangeEvent<HTMLInputElement>) => {
    const f = e.target.files?.[0]
    if (f) setFile(f)
  }

  const addDep = () => setDeps((d) => [...d, { type: '', id: '', version: 'latest' }])
  const removeDep = (i: number) => setDeps((d) => d.filter((_, idx) => idx !== i))
  const updateDep = (i: number, field: keyof Dep, value: string) =>
    setDeps((d) => d.map((dep, idx) => (idx === i ? { ...dep, [field]: value } : dep)))

  const handleSubmit = async () => {
    const isPower = type === 'power'
    if (!isPower && !file) { showToast('Please select a .zip file', 'error'); return }
    if (!artifactId && !type) { showToast('Artifact ID or type is required', 'error'); return }
    setLoading(true)
    try {
      const formData = new FormData()
      if (file) formData.append('file', file)
      formData.append('id', artifactId)
      formData.append('type', type)
      formData.append('version', version)
      formData.append('name', name)
      formData.append('description', description)
      formData.append('tags', tags)
      formData.append('author', author)
      formData.append('scope', scope)
      formData.append('ide', activeIde)
      formData.append('dependencies', JSON.stringify(deps.filter((d) => d.id)))
      if (activeProjectDir) formData.append('project_dir', activeProjectDir)

      const result = await hubApi.upload(formData)
      if (result.success) {
        showToast('Upload successful!', 'success')
        setFile(null)
        setArtifactId('')
        setName('')
        setDescription('')
        setTags('')
        setDeps([])
      } else {
        showToast(`Upload failed: ${result.error ?? 'unknown'}`, 'error')
      }
    } catch { showToast('Upload failed', 'error') }
    finally { setLoading(false) }
  }

  const inputCls = 'w-full px-3.5 py-2.5 rounded-xl border border-border/50 bg-background/50 backdrop-blur-sm text-sm text-foreground outline-none focus:ring-2 focus:ring-primary/20 focus:border-primary/80 transition-all duration-200'

  return (
    <div className="w-full max-w-3xl mx-auto px-4 md:px-8 py-10 relative animate-in fade-in duration-300">
      <div className="absolute -top-24 -right-24 w-72 h-72 bg-primary/5 rounded-full blur-3xl pointer-events-none" />

      <div className="flex items-center gap-4 mb-8 pb-6 border-b border-border/40 relative z-10">
        <div className="w-12 h-12 rounded-2xl bg-primary/10 border border-primary/20 flex items-center justify-center shrink-0">
          <CloudUpload className="w-6 h-6 text-primary" />
        </div>
        <div>
          <h1 className="text-3xl font-heading font-bold tracking-tight text-foreground">Upload Artifact</h1>
          <p className="text-[14px] text-muted-foreground mt-1 leading-relaxed">
            Upload a <strong className="text-foreground font-semibold">.zip</strong> file to publish a new artifact or update an existing one. Choose scope below.
          </p>
        </div>
      </div>

      <div className="glass-panel rounded-2xl p-6 md:p-8 space-y-6 relative z-10">
        {type === 'power' && (
          <div className="flex items-center gap-3 p-4 rounded-xl bg-primary/5 border border-primary/20 animate-fade-in">
            <Wand2 className="w-5 h-5 text-primary shrink-0 animate-pulse" />
            <div>
              <strong className="text-sm font-semibold text-foreground">Power — Virtual Artifact</strong>
              <p className="text-xs text-muted-foreground mt-0.5">Dependency packages with no files. Fill metadata and dependencies below.</p>
            </div>
          </div>
        )}

        {}
        <div
          className={cn(
            'relative border border-dashed rounded-2xl p-10 text-center cursor-pointer transition-all duration-300',
            dragging ? 'border-primary/60 bg-primary/5 scale-[1.01]' : 'border-border/60 hover:border-primary/40 hover:bg-accent/30',
            type === 'power' && 'hidden',
          )}
          onDragOver={(e) => { e.preventDefault(); setDragging(true) }}
          onDragLeave={() => setDragging(false)}
          onDrop={handleFileDrop}
          onClick={() => fileRef.current?.click()}
        >
          <div className="w-12 h-12 rounded-xl bg-accent/40 flex items-center justify-center mx-auto mb-4 border border-border/50 shadow-inner group-hover:scale-105 transition-transform">
            <CloudUpload className="w-6 h-6 text-muted-foreground opacity-80" />
          </div>
          <p className="text-[14px] text-muted-foreground">
            Drag & drop your <strong className="text-foreground font-semibold">.zip</strong> file here, or <span className="text-primary font-semibold hover:underline">browse files</span>
          </p>
          {file && (
            <p className="mt-4 text-xs font-semibold text-emerald-500 bg-emerald-500/10 border border-emerald-500/20 px-3 py-1.5 rounded-lg inline-flex items-center gap-1.5 shadow-sm">
              Selected: {file.name}
            </p>
          )}
          <input ref={fileRef} type="file" accept=".zip" onChange={handleFileSelect} className="hidden" />
        </div>

        {}
        <div>
          <label className="block text-[10px] font-bold uppercase tracking-widest text-muted-foreground/60 mb-2.5">Scope</label>
          <div className="flex gap-3">
            {(['global', 'project'] as UploadScope[]).map((s) => (
              <button
                key={s}
                onClick={() => setScope(s)}
                className={cn(
                  'flex-1 flex items-center justify-center gap-2 py-3 rounded-xl text-[13px] font-semibold border transition-all duration-200',
                  scope === s
                    ? 'bg-primary/10 text-primary border-primary/30 shadow-[0_2px_12px_rgba(0,0,0,0.02)]'
                    : 'bg-background/40 hover:bg-accent/45 border-border/40 text-muted-foreground hover:text-foreground',
                )}
              >
                {s === 'global'
                  ? <><Globe className="w-4 h-4" /> Global Registry</>
                  : <><FolderOpen className="w-4 h-4" /> Current Project</>}
              </button>
            ))}
          </div>
        </div>

        {}
        <div className="space-y-4">
          <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
            <div>
              <label className="block text-xs font-semibold text-muted-foreground mb-1.5">Artifact ID</label>
              <input value={artifactId} onChange={(e) => setArtifactId(e.target.value)} placeholder="my-custom-rule" className={inputCls} />
            </div>
            <div>
              <label className="block text-xs font-semibold text-muted-foreground mb-1.5">Type</label>
              <select value={type} onChange={(e) => setType(e.target.value)} className={inputCls}>
                <option value="">— Select type —</option>
                {TYPES.map((t) => <option key={t} value={t}>{t}</option>)}
              </select>
            </div>
          </div>

          <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
            <div>
              <label className="block text-xs font-semibold text-muted-foreground mb-1.5">Display Name</label>
              <input value={name} onChange={(e) => setName(e.target.value)} placeholder="My Custom Rule" className={inputCls} />
            </div>
            <div>
              <label className="block text-xs font-semibold text-muted-foreground mb-1.5">Version</label>
              <input value={version} onChange={(e) => setVersion(e.target.value)} placeholder="1.0.0" className={inputCls} />
            </div>
          </div>

          <div>
            <label className="block text-xs font-semibold text-muted-foreground mb-1.5">Description</label>
            <textarea value={description} onChange={(e) => setDescription(e.target.value)} placeholder="A short description..." rows={2} className={cn(inputCls, 'resize-y')} />
          </div>

          <div>
            <label className="block text-xs font-semibold text-muted-foreground mb-1.5">Tags (comma-separated)</label>
            <input value={tags} onChange={(e) => setTags(e.target.value)} placeholder="typescript, best-practices" className={inputCls} />
          </div>

          <div>
            <label className="block text-xs font-semibold text-muted-foreground mb-1.5">Author</label>
            <input value={author} onChange={(e) => setAuthor(e.target.value)} placeholder="johndoe" className={inputCls} disabled={webMode} />
          </div>
        </div>

        {}
        <div className="pt-2">
          <label className="block text-xs font-semibold text-muted-foreground mb-2.5">Dependencies</label>
          <div className="space-y-2">
            {deps.map((dep, i) => (
              <div key={i} className="grid grid-cols-[110px_1fr_90px_32px] gap-2 items-center animate-fade-in">
                <select value={dep.type} onChange={(e) => updateDep(i, 'type', e.target.value)} className={cn(inputCls, 'text-xs py-2')}>
                  <option value="">type</option>
                  {TYPES.map((t) => <option key={t} value={t}>{t}</option>)}
                </select>
                <input value={dep.id} onChange={(e) => updateDep(i, 'id', e.target.value)} placeholder="artifact-id" className={cn(inputCls, 'text-xs py-2')} />
                <input value={dep.version} onChange={(e) => updateDep(i, 'version', e.target.value)} placeholder="latest" className={cn(inputCls, 'text-xs py-2')} />
                <button onClick={() => removeDep(i)} className="p-2 rounded-xl text-destructive hover:bg-destructive/5 transition-colors flex items-center justify-center shrink-0">
                  <Trash2 className="w-4 h-4" />
                </button>
              </div>
            ))}
          </div>
          <button onClick={addDep} className="flex items-center gap-1.5 text-xs text-muted-foreground hover:text-foreground px-3 py-1.5 rounded-xl border border-dashed border-border/80 hover:border-foreground/50 transition-colors mt-3">
            <Plus className="w-3.5 h-3.5" /> Add Dependency
          </button>
        </div>

        <div className="pt-4 flex justify-end">
          <button
            onClick={handleSubmit}
            disabled={loading}
            className="flex items-center gap-2 px-6 py-3 rounded-xl bg-primary text-primary-foreground font-semibold text-sm hover:bg-primary/95 disabled:opacity-50 transition-all hover:scale-[1.02] shadow-md btn-premium"
          >
            <CloudUpload className="w-4 h-4" />
            {loading ? 'Uploading...' : 'Upload & Publish'}
          </button>
        </div>
      </div>
    </div>
  )
}
