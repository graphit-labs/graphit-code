import { describe, expect, it } from 'vitest'
import type { TaskSchedule, TaskTimelineItem } from '@/api/task'
import { calendarAxis, civilDateLabel, parseCivilDate, parseSchedule, timelineRows } from './taskTimeline'

const task = (schedule: TaskSchedule | null): TaskTimelineItem => ({
  id: '00000000000000000000000001', title: 'Calendar task', status: 'open', depends_on: [], schedule,
})

describe('native civil dates', () => {
  it.each(['0000-02-29', '0099-01-01', '2000-02-29', '2024-02-29', '9999-12-31'])('preserves %s in UTC', value => {
    const date = parseCivilDate(value)
    expect(civilDateLabel(date)).toBe(value)
    expect(date.getUTCHours()).toBe(0)
  })
  it.each(['2023-02-29', '2100-02-29', '2026-04-31', '2026-13-01', '2026-00-01', '2026-01-00',
    '2026-1-01', '26-01-01', '10000-01-01', '-001-01-01', '2026-01-01T00:00:00Z', '', ' 2026-01-01'])('rejects %s without normalization', value => {
    expect(() => parseCivilDate(value)).toThrow()
  })
  it.each([
    ['2026-10-11', '2026-10-11', 1], ['2024-02-28', '2024-03-01', 3],
    ['2025-12-31', '2026-01-01', 2], ['0000-01-01', '9999-12-31', 3652425],
  ])('counts inclusive civil days %s through %s', (start_date, end_date, expected) => {
    expect(parseSchedule({ start_date, end_date })?.durationDays).toBe(expected)
  })
  it('retains due before or after the range and creates no duration for partial dates', () => {
    for (const due_date of ['2026-01-01', '2026-12-31']) {
      const dates = parseSchedule({ start_date: '2026-10-11', end_date: '2026-10-13', due_date })!
      expect(civilDateLabel(dates.due!)).toBe(due_date)
      expect(dates.durationDays).toBe(3)
    }
    for (const schedule of [{ start_date: '2026-10-11' }, { end_date: '2026-10-11' },
      { due_date: '2026-10-11' }, { start_date: '2026-10-11', due_date: '2026-10-09' }]) {
      expect(parseSchedule(schedule)?.durationDays).toBeUndefined()
    }
    expect(parseSchedule(null)).toBeNull()
  })
  it('distinguishes malformed schedules from undated tasks', () => {
    for (const schedule of [{}, { start_date: '2026-10-12', end_date: '2026-10-11' },
      { due_date: '' }, { start_date: null }, { due_date: '2026-10-11', unknown: true }]) {
      expect(() => parseSchedule(schedule as TaskSchedule)).toThrow()
    }
    const rows = timelineRows([task(null), { ...task({}), id: '00000000000000000000000002' }])
    expect(rows[0].error).toBeNull()
    expect(rows[1].error).not.toBeNull()
    expect(calendarAxis(rows)).toBeNull()
  })
})

describe('loaded catalogue geometry and dependencies', () => {
  it('uses actual dates only, retaining due and an exclusive geometry edge', () => {
    const axis = calendarAxis(timelineRows([task({ start_date: '2026-10-11', end_date: '2026-10-11', due_date: '2026-10-09' })]))!
    expect(civilDateLabel(axis.min)).toBe('2026-10-09')
    expect(civilDateLabel(axis.max)).toBe('2026-10-11')
    expect(axis.rangeEnd(parseCivilDate('2026-10-11'))).toBeGreaterThan(axis.scale(parseCivilDate('2026-10-11')))
    expect(axis.ticks.length).toBeLessThanOrEqual(10)
    expect(axis.ticks.every(date => date >= axis.min && date <= axis.max)).toBe(true)
  })
  it('has bounded unique valid ticks across the full calendar, without year10000 wrapping', () => {
    for (const schedule of [{ start_date: '0000-01-01', end_date: '9999-12-31' }, { due_date: '9999-12-31' }, { start_date: '0000-02-29' }]) {
      const axis = calendarAxis(timelineRows([task(schedule)]))!
      const labels = axis.ticks.map(civilDateLabel)
      expect(axis.ticks.length).toBeGreaterThan(0)
      expect(axis.ticks.length).toBeLessThanOrEqual(10)
      expect(new Set(labels).size).toBe(labels.length)
      expect(axis.ticks.every(date => date.getUTCFullYear() >= 0 && date.getUTCFullYear() <= 9999)).toBe(true)
    }
  })
  it('preserves order and dependency direction without declaring an outside target missing', () => {
    const predecessor = task(null)
    const dependent = { ...task({ due_date: '2026-10-11' }), id: '00000000000000000000000002', depends_on: [predecessor.id, '00000000000000000000000003'] }
    const rows = timelineRows([dependent, predecessor])
    expect(rows.map(row => row.task.id)).toEqual([dependent.id, predecessor.id])
    expect(rows[0].dependencies).toEqual([{ id: predecessor.id, title: predecessor.title }, { id: '00000000000000000000000003', title: undefined }])
    expect(rows[1].dependencies).toEqual([])
  })
})
