import '@testing-library/jest-dom/vitest'
import { cleanup, render, screen, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, expect, it, vi } from 'vitest'
import type { SingleHubMutationResult } from './useHubMutation'
import { HubMutationFeedback } from './HubFeedback'
import { projectA } from './hubTestFixtures'

afterEach(cleanup)

it.each(['Link', { Copy: 'RejectEntryLinks' }] as const)('forget displays preserved paths without recovery bookkeeping (%j)', async mode => {
  const outcome: NonNullable<SingleHubMutationResult['outcome']> = {
    status: 'ok', operation: 'forget', scope: { kind: 'project', project_id: projectA, project_version: 'feature/a' },
    artifact_type: 'skill', id: 'vendor/reviewer', version: 'release-1', binding: 'codex',
    cache: 'not_attempted', publication: { change: 'Forgotten' },
    forgotten: { source: '/cache/ação', destination: 'C:\\project\\skill', mode, kind: 'Directory' },
  }
  const result: SingleHubMutationResult = {
    operation: 'forget', request: { scope: outcome.scope, artifact_type: 'skill', id: 'vendor/reviewer', version: 'release-1', binding: 'codex' },
    outcome, effects: outcome, error: null, completed: true, inspectionRequired: false,
  }
  const inspect = vi.fn()
  render(<HubMutationFeedback result={result} pending={false} onInspect={inspect} inspecting={false} />)
  expect(screen.getByText('Operation completed')).toBeInTheDocument()
  const files = screen.getByText('Files preserved').closest('section')!
  for (const text of ['/cache/ação', 'C:\\project\\skill', 'Directory', mode === 'Link' ? 'Link' : 'Copy']) {
    expect(within(files).getByText(text)).toBeInTheDocument()
  }
  expect(screen.queryByText(/reconciliation remains pending|Backup|Staging directory/)).not.toBeInTheDocument()
  await userEvent.setup().click(screen.getByRole('button', { name: 'Inspect captured target' }))
  expect(inspect).toHaveBeenCalledOnce()
  expect(result.outcome).toBe(outcome)
})

it('reports a partial effect without claiming rollback or adding a recovery action', () => {
  const request = { scope: { kind: 'project' as const, project_id: projectA, project_version: 'main' }, artifact_type: 'skill' as const, id: 'vendor/reviewer', binding: 'codex' as const }
  render(<HubMutationFeedback result={{ operation: 'remove', request, outcome: null, effects: { publication: { effect: 'MayHaveApplied' } }, error: new Error('failed'), completed: false, inspectionRequired: true }} pending={false} onInspect={vi.fn()} inspecting={false} />)
  expect(screen.getByText('Files may have changed')).toBeInTheDocument()
  expect(screen.queryByRole('button', { name: /recover/i })).not.toBeInTheDocument()
  expect(screen.queryByText('Operation completed')).not.toBeInTheDocument()
})
