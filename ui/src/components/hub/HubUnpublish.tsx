import { useEffect, useId, useLayoutEffect, useRef, useState, type ReactNode } from 'react'
import { hubNative, HubUnpublishError, type HubUnpublishInput, type HubUnpublishEffects } from '@/api/hubNative'
import { FactList, WorkNotice, WorkSection } from '@/components/shared/EngineeringUI'
import { ModalPortal } from '@/components/shared/ModalPortal'

type Result = { epoch: number; request: HubUnpublishInput; effects: HubUnpublishEffects | null; error: unknown; inspection: boolean }
type Intent = { context: string; request: HubUnpublishInput }
type Controls = { open: (request: HubUnpublishInput) => void; disabled: boolean; reason: string }
function failure(error: unknown) {
  if (!(error instanceof HubUnpublishError)) return 'The result could not be observed. Inspect the original catalogue before another attempt.'
  if (error.status === 401 || error.status === 403) return 'Unpublication access is unavailable. Recorded catalogue effects still apply.'
  if (error.status === 409) return 'The catalogue changed or the expected archive digest did not match. Inspect the original catalogue before retrying.'
  if (error.status === 413) return 'The full result exceeded the response limit. Inspect the original catalogue before another attempt.'
  if (error.status === 499) return 'Unpublication was cancelled. Review the recorded catalogue effects.'
  if (error.status === 504) return 'The deadline expired. Review the recorded catalogue effects.'
  if (error.status === 503) return 'The service became unavailable. Review the recorded catalogue effects.'
  if (error.status === 404) return 'The selected remote catalogue was not found.'
  if (error.status === 400) return 'The request was rejected. Your entered values are retained.'
  if (error.status === 501) return 'This provider does not support the requested unpublication.'
  return 'Unpublication did not complete. Review the recorded catalogue effects.'
}
function TargetFacts({ request }: { request: HubUnpublishInput }) {
  return <FactList items={[[ 'Provider', request.provider ], ['Producer project', request.producer], ['Artifact type', request.artifact_type], ['Identifier', request.id], ['Target', request.target.kind === 'artifact' ? 'Whole artifact: all published versions' : request.target.version], ['Expected archive SHA256', request.target.kind === 'version' ? request.target.expected_sha256 : null]]} />
}
export function HubUnpublish({ contextKey, onCompleted, children }: { contextKey: string; onCompleted: () => Promise<void>; children: (controls: Controls) => ReactNode }) {
  const [intent, setIntent] = useState<Intent | null>(null), [pending, setPending] = useState(false)
  const [result, setResult] = useState<Result | null>(null)
  const [inspected, setInspected] = useState<{ epoch: number; text: string } | null>(null)
  const [inspecting, setInspecting] = useState(false), [inspectionError, setInspectionError] = useState(false)
  const work = useRef<AbortController | null>(null), read = useRef<AbortController | null>(null)
  const busy = useRef(false), epoch = useRef(0), alive = useRef(true)
  const resultRegion = useRef<HTMLDivElement>(null)
  useEffect(() => { if (result && !result.error && !pending && !intent) resultRegion.current?.focus() }, [result, pending, intent])
  const current = useRef({ contextKey, onCompleted })
  useLayoutEffect(() => { current.current = { contextKey, onCompleted } }, [contextKey, onCompleted])
  useEffect(() => { alive.current = true; return () => { alive.current = false; work.current?.abort(); read.current?.abort() } }, [])
  if (intent && intent.context !== contextKey) setIntent(null)
  const blocked = !!result?.inspection && inspected?.epoch !== result.epoch
  async function submit(input: HubUnpublishInput) {
    if (busy.current || blocked) return
    busy.current = true
    const request = structuredClone(input), originalContext = contextKey, controller = new AbortController(), generation = ++epoch.current
    work.current = controller; read.current?.abort()
    setPending(true); setInspected(null); setInspecting(false); setInspectionError(false)
    setResult({ epoch: generation, request, effects: null, error: null, inspection: false })
    let completed = false
    try {
      const outcome = await hubNative.unpublish(request, controller.signal)
      if (!alive.current || generation !== epoch.current) return
      setResult({ epoch: generation, request, effects: outcome, error: null, inspection: false })
      setIntent(null); completed = true
    } catch (error) {
      if (!alive.current || generation !== epoch.current) return
      setResult({ epoch: generation, request, effects: error instanceof HubUnpublishError ? error.effects : null, error,
        inspection: !(error instanceof HubUnpublishError) || error.inspectionRequired || error.status === 409 })
    } finally {
      if (alive.current && generation === epoch.current) { busy.current = false; setPending(false) }
    }
    if (completed && alive.current && current.current.contextKey === originalContext) {
      // A refresh failure must not rewrite an already confirmed remote receipt.
      try { await current.current.onCompleted() } catch { /* The catalogue loader owns its read error. */ }
    }
  }
  async function inspect() {
    if (!result || busy.current || inspecting) return
    const captured = result, controller = new AbortController(), request = captured.request
    read.current?.abort(); read.current = controller; setInspecting(true); setInspectionError(false)
    try {
      const page = await hubNative.search({ transport: 'broker', provider: request.provider, producer: request.producer, artifact_type: request.artifact_type, id: request.id, limit: 20 }, controller.signal)
      if (!alive.current || controller.signal.aborted || epoch.current !== captured.epoch) return
      if (page.next_cursor) throw new Error('Incomplete exact catalogue inspection')
      const found = page.artifacts.find(item => item.artifact_type === request.artifact_type && item.id === request.id)
      setInspected({ epoch: captured.epoch, text: found
        ? 'Original catalogue versions: ' + found.versions.join(', ') + '. Declared latest: ' + (found.latest ?? 'not declared') + '.'
        : 'The original artifact is absent from the remote catalogue. Payloads, source and existing installations are preserved.' })
    } catch { if (alive.current && !controller.signal.aborted && epoch.current === captured.epoch) setInspectionError(true) }
    finally { if (alive.current && !controller.signal.aborted && epoch.current === captured.epoch) setInspecting(false) }
  }
  const feedback = result && <div ref={resultRegion} role="region" aria-label="Unpublication result" tabIndex={-1}><WorkSection title="Unpublication result">
    <TargetFacts request={result.request} />
    {pending ? <><p role="status">Unpublishing the captured remote target…</p><button type="button" className="work-button" onClick={() => work.current?.abort()}>Cancel unpublication</button></>
      : <WorkNotice title={result.error ? 'Unpublication not completed' : 'Artifact unpublished'} tone={result.error ? 'error' : 'success'}>{result.error ? failure(result.error) : 'The remote catalogue was retracted. Payloads, source and existing installations are preserved.'}</WorkNotice>}
    {result.error instanceof HubUnpublishError && !result.error.receiptCorrelated && <p role="alert">The returned receipt does not match the captured request. Its catalogue effects cannot be attributed to this target; inspect the original catalogue.</p>}
    {result.effects && !(result.error instanceof HubUnpublishError && !result.error.receiptCorrelated) && <FactList items={[[ 'Catalogue effect', result.effects.retraction?.catalog ], ['Retracted versions', Array.isArray(result.effects.retraction?.targeted_versions) ? result.effects.retraction.targeted_versions.join(', ') : null], ['Declared latest after retraction', result.effects.retraction?.desired_latest === null ? 'Not declared' : result.effects.retraction?.desired_latest], ['Stage', result.effects.stage], ['Cause', result.effects.cause], ['Observation', result.effects.observation_failed ? 'Not confirmed' : null]]} />}
    {!pending && <><button type="button" className="work-button" disabled={inspecting} onClick={() => { void inspect() }}>{inspecting ? 'Inspecting original catalogue…' : 'Inspect original catalogue'}</button>
      {blocked && <p>Inspect this original result before another unpublication. No operation is retried automatically.</p>}
      {inspectionError && <p role="alert">The original catalogue could not be inspected. Retry the inspection explicitly.</p>}
      {inspected?.epoch === result.epoch && <p role="status">{inspected.text}</p>}</>}
  </WorkSection></div>
  return <>
    {children({ open: request => { if (!pending && !blocked) setIntent({ context: contextKey, request: structuredClone(request) }) }, disabled: pending || blocked, reason: pending ? 'An unpublication is already pending.' : blocked ? 'Inspect the original unpublication result first.' : '' })}
    {intent ? <UnpublishDialog key={JSON.stringify(intent.request)} request={intent.request} pending={pending} blocked={blocked} onSubmit={submit} onClose={() => setIntent(null)}>{feedback}</UnpublishDialog> : feedback}
  </>
}
function UnpublishDialog({ request, pending, blocked, onSubmit, onClose, children }: { request: HubUnpublishInput; pending: boolean; blocked: boolean; onSubmit: (input: HubUnpublishInput) => Promise<void>; onClose: () => void; children: ReactNode }) {
  const title = useId(), [hash, setHash] = useState(''), [confirmed, setConfirmed] = useState(false)
  const whole = request.target.kind === 'artifact'
  return <ModalPortal onClose={onClose}><div className="work-modal-backdrop"><form className="work-dialog" role="dialog" aria-modal="true" aria-labelledby={title} onSubmit={event => {
    event.preventDefault()
    if (pending || blocked || !confirmed) return
    void onSubmit({ ...request, target: request.target.kind === 'version' ? { ...request.target, ...(hash ? { expected_sha256: hash } : {}) } : request.target })
  }}>
    <header><h2 id={title}>{whole ? 'Unpublish whole artifact' : 'Unpublish version'}</h2></header>
    <div className="work-dialog-body work-form">
      <TargetFacts request={request} />
      <p>{whole ? 'Remove this artifact and all its published versions from the remote catalogue.' : 'Remove this exact version from the remote catalogue. If it is latest, latest becomes absent; another version is never chosen automatically.'} Payloads, source and existing installations are preserved.</p>
      {!whole && <label className="work-field"><span>Expected archive SHA256 (optional)</span><input pattern="[a-f0-9]{64}" value={hash} onChange={event => setHash(event.target.value)} disabled={pending} /></label>}
      <label><input type="checkbox" checked={confirmed} disabled={pending} onChange={event => setConfirmed(event.target.checked)} /> {whole ? 'I confirm unpublishing this whole artifact and every published version.' : 'I confirm unpublishing this exact version.'}</label>
      {children}
    </div>
    <footer><button type="button" className="work-button" onClick={onClose}>{pending ? 'Close' : 'Cancel'}</button><button className="work-button danger" disabled={pending || blocked || !confirmed}>{pending ? 'Unpublishing…' : whole ? 'Unpublish whole artifact' : 'Unpublish version'}</button></footer>
  </form></div></ModalPortal>
}
