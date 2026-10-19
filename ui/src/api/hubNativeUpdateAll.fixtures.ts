import type { UpdateAllInput, UpdateAllOutcome, UpdateAllItem } from './hubNative'
export const updateProject = '01KSH1CRFFG8Z74B5ZS78WW808'
export const updateInput = (cache = false): UpdateAllInput => cache ? { scope: { kind: 'global_cache' } } : { scope: { kind: 'project', project_id: updateProject, project_version: 'feature/a' }, binding: 'kiro-v3' }
export function updatedItem(input: UpdateAllInput, id = 'vendor/reviewer'): UpdateAllItem {
  return { artifact_type: 'skill', id, producer: '01D39ZY06FGSCTVN4T2V9PKHFZ', previous_version: 'release/one', target: input.scope.kind === 'global_cache' ? { kind: 'global_cache' } : { kind: 'shared_adapter', adapter: 'kiro' }, status: 'ok', outcome: { status: 'ok', cache: 'Updated', publication: 'Physical(Updated)', version: 'release/latest' } }
}
export const updateOutcome = (input = updateInput(), items: UpdateAllItem[] = [updatedItem(input)]): UpdateAllOutcome => ({ status: 'ok', operation: 'update_all', scope: structuredClone(input.scope), global: input.scope.kind === 'global_cache', items })
export function partialUpdate(input = updateInput()): UpdateAllOutcome {
  const first = updatedItem(input), second = updatedItem(input, 'vendor/failing'), third = updatedItem(input, 'vendor/later')
  return { ...updateOutcome(input), status: 'error', items: [first, { ...second, status: 'error', code: 'installation_changed', outcome: { status: 'error', cache: 'Updated', publication: { change: 'Unknown', committed: null, core_pending: true } } }, { artifact_type: third.artifact_type, id: third.id, producer: third.producer, previous_version: third.previous_version, target: third.target, status: 'not_attempted' }] }
}
