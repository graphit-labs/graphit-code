import '@/test/contextControls'
import '@testing-library/jest-dom/vitest'
import { act, cleanup, render, screen, waitFor, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, beforeEach, expect, it, vi } from 'vitest'
import { hubNative, HubRequestError, type AuthoredPage, type AuthoredReview } from '@/api/hubNative'
import { useAppStore } from '@/store/appStore'
import { hubContext, installedPage, projectA, projectB, selectHubProject } from './hubTestFixtures'
import ProjectArtifactsPage from './ProjectArtifactsPage'

vi.mock('@/api/hubNative', async original => ({ ...await original<typeof import('@/api/hubNative')>(), hubNative: { authoredList: vi.fn(), authoredReview: vi.fn(), authoredContent: vi.fn(), installed: vi.fn(), publish: vi.fn(), search: vi.fn(), content: vi.fn(), mutate: vi.fn() } }))
vi.mock('@/hooks/useTheme', () => ({ useTheme: () => ({ theme: 'light' }) }))
const resource = { artifact_type: 'skill' as const, id: 'vendor/reviewer', version: 'artifact/one', name: null }
const selection = { project_id: projectA, project_version: 'feature/a', artifact_type: resource.artifact_type, id: resource.id, version: resource.version }
const firstPage = (): AuthoredPage => ({ status: 'ok', project_id: projectA, project_version: 'feature/a', entries: [resource], next_cursor: null })
const review = (): AuthoredReview => ({ status: 'ok', ...selection, source: 'Resources', metadata: null, entries: [{ path: 'docs', kind: 'Directory', bytes: 0 }, { path: 'docs/readme.md', kind: 'File', bytes: 12 }], next_offset: null })
beforeEach(() => {
  vi.resetAllMocks(); hubContext()
  vi.mocked(hubNative.installed).mockImplementation(async query => ({ ...installedPage([]), scope: query.scope }))
  vi.mocked(hubNative.authoredList).mockResolvedValue(firstPage())
  vi.mocked(hubNative.authoredReview).mockResolvedValue(review())
  vi.mocked(hubNative.authoredContent).mockImplementation(async input => { const { path, ...captured } = input; return { status: 'ok', ...captured, content: { path, content: '# Resource text', bytes: 15 } } })
  vi.mocked(hubNative.publish).mockImplementation(async input => ({ status: 'published', project_metadata: 'published', publication: { producer: input.project_id, artifact_type: input.publication.artifact_type, id: input.publication.id, version: input.publication.version, archive_sha256: 'a'.repeat(64), payload: 'published', catalog: 'published' } }))
})
afterEach(cleanup)
async function browse(user: ReturnType<typeof userEvent.setup>) { await user.click(screen.getByRole('button', { name: 'Browse project resources' })); await screen.findByRole('button', { name: /vendor\/reviewer/ }) }
async function inspect(user: ReturnType<typeof userEvent.setup>) { await browse(user); await user.click(screen.getByRole('button', { name: /vendor\/reviewer/ })); await screen.findByRole('button', { name: 'Read docs/readme.md' }) }

it('authored journey reads a selected tree file and publishes its captured source through explicit metadata and review', async () => {
  const user = userEvent.setup(); render(<ProjectArtifactsPage />)
  expect(hubNative.authoredList).not.toHaveBeenCalled()
  await inspect(user)
  expect(hubNative.authoredList).toHaveBeenCalledWith({ project_id: projectA, project_version: 'feature/a', limit: 20 }, expect.any(AbortSignal))
  expect(hubNative.authoredReview).toHaveBeenCalledWith({ ...selection, offset: 0, limit: 20 }, expect.any(AbortSignal))
  expect(screen.getByText('No source metadata descriptor')).toBeInTheDocument()
  expect(screen.getByRole('article', { name: 'Project resource details' })).toHaveFocus()
  await user.click(screen.getByRole('button', { name: 'Read docs/readme.md' }))
  const content = await screen.findByRole('region', { name: 'Resource file content' })
  expect(content).toHaveFocus(); expect(content).toHaveTextContent('# Resource text')
  expect(hubNative.authoredContent).toHaveBeenCalledWith({ ...selection, path: 'docs/readme.md' }, expect.any(AbortSignal))
  expect(hubNative.content).not.toHaveBeenCalled()
  await user.keyboard('{Escape}')
  expect(screen.getByRole('button', { name: 'Read docs/readme.md' })).toHaveFocus()
  expect(screen.getByRole('article', { name: 'Project resource details' })).toBeInTheDocument()
  await user.click(screen.getByRole('button', { name: 'Publish project resources' }))
  const dialog = screen.getByRole('dialog')
  expect(dialog).toHaveTextContent('artifact/one')
  expect(within(dialog).queryByRole('textbox', { name: 'Publication artifact ID' })).not.toBeInTheDocument()
  expect(within(dialog).queryByRole('textbox', { name: 'Concrete publication version' })).not.toBeInTheDocument()
  expect(screen.getByRole('textbox', { name: 'Publication name' })).toHaveValue('')
  await user.type(screen.getByRole('textbox', { name: 'Publication name' }), 'Reviewed resource')
  await user.click(screen.getByRole('button', { name: 'Review publication' }))
  expect(hubNative.publish).not.toHaveBeenCalled()
  await user.click(screen.getByRole('button', { name: 'Publish artifact' }))
  await screen.findByText('Publication confirmed')
  expect(hubNative.publish).toHaveBeenCalledWith({ project_id: projectA, project_version: 'feature/a', publication: { provider: 'corporate', artifact_type: 'skill', id: resource.id, version: resource.version, name: 'Reviewed resource', set_latest: false } }, expect.any(AbortSignal))
})

