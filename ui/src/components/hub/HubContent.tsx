import { useEffect, useRef, useState } from 'react'
import { hubNative, type FileContent, type LifecycleSelection, type RemoteSelection } from '@/api/hubNative'
import { WorkSection } from '@/components/shared/EngineeringUI'
import { HubFailure } from './HubFeedback'

export function HubContent({ remote, installed }: { remote?: RemoteSelection; installed?: LifecycleSelection & { version: string } }) {
  const [path, setPath] = useState('')
  const [content, setContent] = useState<FileContent | null>(null)
  const [error, setError] = useState<unknown>(null)
  const [loading, setLoading] = useState(false)
  const pending = useRef<AbortController | null>(null)
  const reader = useRef<HTMLElement>(null), trigger = useRef<HTMLButtonElement>(null)
  useEffect(() => () => pending.current?.abort(), [])
  useEffect(() => { if (content) reader.current?.focus() }, [content])
  async function read(event: React.FormEvent) {
    event.preventDefault()
    if (!path.trim() || (!remote && !installed)) return
    pending.current?.abort()
    const controller = new AbortController(); pending.current = controller
    setContent(null); setError(null); setLoading(true)
    try {
      const request = remote ? { remote: { selection: structuredClone(remote), path: path.trim() } } : { installed: { ...structuredClone(installed!), path: path.trim() } }
      const result = await hubNative.content(request, controller.signal)
      if (!controller.signal.aborted) setContent(result)
    } catch (cause) { if (!controller.signal.aborted) setError(cause) }
    finally { if (!controller.signal.aborted) setLoading(false) }
  }
  return <WorkSection title="Read a package file">
    <form className="work-form" onSubmit={read}><label className="work-field"><span>Relative file path</span><input required value={path} onChange={event => setPath(event.target.value)} placeholder="SKILL.md" /></label><button ref={trigger} className="work-button" disabled={loading || !path.trim()}>{loading ? 'Reading file…' : 'Read file'}</button></form>
    {loading && <p role="status">Reading the selected file…</p>}
    {error !== null && <HubFailure error={error} />}
    {content && <section ref={reader} tabIndex={-1} aria-label="Package file content" className="hub-file" onKeyDown={event => { if (event.key === 'Escape') { setContent(null); trigger.current?.focus() } }}><p>{content.content.path} · {content.version} · {content.cache === 'ephemeral' ? 'Temporary remote read' : 'Verified installed cache'}</p><pre><code>{content.content.content}</code></pre></section>}
  </WorkSection>
}
