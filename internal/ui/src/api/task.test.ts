import { afterEach, describe, expect, it, vi } from 'vitest'

import { taskApi } from './task'

describe('Task API', () => {
  afterEach(() => {
    vi.unstubAllGlobals()
  })

  it('uses the canonical export endpoint for all and exact task documents', async () => {
    const response = {
      schema_version: 1, project_id: 'project-1', tasks: [], dependencies: [], checks: [],
      events: [], comments: [], spec_revisions: [],
    }
    const fetchMock = vi.fn().mockResolvedValue({ ok: true, json: async () => response })
    vi.stubGlobal('fetch', fetchMock)

    await taskApi.export('/work/project')
    await taskApi.export('/work/project', 'tsk-abcd')

    expect(fetchMock.mock.calls[0][0]).toBe('/api/tasks/export?project_dir=%2Fwork%2Fproject')
    expect(fetchMock.mock.calls[1][0]).toBe('/api/tasks/export?project_dir=%2Fwork%2Fproject&id=tsk-abcd')
  })

  it('uses bounded catalogue parameters and an opaque cursor', async () => {
    const response = { results: [], next_cursor: 'next-page' }
    const fetchMock = vi.fn().mockResolvedValue({ ok: true, json: async () => response })
    vi.stubGlobal('fetch', fetchMock)

    await taskApi.list({
      projectDir: '/work/project',
      query: 'scheduler',
      status: 'blocked',
      pageSize: 25,
      cursor: 'opaque-token',
    })

    expect(fetchMock).toHaveBeenCalledWith(
      '/api/tasks?project_dir=%2Fwork%2Fproject&query=scheduler&status=blocked&page_size=25&cursor=opaque-token',
      expect.anything(),
    )
  })
})