it.each(['ast', 'knowledge'] as const)('authored journey reads %s while keeping FILE publication unavailable', async artifact_type => {
  vi.mocked(hubNative.authoredList).mockResolvedValue({ ...firstPage(), entries: [{ ...resource, artifact_type }] })
  vi.mocked(hubNative.authoredReview).mockResolvedValue({ ...review(), artifact_type })
  const user = userEvent.setup(); render(<ProjectArtifactsPage />); await inspect(user)
  expect(screen.getByText('Read-only resource here')).toBeInTheDocument()
  expect(screen.queryByRole('button', { name: 'Publish project resources' })).not.toBeInTheDocument()
  await user.click(screen.getByRole('button', { name: 'Read docs/readme.md' }))
  await screen.findByRole('region', { name: 'Resource file content' })
  expect(hubNative.authoredContent).toHaveBeenCalledWith(expect.objectContaining({ artifact_type }), expect.any(AbortSignal))
  expect(hubNative.publish).not.toHaveBeenCalled()
})

it('authored journey keeps lexical list and independent tree pagination explicit and refreshes from zero', async () => {
  vi.mocked(hubNative.authoredList).mockResolvedValueOnce({ ...firstPage(), next_cursor: 'lexical-after-first' }).mockResolvedValueOnce(firstPage()).mockResolvedValue(firstPage())
  vi.mocked(hubNative.authoredReview).mockResolvedValueOnce({ ...review(), next_offset: 20 }).mockResolvedValue(review())
  const user = userEvent.setup(); render(<ProjectArtifactsPage />); await browse(user)
  await user.click(screen.getByRole('button', { name: 'Next resource page' }))
  await waitFor(() => expect(hubNative.authoredList).toHaveBeenLastCalledWith(expect.objectContaining({ cursor: 'lexical-after-first', limit: 20 }), expect.any(AbortSignal)))
  await user.click(await screen.findByRole('button', { name: /vendor\/reviewer/ }))
  await user.click(await screen.findByRole('button', { name: 'Next tree page' }))
  await waitFor(() => expect(hubNative.authoredReview).toHaveBeenLastCalledWith({ ...selection, offset: 20, limit: 20 }, expect.any(AbortSignal)))
  await user.click(await screen.findByRole('button', { name: 'Refresh resource tree' }))
  await waitFor(() => expect(hubNative.authoredReview).toHaveBeenLastCalledWith({ ...selection, offset: 0, limit: 20 }, expect.any(AbortSignal)))
  const reads = vi.mocked(hubNative.authoredReview).mock.calls.length
  await user.click(await screen.findByRole('button', { name: 'Refresh resource tree' }))
  await waitFor(() => expect(hubNative.authoredReview).toHaveBeenCalledTimes(reads + 1))
  await user.click(screen.getByRole('button', { name: 'Refresh project resources' }))
  await waitFor(() => expect(hubNative.authoredList).toHaveBeenLastCalledWith({ project_id: projectA, project_version: 'feature/a', limit: 20 }, expect.any(AbortSignal)))
  expect(screen.queryByRole('article', { name: 'Project resource details' })).not.toBeInTheDocument()
})

it.each([[404, 'authored_file_not_found', 'The resource file is no longer available'], [415, 'authored_content_not_text', 'This file is not text'], [413, 'authored_too_large', 'The resource exceeds its read limit']] as const)('authored journey exposes HTTP%s content errors without an empty-file claim', async (status, code, title) => {
  vi.mocked(hubNative.authoredContent).mockRejectedValueOnce(new HubRequestError(status, code, null, false, null, {}))
  const user = userEvent.setup(); render(<ProjectArtifactsPage />); await inspect(user)
  await user.click(screen.getByRole('button', { name: 'Read docs/readme.md' })); await screen.findByText(title)
  expect(screen.queryByText('This text file is empty.')).not.toBeInTheDocument()
  expect(hubNative.authoredContent).toHaveBeenCalledOnce()
})

it('authored journey distinguishes successful empty text from errors and empty inventory', async () => {
  vi.mocked(hubNative.authoredContent).mockResolvedValueOnce({ status: 'ok', ...selection, content: { path: 'docs/readme.md', content: '', bytes: 0 } })
  const user = userEvent.setup(); render(<ProjectArtifactsPage />); await inspect(user)
  await user.click(screen.getByRole('button', { name: 'Read docs/readme.md' })); await screen.findByText('This text file is empty.')
  expect(screen.queryByText('No maintained project resources')).not.toBeInTheDocument()
})

