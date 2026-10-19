import { cleanup, render, screen, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, describe, expect, it, vi } from 'vitest'
import type { TaskSchedule, TaskTimelineItem } from '@/api/task'
import { TaskTimeline } from './TaskTimeline'

afterEach(cleanup)
const item = (id: number, title: string, schedule: TaskSchedule | null): TaskTimelineItem => ({
  id: id.toString().padStart(26, '0'), title, schedule, status: 'open', depends_on: [],
})

describe('TaskTimeline native projection renderer', () => {
  it('shows one-day duration, partial markers and due outside a range with complete text alternatives', () => {
    const { container } = render(<TaskTimeline state="ready" items={[
      item(1, 'One day', { start_date: '2026-10-11', end_date: '2026-10-11', due_date: '2026-10-09' }),
      item(2, 'Start only', { start_date: '2026-10-10' }),
      item(3, 'End only', { end_date: '2026-10-12' }), item(4, 'Due only', { due_date: '2026-10-14' }),
    ]} />)
    const section = screen.getByRole('region', { name: 'Scheduled tasks' })
    expect(within(section).getByText('1 day inclusive')).toBeTruthy()
    expect(container.querySelectorAll('.task-timeline-range').length).toBe(1)
    expect(container.querySelectorAll('.task-timeline-marker.start').length).toBe(1)
    expect(container.querySelectorAll('.task-timeline-marker.end').length).toBe(1)
    expect(container.querySelectorAll('.task-timeline-marker.due').length).toBe(2)
    const facts = container.querySelectorAll('.task-timeline-facts')
    expect(facts[0].textContent).toContain('Due2026-10-09')
    expect(facts[1].textContent).not.toContain('Duration')
    expect(facts[2].textContent).not.toContain('Duration')
    expect(facts[3].textContent).toBe('Due2026-10-14')
    expect(container.querySelectorAll('.task-timeline-track[aria-hidden="true"]').length).toBe(4)
  })
  it('keeps undated tasks visible and opens task/dependency IDs by keyboard without routing', async () => {
    const user = userEvent.setup()
    const onOpenTask = vi.fn()
    const predecessor = item(1, 'Undated prerequisite', null)
    const dependent = { ...item(2, 'Dependent', { due_date: '2026-10-11' }), depends_on: [predecessor.id, item(3, 'Outside', null).id] }
    render(<TaskTimeline state="ready" items={[dependent, predecessor]} onOpenTask={onOpenTask} />)
    const undated = screen.getByRole('region', { name: 'Without dates' })
    const taskButton = within(undated).getByRole('button', { name: predecessor.title })
    taskButton.focus()
    await user.keyboard('{Enter}')
    expect(onOpenTask).toHaveBeenLastCalledWith(predecessor.id)
    expect(screen.getByText('Outside loaded tasks')).toBeTruthy()
    const external = screen.getByRole('button', { name: `Open dependency ${dependent.depends_on[1]}` })
    external.focus()
    await user.keyboard(' ')
    expect(onOpenTask).toHaveBeenLastCalledWith(dependent.depends_on[1])
    expect(screen.queryByText(/missing/i)).toBeNull()
    expect(within(undated).queryByText('Duration')).toBeNull()
  })
  it('shows invalid rows as errors, rather than silently putting them in Without dates', () => {
    const { container } = render(<TaskTimeline state="ready" items={[
      item(1, 'Bad date', { start_date: '2023-02-29' }), item(2, 'Empty schedule', {}),
      item(3, 'Valid leap', { start_date: '0000-02-29', end_date: '0000-02-29' }),
    ]} />)
    expect(screen.getByRole('alert').textContent).toContain('Some task dates are invalid')
    expect(screen.getByRole('region', { name: 'Invalid task dates' }).textContent).toContain('Bad date')
    expect(screen.queryByRole('region', { name: 'Without dates' })).toBeNull()
    expect(container.querySelector('.task-timeline-range')).toBeTruthy()
    expect(screen.getByText('1 day inclusive')).toBeTruthy()
  })
  it('does not create an axis or actionable buttons when all tasks have no dates and no callback', () => {
    const { container } = render(<TaskTimeline state="ready" items={[item(1, 'No date', null)]} />)
    expect(screen.getByRole('region', { name: 'Without dates' })).toBeTruthy()
    expect(container.querySelector('.task-timeline-axis')).toBeNull()
    expect(screen.queryByRole('button')).toBeNull()
  })
  it('replaces rows for controlled loading/error/empty states and preserves provided error text', () => {
    const { rerender } = render(<TaskTimeline state="ready" items={[item(1, 'Previous task', null)]} />)
    rerender(<TaskTimeline state="loading" />)
    expect(screen.getByRole('status').textContent).toContain('Loading task dates')
    expect(screen.queryByText('Previous task')).toBeNull()
    rerender(<TaskTimeline state="error" message="Access denied for the selected project." />)
    expect(screen.getByRole('alert').textContent).toContain('Access denied for the selected project.')
    expect(screen.queryByRole('status')).toBeNull()
    rerender(<TaskTimeline state="ready" items={[]} />)
    expect(screen.getByRole('heading', { name: 'No tasks to display' })).toBeTruthy()
    expect(screen.queryByRole('alert')).toBeNull()
  })
  it('shows all native statuses and long titles/IDs without shortening their text', async () => {
    const user = userEvent.setup()
    const statuses = ['open', 'in_progress', 'completed', 'cancelled'] as const
    const title = 'An explicit calendar task with a long title that remains understandable at narrow widths'
    const { container } = render(<TaskTimeline state="ready" onOpenTask={vi.fn()} items={statuses.map((status, i) => ({
      ...item(i + 1, `${title} ${i}`, { due_date: '9999-12-31' }), status,
    }))} />)
    for (const status of statuses) expect(screen.getByText(status.replace(/_/g, ' '))).toBeTruthy()
    expect(screen.getByRole('button', { name: `${title} 0` })).toBeTruthy()
    expect(screen.getByText(item(1, '', null).id)).toBeTruthy()
    await user.tab()
    expect(document.activeElement).toBe(container.querySelector('.task-timeline-scroll'))
    await user.tab()
    expect(document.activeElement).toBe(screen.getByRole('button', { name: `${title} 0` }))
  })
})
