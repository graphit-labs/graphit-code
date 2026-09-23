import { act, render, screen, waitFor } from '@testing-library/react'
import { beforeEach, describe, expect, it, vi } from 'vitest'

import { dreamApi, type DreamStatus } from '@/api/dream'
import { useAppStore } from '@/store/appStore'
import DreamDashboard from './DreamDashboard'

vi.mock('@/api/dream', () => ({ dreamApi: { getStatus: vi.fn() } }))

const baseStatus: DreamStatus = {
  enabled: true,
  daemon_running: true,
  status: 'standby',
  idle_timeout: '1h',
  max_duration: '2h',
}

describe('Dream dashboard', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    useAppStore.setState({ activeProjectDir: '/project-a' })
  })

  it('shows the latest operational run without a report catalogue', async () => {
    vi.mocked(dreamApi.getStatus).mockResolvedValue({
      ...baseStatus,
      last_run: {
        run_id: 'run-1', status: 'completed', started_at: '2026-09-23T12:00:00Z',
        tool_calls: 3, memory_mutation_attempts: 1, target_ids: ['memory-1'],
      },
    })
    render(<DreamDashboard />)

    expect(await screen.findByText('run-1')).toBeTruthy()
    expect(screen.getByText('memory-1')).toBeTruthy()
    expect(screen.queryByText('Maintenance reports')).toBeNull()
  })

  it('clears a previous project run while loading the next project', async () => {
    let resolveNext!: (value: DreamStatus) => void
    vi.mocked(dreamApi.getStatus).mockImplementation(projectDir =>
      projectDir === '/project-a'
        ? Promise.resolve({ ...baseStatus, last_run: { run_id: 'run-a', status: 'completed', started_at: '2026-09-23T12:00:00Z', tool_calls: 1, memory_mutation_attempts: 0 } })
        : new Promise(resolve => { resolveNext = resolve }),
    )
    render(<DreamDashboard />)
    expect(await screen.findByText('run-a')).toBeTruthy()

    act(() => useAppStore.setState({ activeProjectDir: '/project-b' }))
    await waitFor(() => expect(screen.queryByText('run-a')).toBeNull())
    await act(async () => resolveNext(baseStatus))
    expect(await screen.findByText('No Dream runs yet')).toBeTruthy()
  })

  it('shows a request error when status is unavailable', async () => {
    vi.mocked(dreamApi.getStatus).mockRejectedValue(new Error('connection failed'))
    render(<DreamDashboard />)
    expect(await screen.findByText('Dream status unavailable')).toBeTruthy()
    expect(screen.getByText('connection failed')).toBeTruthy()
  })
})
