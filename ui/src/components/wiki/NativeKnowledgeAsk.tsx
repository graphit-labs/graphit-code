import { useEffect, useRef, useState } from 'react'
import { nativeKnowledgeApi, type NativeKnowledgeAskInput, type NativeKnowledgeAskProgress, type NativeKnowledgeAskResult, type NativeKnowledgeHead, type NativeKnowledgeSearchMetric, type NativeKnowledgeSearchMode } from '@/api/nativeKnowledge'
import { nativeSelectionKey } from '@/api/nativeRecordNavigation'
import { parseNativeJson, stringifyNativeJson, type NativeInteger } from '@/api/nativeJson'
import type { TaskContext } from '@/api/task'
import { usePageRefresh } from '@/components/layout/WorkspaceRefresh'
import { FactList, WorkEmpty, WorkNotice, WorkSection } from '@/components/shared/EngineeringUI'
import { StyledSelect } from '@/components/shared/StyledSelect'
import { MarkdownContent } from './WikiMarkdown'

export function NativeKnowledgeAsk({ selection, heads }: { selection: TaskContext; heads: NativeKnowledgeHead[] | null }) {
  const binding = nativeSelectionKey(selection)
  const [query, setQuery] = useState(''), [model, setModel] = useState('')
  const [evidence, setEvidence] = useState('selected'), [slugs, setSlugs] = useState<string[]>([])
  const [mode, setMode] = useState<NativeKnowledgeSearchMode>('fts'), [metric, setMetric] = useState<NativeKnowledgeSearchMetric>('cosine')
  const [limit, setLimit] = useState('5')
  const [reply, setReply] = useState<{ binding: string; value: NativeKnowledgeAskResult } | null>(null)
  const [progress, setProgress] = useState<{ binding: string; events: NativeKnowledgeAskProgress[] } | null>(null)
  const [failure, setFailure] = useState<{ binding: string; message: string } | null>(null)
  const [pending, setPending] = useState<string | null>(null)
  const [captured, setCaptured] = useState<{ binding: string; input: NativeKnowledgeAskInput } | null>(null)
  const controller = useRef<AbortController | null>(null), request = useRef(0)
  const clear = () => {
    request.current += 1; controller.current?.abort()
    setReply(null); setProgress(null); setFailure(null); setPending(null); setCaptured(null)
  }
  useEffect(() => () => { request.current += 1; controller.current?.abort() }, [binding])
  usePageRefresh(async () => { clear() })
  const run = async (input: NativeKnowledgeAskInput) => {
    controller.current?.abort()
    const current = new AbortController(); controller.current = current
    const id = ++request.current
    setCaptured({ binding, input }); setReply(null); setFailure(null); setProgress({ binding, events: [] }); setPending(binding)
    try {
      const value = await nativeKnowledgeApi.ask(selection, input, { signal: current.signal, onProgress: event => {
        if (id === request.current && !current.signal.aborted) setProgress(previous => ({ binding, events: [...(previous?.binding === binding ? previous.events : []), event] }))
      } })
      if (id === request.current && !current.signal.aborted) setReply({ binding, value })
    } catch (cause) {
      if (id === request.current && !current.signal.aborted) setFailure({ binding, message: cause instanceof Error ? cause.message : String(cause) })
    } finally { if (id === request.current) setPending(null) }
  }
  const submit = () => {
    if (evidence === 'selected') { void run({ query, model, selected_slugs: slugs }); return }
    if (!/^[1-9]\d*$/.test(limit)) { clear(); setFailure({ binding, message: 'Use a positive whole number for retrieved pages.' }); return }
    const top_k = parseNativeJson<NativeInteger>(limit)
    void run({ query, model, retrieval: mode === 'fts' ? { mode, top_k } : { mode, top_k, metric } })
  }
  const cancel = () => {
    request.current += 1; controller.current?.abort(); setReply(null); setProgress(null); setPending(null)
    setFailure({ binding, message: 'Question request stopped.' })
  }
  const result = reply?.binding === binding ? reply.value : null
  const events = progress?.binding === binding ? progress.events : []
  const error = failure?.binding === binding ? failure.message : ''
  const loading = pending === binding
  const answer = result?.status === 'answer' && typeof result.receipt?.completion?.answer === 'string' ? result.receipt.completion.answer : null
  return <WorkSection title="Ask Knowledge">
    <form className="knowledge-ask-form" onSubmit={event => { event.preventDefault(); submit() }}>
      <label className="work-field knowledge-ask-question">Question<textarea aria-label="Knowledge question" value={query} onChange={event => { clear(); setQuery(event.target.value) }} /></label>
      <label className="work-field">Requested model<input aria-label="Knowledge requested model" value={model} onChange={event => { clear(); setModel(event.target.value) }} /></label>
      <label className="work-field">Evidence<StyledSelect aria-label="Knowledge question evidence" value={evidence} onChange={event => { clear(); setEvidence(event.target.value) }}><option value="selected">Selected pages</option><option value="retrieval">Retrieve pages</option></StyledSelect></label>
      {evidence === 'selected' ? <fieldset className="knowledge-ask-evidence"><legend>Pages to provide</legend>
        <div className="knowledge-ask-pages">
          {heads === null ? <p>Load the published pages to select evidence.</p> : heads.length === 0 ? <p>No published pages to select.</p> : heads.map(head => <label className="knowledge-ask-page" key={head.slug}><input type="checkbox" checked={slugs.includes(head.slug)} onChange={event => { clear(); setSlugs(previous => event.target.checked ? [...previous, head.slug] : previous.filter(slug => slug !== head.slug)) }} /><span><span className="knowledge-ask-page-title">{head.title || head.slug}</span><small>{head.slug}</small></span></label>)}
        </div>
        <p className="work-help">An empty selection provides no pages. Choose retrieval to search for evidence.</p>
      </fieldset> : <>
        <label className="work-field">Retrieval mode<StyledSelect aria-label="Knowledge question retrieval mode" value={mode} onChange={event => { clear(); setMode(event.target.value as NativeKnowledgeSearchMode) }}><option value="fts">fts</option><option value="semantic">semantic</option><option value="hybrid">hybrid</option></StyledSelect></label>
        {mode !== 'fts' && <label className="work-field">Distance metric<StyledSelect aria-label="Knowledge question distance metric" value={metric} onChange={event => { clear(); setMetric(event.target.value as NativeKnowledgeSearchMetric) }}><option value="cosine">cosine</option><option value="l2">l2</option><option value="dot">dot</option></StyledSelect></label>}
        <label className="work-field">Maximum pages<input inputMode="numeric" aria-label="Knowledge question page limit" value={limit} onChange={event => { clear(); setLimit(event.target.value) }} /></label>
      </>}
      <div className="work-actions knowledge-ask-actions">
        <button className="work-button primary" disabled={!query.trim() || !model.trim() || loading} type="submit">{loading ? 'Asking…' : 'Ask question'}</button>
        {loading && <button className="work-button" type="button" onClick={cancel}>Cancel question</button>}
      </div>
    </form>
    {loading && <p role="status">Waiting for the question result. {events.length} progress events received.</p>}
    {error && <WorkNotice title="Unable to complete question" tone="error">{error}{captured?.binding === binding && <button className="work-button" onClick={() => void run(captured.input)}>Retry question</button>}</WorkNotice>}
    {events.length > 0 && <details className="work-disclosure"><summary>Question progress</summary><pre className="work-code">{events.map(event => stringifyNativeJson(event)).join('\n')}</pre></details>}
    {result && <>
      <FactList items={[["Question status", result.status], ["Source publication", result.generation?.version], ["Requested model", result.receipt?.completion?.requested_model]]} />
      {answer !== null ? <article aria-label="Knowledge answer"><MarkdownContent content={answer} /></article> : <WorkEmpty title={'Question result: ' + result.status} />}
      {(result.error || result.execution_error || result.delivery_error || result.category) && <WorkNotice title="Question execution details" tone="error">{[result.error, result.execution_error, result.delivery_error, result.category].filter(Boolean).join(' · ')}</WorkNotice>}
      {result.provided_slugs && <div><p>Pages provided to the model</p><ul>{result.provided_slugs.map((slug, index) => <li key={index}>{slug}</li>)}</ul></div>}
      <details className="work-disclosure"><summary>Question receipt</summary><pre className="work-code">{stringifyNativeJson(result, 2)}</pre></details>
    </>}
  </WorkSection>
}
