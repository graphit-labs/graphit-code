import { useEffect, useRef, useState } from 'react'
import { effectsNeedInspection, hubNative, HubRequestError, HubUpdateAllError, mutationCompleted, updateAllCompleted, updateAllNeedsInspection, type FileEffects, type FileOperation, type FileOutcome, type MutationSelection, type UpdateAllInput, type UpdateAllOutcome } from '@/api/hubNative'
interface MutationState { epoch: number; error: unknown; effects: FileEffects | null; inspectionRequired: boolean; inspected: boolean; completed: boolean }
export interface SingleHubMutationResult extends MutationState { operation: FileOperation; request: MutationSelection; outcome: FileOutcome | null }
export interface HubUpdateAllResult extends MutationState { operation: 'update_all'; request: UpdateAllInput; outcome: UpdateAllOutcome | null }
export type HubMutationResult = SingleHubMutationResult | HubUpdateAllResult
export function useHubMutation() {
  const active = useRef<AbortController | null>(null), epoch = useRef(0)
  const [pending, setPending] = useState(false), [result, setResult] = useState<HubMutationResult | null>(null)
  useEffect(() => () => active.current?.abort(), [])
  async function perform(operation: FileOperation, input: MutationSelection): Promise<boolean>
  async function perform(operation: 'update_all', input: UpdateAllInput): Promise<boolean>
  async function perform(operation: FileOperation | 'update_all', input: MutationSelection | UpdateAllInput) {
    if (active.current || (result?.inspectionRequired && !result.inspected)) return false
    const controller = new AbortController(); active.current = controller
    const currentEpoch = ++epoch.current
    const captured = operation === 'update_all' ? { operation, request: structuredClone(input as UpdateAllInput) } : { operation, request: structuredClone(input as MutationSelection) }
    const state = { ...captured, epoch: currentEpoch, effects: null, error: null, inspectionRequired: false, inspected: false, completed: false }
    setPending(true); setResult({ ...state, outcome: null })
    try {
      if (captured.operation === 'update_all') {
        const outcome = await hubNative.updateAll(captured.request, controller.signal)
        if (controller.signal.aborted) return false
        const completed = updateAllCompleted(outcome)
        setResult({ ...state, ...captured, outcome, completed, inspectionRequired: !completed })
        return completed
      }
      const outcome = await hubNative.mutate(captured.operation, captured.request, controller.signal)
      if (controller.signal.aborted) return false
      const completed = mutationCompleted(outcome)
      setResult({ ...state, ...captured, outcome, effects: outcome, completed, inspectionRequired: !completed })
      return completed
    } catch (error) {
      if (controller.signal.aborted) return false
      if (captured.operation === 'update_all') {
        const known = error instanceof HubUpdateAllError, outcome = known ? error.outcome : null, effects = known ? error.effects : null
        const knownNotAttempted = known && error.notAttempted && effects?.cache === 'not_attempted' && effects.publication === 'not_attempted' && !effectsNeedInspection(effects)
        const uncertain = !known || !error.receiptCorrelated || error.inspectionRequired || (effects !== null && effectsNeedInspection(effects))
          || (outcome !== null && updateAllNeedsInspection(outcome)) || (!outcome && !knownNotAttempted && ![400, 403, 404, 409, 501].includes(error.status))
        setResult({ ...state, ...captured, outcome, effects, error, inspectionRequired: uncertain })
      } else {
        const known = error instanceof HubRequestError, outcome = known ? error.outcome : null, effects = known ? outcome || error.effects : null
        const uncertain = !known || error.inspectionRequired || (effects !== null && effectsNeedInspection(effects)) || (!outcome && ![400, 403, 404, 409, 501].includes(error.status))
        setResult({ ...state, ...captured, outcome, effects, error, inspectionRequired: uncertain })
      }
      return false
    } finally { if (!controller.signal.aborted) { setPending(false); active.current = null } }
  }
  function inspected(expectedEpoch: number) {
    if (epoch.current !== expectedEpoch) return false
    setResult(previous => previous?.epoch === expectedEpoch ? { ...previous, inspected: true } : previous)
    return true
  }
  return { pending, result, perform, inspected, inspectionBlocked: !!result?.inspectionRequired && !result.inspected }
}
