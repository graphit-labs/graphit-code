import '@testing-library/jest-dom/vitest'
import { cleanup, render, screen, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, expect, it, vi } from 'vitest'
import type { SingleHubMutationResult } from './useHubMutation'
import { HubMutationFeedback } from './HubFeedback'
import { projectA } from './hubTestFixtures'

afterEach(cleanup)

it.each(['Link', { Copy: 'RejectFileLinks' }] as const)('Unicode path preview preserves literal text, descriptors and residue semantics (%j)', async mode => {
  const parent = '/project/ação'
  const outcome: NonNullable<SingleHubMutationResult['outcome']> = {
    status: 'ok', operation: 'forget', scope: { kind: 'project', project_id: projectA, project_version: 'feature/a' },
    artifact_type: 'skill', id: 'vendor/reviewer', version: 'release-1', binding: 'codex',
    cache: 'not_attempted', publication: { change: 'Forgotten', cleanup_pending: true, core_pending: true },
    payload_preserved: true, stage_cleanup_pending: true,
    possible_parents: [parent, "/ÿ"],
    forget_preview: {
      owned: { source: '/cache/source', destination: 'C:\\project\\skill', mode, node_kind: 'Directory' },
      pending: {
        candidate: { source: '/cache/candidate', destination: '/project/candidate', mode: { Copy: 'FollowRegularFiles' }, node_kind: 'File' },
        staging_directory: '/project/stage', old_staging_directory: '/project/old-stage',
        old_destination: '/project/old-destination', payload: 'C:\\cache\\payload',
        backup: 'C:\\cache\\ação backup', permissions_may_be_modified: true,
      },
      possible_parents: [structuredClone(parent), '/project/ação'],
    },
  }
  const result: SingleHubMutationResult = {
    epoch: 8, operation: 'forget', request: { scope: outcome.scope, artifact_type: 'skill', id: 'vendor/reviewer', version: 'release-1', binding: 'codex' },
    outcome, effects: outcome, error: null, completed: false, inspected: false, inspectionRequired: true,
  }
  const inspect = vi.fn()
  render(<HubMutationFeedback result={result} pending={false} onInspect={inspect} inspecting={false} />)
  expect(screen.getByText('Inspect the remaining state')).toBeInTheDocument()
  const preview = screen.getByText('Preserved physical residues').closest('section')!
  for (const text of ['/cache/source', 'C:\\project\\skill', '/cache/candidate', '/project/candidate', '/project/stage', '/project/old-stage', '/project/old-destination', 'C:\\cache\\payload', 'C:\\cache\\ação backup', 'Copy (FollowRegularFiles)']) {
    expect(within(preview).getByText(text)).toBeInTheDocument()
  }
  expect(within(preview).getByText(typeof mode === 'string' ? 'Link' : 'Copy (RejectFileLinks)')).toBeInTheDocument()
  expect(screen.getByText('Forget changes logical ownership. It does not authorize deletion of these paths.')).toBeInTheDocument()
  const parents = screen.getByText('Possible parent residues').closest('section')!
  expect(within(parents).getAllByRole('listitem')).toHaveLength(2)
  expect(within(parents).getAllByText('/project/ação')).toHaveLength(1)
  expect(within(parents).getByText('/ÿ')).toBeInTheDocument()
  for (const text of ['Physical payload was preserved.', 'Physical reconciliation remains pending.', 'Temporary stage cleanup remains pending.', 'Inspection required']) {
    expect(screen.getByText(text)).toBeInTheDocument()
  }
  await userEvent.setup().click(screen.getByRole('button', { name: 'Inspect captured target' }))
  expect(inspect).toHaveBeenCalledOnce()
  expect(result.outcome).toBe(outcome)
  expect(result.request).toMatchObject({ scope: outcome.scope, binding: 'codex', version: 'release-1' })
})