it.each(['identity', 'logout', 'apiBase', 'project'] as const)('authored journey aborts and discards an old deferred list after %s changes', async boundary => {
  let finish!: (value: AuthoredPage) => void
  vi.mocked(hubNative.authoredList).mockImplementationOnce(() => new Promise(resolve => { finish = resolve }))
  const user = userEvent.setup(); render(<ProjectArtifactsPage />)
  await user.click(screen.getByRole('button', { name: 'Browse project resources' }))
  const [, signal] = vi.mocked(hubNative.authoredList).mock.calls[0]
  act(() => { if (boundary === 'project') selectHubProject(projectB, 'feature/new', '/b'); else useAppStore.setState(boundary === 'logout' ? { webIdentity: { state: 'anonymous' } } : boundary === 'apiBase' ? { apiBase: '/another-api' } : { webIdentity: { state: 'authenticated', provider: 'corporate', subject: 'another-user' } }) })
  await waitFor(() => expect(signal?.aborted).toBe(true))
  await act(async () => finish(firstPage()))
  expect(screen.queryByRole('button', { name: /vendor\/reviewer/ })).not.toBeInTheDocument()
  expect(screen.queryByRole('dialog')).not.toBeInTheDocument()
})

it('authored journey discards content from a previous selection and closes review on context change', async () => {
  let finish!: (value: Awaited<ReturnType<typeof hubNative.authoredContent>>) => void
  vi.mocked(hubNative.authoredList).mockResolvedValue({ ...firstPage(), entries: [resource, { ...resource, id: 'other/resource' }] })
  vi.mocked(hubNative.authoredReview).mockImplementation(async input => ({ ...review(), id: input.id }))
  vi.mocked(hubNative.authoredContent).mockImplementationOnce(() => new Promise(resolve => { finish = resolve }))
  const user = userEvent.setup(); render(<ProjectArtifactsPage />); await inspect(user)
  await user.click(screen.getByRole('button', { name: 'Read docs/readme.md' }))
  const [, signal] = vi.mocked(hubNative.authoredContent).mock.calls[0]
  await user.click(screen.getByRole('button', { name: /other\/resource/ }))
  await waitFor(() => expect(signal?.aborted).toBe(true))
  await act(async () => finish({ status: 'ok', ...selection, content: { path: 'docs/readme.md', content: 'Stale private content', bytes: 21 } }))
  expect(screen.queryByText('Stale private content')).not.toBeInTheDocument()
  await user.click(await screen.findByRole('button', { name: 'Publish project resources' }))
  await user.type(screen.getByRole('textbox', { name: 'Publication name' }), 'Draft')
  await user.click(screen.getByRole('button', { name: 'Review publication' }))
  act(() => selectHubProject(projectB, 'feature/new', '/b'))
  await waitFor(() => expect(screen.queryByRole('dialog')).not.toBeInTheDocument())
  expect(hubNative.publish).not.toHaveBeenCalled()
})

it('authored journey keeps a read denial distinct from an empty inventory and retries only explicitly', async () => {
  vi.mocked(hubNative.authoredList).mockRejectedValueOnce(new HubRequestError(403, 'permission_denied', null, false, null, {})).mockResolvedValueOnce({ ...firstPage(), entries: [] })
  const user = userEvent.setup(); render(<ProjectArtifactsPage />)
  await user.click(screen.getByRole('button', { name: 'Browse project resources' })); await screen.findByText('Access unavailable')
  expect(screen.queryByText('No maintained project resources')).not.toBeInTheDocument()
  expect(hubNative.authoredList).toHaveBeenCalledOnce()
  await user.click(screen.getByRole('button', { name: 'Refresh project resources' })); await screen.findByText('No maintained project resources')
})

it.each([[404, 'authored_file_not_found', 'The resource file is no longer available'], [409, 'native_operation_failed', 'The selected state changed']] as const)('authored journey keeps current-capture review HTTP%s explicit and refreshes only on request', async (status, code, title) => {
  vi.mocked(hubNative.authoredReview).mockRejectedValueOnce(new HubRequestError(status, code, null, false, null, {})).mockResolvedValueOnce(review())
  const user = userEvent.setup(); render(<ProjectArtifactsPage />); await browse(user)
  await user.click(screen.getByRole('button', { name: /vendor\/reviewer/ })); await screen.findByText(title)
  expect(screen.queryByRole('button', { name: 'Publish project resources' })).not.toBeInTheDocument()
  expect(hubNative.authoredReview).toHaveBeenCalledOnce()
  await user.click(screen.getByRole('button', { name: 'Refresh resource tree' }))
  await screen.findByRole('button', { name: 'Read docs/readme.md' })
  expect(hubNative.authoredReview).toHaveBeenCalledTimes(2)
  expect(hubNative.authoredReview).toHaveBeenLastCalledWith({ ...selection, offset: 0, limit: 20 }, expect.any(AbortSignal))
  expect(hubNative.publish).not.toHaveBeenCalled()
})
