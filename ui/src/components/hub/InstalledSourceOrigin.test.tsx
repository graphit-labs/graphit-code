import '@/test/contextControls'
import '@testing-library/jest-dom/vitest'
import { act, cleanup, render, screen, waitFor, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, beforeEach, expect, it, vi } from 'vitest'
import { hubNative, type InstalledFile, type SourceOrigin } from '@/api/hubNative'
import { RefreshContext, type Registration } from '@/components/layout/WorkspaceRefresh'
import ProjectArtifactsPage from './ProjectArtifactsPage'
import { completed, hubContext, installed, installedPage } from './hubTestFixtures'
vi.mock('@/api/hubNative', async importOriginal => ({ ...await importOriginal<typeof import('@/api/hubNative')>(), hubNative: { installed: vi.fn(), mutate: vi.fn(), content: vi.fn() } }))
vi.mock('@/hooks/useTheme', () => ({ useTheme: () => ({ theme: 'light' }) }))
beforeEach(() => { vi.resetAllMocks(); hubContext(); vi.mocked(hubNative.mutate).mockImplementation(async (operation, input) => completed(operation, input)) })
afterEach(cleanup)
function inventory(entry: InstalledFile) { vi.mocked(hubNative.installed).mockImplementation(async query => ({ ...installedPage([entry]), scope: query.scope })) }

it.each(['Resources', 'LocalLink', 'HubCache'] as const)('installed inventory and inspector display exact %s with null remote origin and no remote capability', async source_origin => {
  inventory({ ...installed, source_origin, origin: null })
  const user = userEvent.setup(); render(<ProjectArtifactsPage />)
  const row = await screen.findByRole('button', { name: /Installed reviewer/ })
  expect(row).toHaveTextContent(source_origin)
  await user.click(row)
  const inspector = screen.getByRole('article', { name: 'Artifact details' })
  expect(within(inspector).getByText(source_origin)).toBeInTheDocument()
  expect(within(inspector).queryByText('Local source')).not.toBeInTheDocument()
  expect(within(inspector).queryByRole('button', { name: 'Update artifact' })).not.toBeInTheDocument()
  expect(within(inspector).queryByRole('textbox', { name: 'Relative file path' })).not.toBeInTheDocument()
  for (const name of ['Remove artifact', 'Recover target', 'Forget logical ownership']) expect(within(inspector).getByRole('button', { name })).toBeEnabled()
  expect(hubNative.content).not.toHaveBeenCalled()
  expect(hubNative.mutate).not.toHaveBeenCalled()
})

it.each([
  { transport: 'broker', provider: 'corporate' },
  { transport: 'standalone', provider: 'files', profile: 'release' },
] as const)('installed HubCache keeps remote transport separate and enables actual remote Update/content %j', async origin => {
  inventory({ ...installed, source_origin: 'HubCache', origin })
  const user = userEvent.setup(); render(<ProjectArtifactsPage />)
  await user.click(await screen.findByRole('button', { name: /Installed reviewer/ }))
  const inspector = screen.getByRole('article', { name: 'Artifact details' })
  expect(within(inspector).getByText('HubCache')).toBeInTheDocument()
  expect(within(inspector).getByText(origin.transport)).toBeInTheDocument()
  expect(within(inspector).getByText(origin.provider)).toBeInTheDocument()
  expect(within(inspector).getByRole('textbox', { name: 'Relative file path' })).toBeInTheDocument()
  await user.click(within(inspector).getByRole('button', { name: 'Update artifact' }))
  await user.click(within(screen.getByRole('dialog')).getByRole('button', { name: 'Update artifact' }))
  await screen.findByText('Operation completed')
  expect(hubNative.mutate).toHaveBeenCalledWith('update', { selection: { ...origin, producer: installed.producer, artifact_type: installed.artifact_type, id: installed.id, version: installed.version, exact: true }, scope: installedPage().scope, binding: 'kiro-v3' }, expect.any(AbortSignal))
})

it('installed SourceOrigin changes invalidate the selected inspector even when all old key fields match', async () => {
  let source_origin: SourceOrigin = 'Resources'
  vi.mocked(hubNative.installed).mockImplementation(async query => ({ ...installedPage([{ ...installed, source_origin, origin: null }]), scope: query.scope }))
  let refresh!: Registration
  const user = userEvent.setup(); render(<RefreshContext.Provider value={{ scope: 'fixture', register: entry => { refresh = entry; return () => {} }, refresh: async () => {}, refreshing: false, error: '' }}><ProjectArtifactsPage /></RefreshContext.Provider>)
  await user.click(await screen.findByRole('button', { name: /Installed reviewer/ }))
  expect(within(screen.getByRole('article', { name: 'Artifact details' })).getByText('Resources')).toBeInTheDocument()
  const previousReads = vi.mocked(hubNative.installed).mock.calls.length
  source_origin = 'LocalLink'
  await act(async () => { await refresh.run() })
  await waitFor(() => expect(hubNative.installed).toHaveBeenCalledTimes(previousReads + 1))
  expect(await screen.findByRole('button', { name: /Installed reviewer/ })).toHaveTextContent('LocalLink')
  expect(screen.queryByRole('article', { name: 'Artifact details' })).not.toBeInTheDocument()
  await user.click(screen.getByRole('button', { name: /Installed reviewer/ }))
  expect(within(screen.getByRole('article', { name: 'Artifact details' })).getByText('LocalLink')).toBeInTheDocument()
})
