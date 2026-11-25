import { useEffect, useRef, useState } from 'react'
import { hubNative, HubPublicationError, type PublicationTarget, type PublicationEffects } from '@/api/hubNative'
import { PublicationFeedback, type PublicationResult } from './PublicationFeedback'

export function useHubPublication<T extends PublicationTarget>() {
  const [pending, setPending] = useState(false)
  const [result, setResult] = useState<PublicationResult | null>(null)
  const [inspected, setInspected] = useState<{ epoch: number; text: string } | null>(null)
  const [inspecting, setInspecting] = useState(false), [inspectionError, setInspectionError] = useState<unknown>(null)
  const work = useRef<AbortController | null>(null), read = useRef<AbortController | null>(null)
  const epoch = useRef(0), busy = useRef(false)
  useEffect(() => () => { work.current?.abort(); read.current?.abort() }, [])
  const blocked = !!result && result.inspection && inspected?.epoch !== result.epoch
  async function publish(input: T, execute: (input: T, signal: AbortSignal) => Promise<PublicationEffects>) {
    if (busy.current || blocked) return false
    busy.current = true
    const captured = structuredClone(input), controller = new AbortController(), generation = ++epoch.current
    work.current = controller; read.current?.abort()
    setInspected(null); setInspectionError(null); setInspecting(false); setPending(true)
    setResult({ epoch: generation, request: captured, effects: null, error: null, inspection: false })
    try {
      const outcome = await execute(captured, controller.signal)
      if (controller.signal.aborted) return false
      setResult({ epoch: generation, request: captured, effects: outcome, error: null, inspection: false })
      return true
    } catch (error) {
      if (!controller.signal.aborted) setResult({ epoch: generation, request: captured, effects: error instanceof HubPublicationError ? error.effects : null,
        error, inspection: !(error instanceof HubPublicationError) || error.inspectionRequired || error.status === 409 || error.effects?.observation_failed === true
          || error.effects?.owned_temporary_residue != null
          || (error.effects?.project_metadata !== undefined && !['not_written', 'published', 'unchanged', 'rejected'].includes(error.effects.project_metadata))
          || (typeof error.effects?.publication === 'object' && error.effects.publication !== null && [error.effects.publication.payload, error.effects.publication.catalog].some(value => value !== undefined && !['not_attempted', 'published', 'rejected'].includes(value))) })
      return false
    } finally { if (!controller.signal.aborted) { setPending(false); busy.current = false } }
  }
  async function inspect() {
    if (!result || pending || inspecting) return
    const captured = structuredClone(result), controller = new AbortController()
    read.current?.abort(); read.current = controller; setInspecting(true); setInspectionError(null)
    try {
      const input = captured.request, artifact = input.publication
      const page = await hubNative.search({ transport: 'broker', provider: artifact.provider, producer: input.project_id, artifact_type: artifact.artifact_type, id: artifact.id, version: artifact.version, limit: 20 }, controller.signal)
      if (!controller.signal.aborted && epoch.current === captured.epoch) {
        const found = page.artifacts.find(item => item.id === artifact.id && item.artifact_type === artifact.artifact_type && item.versions.includes(artifact.version))
        setInspected({ epoch: captured.epoch, text: found ? 'The original concrete version is listed. Latest: ' + (found.latest ?? 'not declared') + '. Review the recorded receipt and source before publishing again.' : 'The original concrete version is not listed. This does not prove that no payload or temporary residue remains.' })
      }
    } catch (error) { if (!controller.signal.aborted) setInspectionError(error) }
    finally { if (!controller.signal.aborted) setInspecting(false) }
  }
  const feedback = result && <PublicationFeedback result={result} pending={pending} inspected={inspected?.epoch === result.epoch ? inspected.text : null} inspecting={inspecting} inspectionError={inspectionError} onInspect={() => { void inspect() }} />
  return { pending, result, blocked, publish, feedback }
}
