import { scaleUtc, utcDay } from 'd3'
import type { TaskSchedule, TaskTimelineItem } from '@/api/task'

export type ParsedSchedule = {
  start?: Date
  end?: Date
  due?: Date
  durationDays?: number
}

export type TimelineRow = {
  task: TaskTimelineItem
  dates: ParsedSchedule | null
  error: string | null
  dependencies: { id: string; title?: string }[]
}

/** Use the platform ISO parser: D3 utcParse normalizes 0000-02-29 incorrectly. */
export function parseCivilDate(value: string): Date {
  if (typeof value !== 'string' || !/^\d{4}-\d{2}-\d{2}$/.test(value)) {
    throw new Error('Expected a civil date in YYYY-MM-DD format.')
  }
  const date = new Date(`${value}T00:00:00.000Z`)
  if (!Number.isFinite(date.getTime()) || date.toISOString().slice(0, 10) !== value) {
    throw new Error('Expected a valid calendar date.')
  }
  return date
}

export function civilDateLabel(date: Date): string {
  return date.toISOString().slice(0, 10)
}

export function parseSchedule(schedule: TaskSchedule | null): ParsedSchedule | null {
  if (schedule === null) return null
  if (typeof schedule !== 'object' || !schedule ||
      Object.keys(schedule).some(key => !['start_date', 'end_date', 'due_date'].includes(key))) {
    throw new Error('Expected a task schedule.')
  }
  const start = schedule.start_date === undefined ? undefined : parseCivilDate(schedule.start_date)
  const end = schedule.end_date === undefined ? undefined : parseCivilDate(schedule.end_date)
  const due = schedule.due_date === undefined ? undefined : parseCivilDate(schedule.due_date)
  if (!start && !end && !due) throw new Error('The schedule has no explicit dates.')
  if (start && end && end < start) throw new Error('The end date precedes the start date.')
  return { start, end, due, durationDays: start && end ? utcDay.count(start, end) + 1 : undefined }
}

export function timelineRows(items: readonly TaskTimelineItem[]): TimelineRow[] {
  const loaded = new Map(items.map(task => [task.id, task.title]))
  return items.map(task => {
    let dates: ParsedSchedule | null = null
    let error: string | null = null
    try {
      dates = parseSchedule(task.schedule)
    } catch (cause) {
      error = cause instanceof Error ? cause.message : 'Invalid task dates.'
    }
    return { task, dates, error, dependencies: task.depends_on.map(id => ({ id, title: loaded.get(id) })) }
  })
}

export function calendarAxis(rows: readonly TimelineRow[]) {
  const dates = rows.flatMap(row => row.dates
    ? [row.dates.start, row.dates.end, row.dates.due].filter((date): date is Date => !!date)
    : [])
  if (!dates.length) return null
  const min = new Date(Math.min(...dates.map(Number)))
  const max = new Date(Math.max(...dates.map(Number)))
  // Padding and exclusive bar edge are geometry only, never inferred task dates.
  const scale = scaleUtc([utcDay.offset(min, -1), utcDay.offset(max, 2)], [0, 100])
  const ticks = scale.ticks(6).filter(date => date >= min && date <= max &&
    date.getUTCFullYear() >= 0 && date.getUTCFullYear() <= 9999 && date.getUTCHours() === 0)
  return { scale, ticks, min, max, rangeEnd: (date: Date) => scale(utcDay.offset(date, 1)) }
}

export function scheduleLabel(schedule: TaskSchedule | null): string {
 const dates = parseSchedule(schedule)
 if (!dates) return 'No scheduled dates'
 return [schedule?.start_date && 'Start ' + schedule.start_date, schedule?.end_date && 'End ' + schedule.end_date, schedule?.due_date && 'Due ' + schedule.due_date, dates.durationDays !== undefined && dates.durationDays + (dates.durationDays === 1 ? ' calendar day' : ' calendar days')].filter(Boolean).join(' · ')
}
