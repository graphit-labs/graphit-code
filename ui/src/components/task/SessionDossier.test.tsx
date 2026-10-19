import '@testing-library/jest-dom/vitest'
import { act, cleanup, render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { MemoryRouter } from 'react-router-dom'
import { afterEach, beforeEach, expect, it, vi } from 'vitest'
import { sessionApi } from '@/api/taskSession'
import { parseNativeJson } from '@/api/nativeJson'
import { SessionDossier } from './SessionDossier'
import { context, sessionDetail, sessionHistoryPage } from './task.test-fixtures'
import { useTaskWorkspace } from './taskWorkspace'
vi.mock('@/api/taskSession', async original => ({ ...await original<typeof import('@/api/taskSession')>(), sessionApi: { get: vi.fn() } }))
vi.mock('@/components/shared/RecordReferences', () => ({ RecordReferences: () => <h3>Record links</h3> }))
let filename = ''
beforeEach(() => {
 vi.resetAllMocks(); filename = ''
 vi.stubGlobal('matchMedia', vi.fn((query: string) => ({ matches: false, media: query, addEventListener: vi.fn(), removeEventListener: vi.fn() })))
 useTaskWorkspace.setState({ selection: context, contexts: [context], phase: 'ready' })
 class TestURL extends URL { static createObjectURL = vi.fn(() => 'blob:session'); static revokeObjectURL = vi.fn() }
 vi.stubGlobal('URL', TestURL)
 vi.stubGlobal('Blob', class { constructor(public readonly parts: BlobPart[]) {} })
 vi.spyOn(HTMLAnchorElement.prototype, 'click').mockImplementation(function (this: HTMLAnchorElement) { filename = this.download })
})
afterEach(() => { cleanup(); vi.restoreAllMocks(); vi.unstubAllGlobals() })
function page() { return render(<MemoryRouter><SessionDossier detail={sessionDetail} selection={context} onBack={vi.fn()} linkedTasks={<p>Linked task region</p>} /></MemoryRouter>) }
it('does not load audit on mount and downloads complete native JSON only on explicit export', async () => {
 vi.mocked(sessionApi.get).mockResolvedValueOnce({ ...sessionDetail, history: sessionHistoryPage })
 const user = userEvent.setup(); page()
 expect(sessionApi.get).not.toHaveBeenCalled()
 expect(screen.getByText('Current session response')).toBeTruthy()
 expect(screen.queryByText('Complete JSON record')).toBeNull()
 await user.click(screen.getByRole('button', { name: 'Export complete JSON' }))
 expect(filename).toBe('session1.json')
 const blob = vi.mocked(URL.createObjectURL).mock.calls[0][0] as unknown as { parts: BlobPart[] }
 expect(parseNativeJson(blob.parts.join(''))).toEqual({ ...sessionDetail, history: sessionHistoryPage })
 expect(URL.revokeObjectURL).toHaveBeenCalledWith('blob:session')
 expect(sessionApi.get).toHaveBeenCalledWith(context, 'session1', { signal: expect.any(AbortSignal), history: { page_size: 20, cursor: sessionDetail.history_start_cursor } })
})
it('exposes export failure without producing a partial file', async () => {
 vi.mocked(sessionApi.get).mockResolvedValueOnce({ ...sessionDetail, history: { ...sessionHistoryPage, next_cursor: 'later-page' } }).mockRejectedValueOnce(new Error('Audit denied'))
 const user = userEvent.setup(); page()
 await user.click(screen.getByRole('button', { name: 'Export complete JSON' }))
 expect(await screen.findByRole('alert')).toHaveTextContent('Audit denied')
 expect(URL.createObjectURL).not.toHaveBeenCalled()
 expect(filename).toBe('')
})
it('aborts export on unmount and ignores its late result', async () => {
 let resolve!: (value: typeof sessionDetail) => void
 vi.mocked(sessionApi.get).mockImplementationOnce(() => new Promise(done => { resolve = done }))
 const user = userEvent.setup(), view = page()
 await user.click(screen.getByRole('button', { name: 'Export complete JSON' }))
 const signal = vi.mocked(sessionApi.get).mock.calls[0][2]!.signal!
 view.unmount(); expect(signal.aborted).toBe(true)
 await act(async () => resolve({ ...sessionDetail, history: sessionHistoryPage }))
 expect(URL.createObjectURL).not.toHaveBeenCalled()
})
