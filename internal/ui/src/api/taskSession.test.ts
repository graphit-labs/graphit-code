import { afterEach, describe, expect, it, vi } from 'vitest'

import { sessionApi } from './taskSession'

describe('Session API', () => {
  afterEach(() => {
    vi.unstubAllGlobals()
  })

  it('uses bounded catalogue parameters and an opaque cursor', async () => {
    const response = { results: [], next_cursor: 'next-page' }
    const fetchMock = vi.fn().mockResolvedValue({ ok: true, json: async () => response })
    vi.stubGlobal('fetch', fetchMock)

    await sessionApi.list({
      projectDir: '/work/project',
      query: 'archive',
      status: 'in_progress',
      active: true,
      pageSize: 25,
      cursor: 'opaque-token',
    })

    expect(fetchMock).toHaveBeenCalledWith(
      '/api/tasks/sessions?project_dir=%2Fwork%2Fproject&query=archive&status=in_progress&active=true&page_size=25&cursor=opaque-token',
      expect.anything(),
    )
  })

  it('omits status and active when unset or falsy', async () => {
    const response = { results: [], next_cursor: '' }
    const fetchMock = vi.fn().mockResolvedValue({ ok: true, json: async () => response })
    vi.stubGlobal('fetch', fetchMock)

    await sessionApi.list({ projectDir: '/work/project', status: 'all', active: false })

    expect(fetchMock).toHaveBeenCalledWith('/api/tasks/sessions?project_dir=%2Fwork%2Fproject&page_size=20', expect.anything())
  })

  it('fetches a session detail document by id', async () => {
    const response = { session: { id: 'ses-abcd' }, events: [], checkpoints: [], spec_revisions: [], tasks: [] }
    const fetchMock = vi.fn().mockResolvedValue({ ok: true, json: async () => response })
    vi.stubGlobal('fetch', fetchMock)

    await sessionApi.get('/work/project', 'ses-abcd')

    expect(fetchMock).toHaveBeenCalledWith('/api/tasks/sessions/ses-abcd?project_dir=%2Fwork%2Fproject', expect.anything())
  })

  it('addresses Hub sessions by project id without a workspace path', async () => {
    const fetchMock = vi.fn().mockResolvedValue({ ok: true, json: async () => ({ results: [], next_cursor: '' }) })
    vi.stubGlobal('fetch', fetchMock)

    await sessionApi.list({ projectId: '01HUB' })
    await sessionApi.get(undefined, 'ses-remote', '01HUB')

    expect(fetchMock.mock.calls[0][0]).toBe('/api/tasks/sessions?project_id=01HUB&page_size=20')
    expect(fetchMock.mock.calls[1][0]).toBe('/api/tasks/sessions/ses-remote?project_id=01HUB')
  })
})
