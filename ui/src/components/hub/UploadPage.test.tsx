import '@testing-library/jest-dom/vitest'
import '@/test/contextControls'
import { act, cleanup, fireEvent, render, screen, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, beforeEach, expect, it, vi } from 'vitest'
import { hubNative, HubPublicationError, type ArtifactKind, type PackageUploadMetadata, type PackagePublicationOutcome } from '@/api/hubNative'
import { useAppStore } from '@/store/appStore'
import { useTaskWorkspace } from '@/components/task/taskWorkspace'
import { hubContext, projectA, projectB, selectHubProject } from './hubTestFixtures'
import UploadPage from './UploadPage'
vi.mock('@/hooks/useTheme', () => ({ useTheme: () => ({ theme: 'light' }) }))
function published(metadata: PackageUploadMetadata): PackagePublicationOutcome {
  const common = { status: 'published' as const, project_metadata: 'published' }
  if (metadata.publication.format === 'native') return { ...common, publication: { producer: metadata.project_id, artifact_type: metadata.publication.artifact_type, id: metadata.publication.id, version: metadata.publication.version, root_sha256: 'b'.repeat(64), catalog: 'published' } }
  return { ...common, publication: { producer: metadata.project_id, artifact_type: metadata.publication.artifact_type, id: metadata.publication.id, version: metadata.publication.version, archive_sha256: 'a'.repeat(64), payload: 'published', catalog: 'published' } }
}
const description = '**Purpose**\n\n- Preserve ' + String.fromCharCode(96) + 'contracts' + String.fromCharCode(96) + '\n- Read the [guide](https://example.test/guide)'
beforeEach(() => { hubContext(); useAppStore.setState({ apiBase: '', webMode: true }) })
afterEach(() => { cleanup(); vi.restoreAllMocks() })
async function packageStep(kind: ArtifactKind = 'skill') {
  const user = userEvent.setup(), native = kind === 'ast' || kind === 'knowledge'
  const file = new File(['opaque client fixture, not a valid package'], 'literal.' + (native ? kind : 'zip'))
  await user.click(screen.getByRole('combobox', { name: 'Artifact type' })); await user.click(screen.getByRole('option', { name: kind }))
  await user.upload(screen.getByLabelText(/^Package file/), file); await user.click(screen.getByRole('button', { name: 'Describe artifact' }))
  await user.type(screen.getByRole('textbox', { name: 'Artifact ID' }), 'literal/pkg')
  await user.type(screen.getByRole('textbox', { name: 'Version' }), 'release/one')
  await user.type(screen.getByRole('textbox', { name: 'Publication name' }), 'Uploader')
  return { user, file }
}
async function send(user: ReturnType<typeof userEvent.setup>) {
  await user.click(screen.getByRole('button', { name: 'Review publication' })); await user.click(screen.getByRole('button', { name: 'Upload & publish' }))
}
it('reviews Markdown without mutation and submits unchanged File with exact metadata and omitted dependencies', async () => {
  const upload = vi.spyOn(hubNative, 'upload').mockImplementation(async metadata => published(metadata)); render(<UploadPage />)
  const { user, file } = await packageStep();
  await user.click(screen.getByText('Optional publication metadata')); fireEvent.change(screen.getByRole('textbox', { name: 'Description' }), { target: { value: description } })
  await user.click(screen.getByRole('button', { name: 'Review publication' }))
  expect(screen.getByRole('heading', { name: 'Review the publication' })).toHaveFocus(); expect(screen.getByText('Purpose').tagName).toBe('STRONG'); expect(screen.getByText('contracts').tagName).toBe('CODE'); expect(screen.getByRole('link', { name: 'guide' })).toHaveAttribute('href', 'https://example.test/guide'); expect(upload).not.toHaveBeenCalled()
  await user.click(screen.getByRole('button', { name: 'Back' })); expect(screen.getByRole('textbox', { name: 'Description' })).toHaveValue(description)
  await send(user); await screen.findByRole('heading', { name: 'Publication confirmed' })
  expect(upload).toHaveBeenCalledWith({ project_id: projectA, project_version: 'feature/a', publication: { format: 'file', provider: 'corporate', artifact_type: 'skill', id: 'literal/pkg', version: 'release/one', name: 'Uploader', description, set_latest: false } }, file, expect.any(AbortSignal))
  expect(upload).toHaveBeenCalledOnce(); expect(screen.queryByRole('button', { name: 'Upload & publish' })).toBeNull()
  await user.click(screen.getByRole('button', { name: 'Choose another package' })); expect(screen.getByLabelText(/^Package file/)).toHaveValue('')
})
it.each(['ast', 'knowledge'] as const)('publishes Native %s with explicit empty dependencies and root digest only', async kind => {
  const upload = vi.spyOn(hubNative, 'upload').mockImplementation(async metadata => published(metadata)); render(<UploadPage />)
  const { user, file } = await packageStep(kind); expect(screen.queryByRole('checkbox', { name: 'Set latest to this concrete version' })).toBeNull()
  await user.click(screen.getByText('Optional publication metadata')); expect(screen.queryByRole('textbox', { name: 'Expected archive SHA256 for replacement' })).toBeNull()
  await user.selectOptions(screen.getByRole('combobox', { name: 'Dependency handling' }), 'replace'); await send(user); await screen.findByRole('heading', { name: 'Publication confirmed' })
  const [metadata, sentFile] = upload.mock.calls[0]; expect(metadata.publication).toEqual({ format: 'native', provider: 'corporate', artifact_type: kind, id: 'literal/pkg', version: 'release/one', name: 'Uploader', dependencies: [] }); expect(sentFile).toBe(file)
  const outcome = screen.getByRole('region', { name: 'Publication outcome' }); expect(outcome).toHaveTextContent('Root SHA256'); expect(outcome).not.toHaveTextContent('Archive SHA256'); expect(outcome).not.toHaveTextContent('Payload')
})
it('preserves literal dependency selectors and FILE-only latest/digest choices for a Power ZIP input', async () => {
  const upload = vi.spyOn(hubNative, 'upload').mockImplementation(async metadata => published(metadata)); render(<UploadPage />); const { user } = await packageStep('power')
  await user.click(screen.getByRole('checkbox', { name: 'Set latest to this concrete version' })); await user.click(screen.getByText('Optional publication metadata'))
  await user.type(screen.getByRole('textbox', { name: 'Expected archive SHA256 for replacement' }), 'a'.repeat(64)); await user.selectOptions(screen.getByRole('combobox', { name: 'Dependency handling' }), 'replace'); await user.click(screen.getByRole('button', { name: 'Add dependency' }))
  await user.selectOptions(screen.getByRole('combobox', { name: 'Dependency type' }), 'ast'); await user.type(screen.getByRole('textbox', { name: 'Dependency artifact ID' }), 'literal/dependency'); await user.type(screen.getByRole('textbox', { name: 'Dependency version selector' }), 'refs/pinned one'); await send(user)
  expect(upload.mock.calls[0][0].publication).toMatchObject({ format: 'file', artifact_type: 'power', set_latest: true, expected_sha256: 'a'.repeat(64), dependencies: [{ artifact_type: 'ast', id: 'literal/dependency', selector: 'refs/pinned one' }] })
})
it('keeps original owner and File across PROJECT/version/binding changes without another target reload', async () => {
  let finish!: (value: PackagePublicationOutcome) => void
  const upload = vi.spyOn(hubNative, 'upload').mockImplementationOnce(() => new Promise(resolve => { finish = resolve })), installed = vi.spyOn(hubNative, 'installed'); render(<UploadPage />)
  const { user, file } = await packageStep(); await send(user); const [captured, sentFile, signal] = upload.mock.calls[0]
  act(() => { selectHubProject(projectB, 'other/version'); useAppStore.setState({ activeAgent: 'qwen' }) }); expect(signal?.aborted).toBe(false); expect(sentFile).toBe(file)
  expect(screen.getByRole('region', { name: 'Publication outcome' })).toHaveTextContent(projectA + ' @ feature/a'); await act(async () => finish(published(captured)))
  expect(screen.getByRole('region', { name: 'Publication outcome' })).toHaveTextContent(projectA + ' @ feature/a'); expect(upload).toHaveBeenCalledOnce(); expect(installed).not.toHaveBeenCalled(); expect(screen.getByRole('heading', { name: 'Choose what you are sharing' })).toBeTruthy()
})
it.each(['identity', 'logout', 'apiBase'] as const)('aborts/discards sent upload after %s changes', async boundary => {
  let finish!: (value: PackagePublicationOutcome) => void
  const upload = vi.spyOn(hubNative, 'upload').mockImplementationOnce(() => new Promise(resolve => { finish = resolve })); render(<UploadPage />); const { user } = await packageStep(); await send(user)
  const [captured, , signal] = upload.mock.calls[0]; act(() => { if (boundary === 'identity') useAppStore.setState({ webIdentity: { state: 'authenticated', provider: 'corporate', subject: 'second-user' } }); else if (boundary === 'logout') useAppStore.setState({ webIdentity: { state: 'anonymous' } }); else useAppStore.setState({ apiBase: '/other-api' }) })
  expect(signal?.aborted).toBe(true); await act(async () => finish(published(captured))); expect(screen.queryByRole('region', { name: 'Publication outcome' })).toBeNull(); expect(upload).toHaveBeenCalledOnce()
})
it('retains drafts/File and actual413 effects until original epoch inspection and explicit retry', async () => {
  const upload = vi.spyOn(hubNative, 'upload').mockImplementationOnce(async metadata => { throw new HubPublicationError(413, 'request_too_large', { publication: 'unknown', project_metadata: 'unknown', observation_failed: true }, true, metadata) }).mockImplementation(async metadata => published(metadata))
  const search = vi.spyOn(hubNative, 'search').mockResolvedValue({ status: 'ok', producer: projectA, artifacts: [], next_cursor: null }); render(<UploadPage />); const { user, file } = await packageStep(); await send(user)
  await screen.findByRole('heading', { name: 'Publication needs review' }); const outcome = screen.getByRole('region', { name: 'Publication outcome' }); expect(outcome).toHaveTextContent('413'); expect(outcome).toHaveTextContent('unknown'); expect(screen.getByRole('button', { name: 'Upload & publish' })).toBeDisabled()
  await user.click(screen.getByRole('button', { name: 'Back' })); expect(screen.getByRole('textbox', { name: 'Artifact ID' })).toHaveValue('literal/pkg'); await user.click(screen.getByRole('button', { name: 'Back' })); expect(screen.getAllByText('literal.zip', { exact: false }).length).toBeGreaterThan(0)
  await user.click(within(outcome).getByRole('button', { name: 'Inspect original publication' })); await screen.findByText('Publication catalogue inspected')
  expect(search).toHaveBeenCalledWith({ transport: 'broker', provider: 'corporate', producer: projectA, artifact_type: 'skill', id: 'literal/pkg', version: 'release/one', limit: 20 }, expect.any(AbortSignal)); expect(upload).toHaveBeenCalledOnce()
  await user.click(screen.getByRole('button', { name: 'Describe artifact' })); await send(user); await screen.findByRole('heading', { name: 'Publication confirmed' }); expect(upload).toHaveBeenCalledTimes(2); expect(upload.mock.calls[1][1]).toBe(file)
})
it('requires a Power ZIP input and never offers unsupported Framework', async () => {
  render(<UploadPage />); const user = userEvent.setup(); await user.click(screen.getByRole('combobox', { name: 'Artifact type' })); expect(screen.queryByRole('option', { name: 'framework' })).toBeNull(); await user.click(screen.getByRole('option', { name: 'power' }))
  expect(screen.getByRole('button', { name: 'Describe artifact' })).toBeDisabled(); expect(screen.getByLabelText(/^Package file/)).toHaveAttribute('accept', '.zip'); fireEvent.change(screen.getByLabelText(/^Package file/), { target: { files: [new File(['opaque'], 'wrong.ast')] } }); expect(screen.getByRole('alert')).toHaveTextContent('Choose one .zip package'); expect(screen.getByRole('button', { name: 'Describe artifact' })).toBeDisabled()
})
it('requires authorized PROJECT but no binding and keeps anonymous state explicit', () => {
  act(() => useTaskWorkspace.setState({ module: 'hub', phase: 'ready', selection: null, contexts: [] })); const view = render(<UploadPage />); expect(screen.getByText('Choose a publication project')).toBeTruthy(); expect(screen.queryByLabelText(/^Package file/)).toBeNull()
  act(() => { selectHubProject(); useAppStore.setState({ activeAgent: 'unsupported' }) }); expect(screen.getByLabelText(/^Package file/)).toBeTruthy(); view.unmount(); act(() => useAppStore.setState({ webIdentity: { state: 'anonymous' } })); render(<UploadPage />); expect(screen.getByText('Sign in to publish artifacts')).toBeTruthy()
})
it('invalidates unsent draft on data-version changes, preserving it across binding changes', async () => {
  render(<UploadPage />); const { user } = await packageStep(); act(() => useAppStore.setState({ activeAgent: 'qwen' })); expect(screen.getByRole('textbox', { name: 'Artifact ID' })).toHaveValue('literal/pkg'); act(() => selectHubProject(projectA, 'new-version')); expect(screen.getByRole('heading', { name: 'Choose what you are sharing' })).toBeTruthy(); expect(screen.getByLabelText(/^Package file/)).toHaveValue(''); await user.tab(); expect(document.activeElement).not.toBe(document.body)
})
