---
title: Task timeline renderer
type: specification
status: component
updated: 2026-10-11
tags: [task, ui, calendar, react]
references:
  - target: {type: task, id: tsk-50bb}
    relation: implemented_by
  - target: {type: task, id: tsk-f0ec}
    relation: derived_from
  - target: {type: task, id: tsk-b6cd}
    relation: derived_from
  - target: {type: task, id: tsk-c182}
    relation: derived_from
  - target: {type: task, id: tsk-8f4d}
    relation: derived_from
---

# Task timeline renderer

For React integrators, `TaskTimeline` compares explicit dates and dependencies in
already-loaded native Task catalogue rows. It receives data and an optional open
callback; it does not fetch, mutate, schedule, construct routes or make access
decisions. This component delivery does not qualify the complete catalogue page,
authenticated browser transport or Main composition.

The [native Task protocol](rust_task_protocol.md) owns domain and wire semantics.
The [Graphit design system](design_system.md) owns shared visual and interaction
patterns. Source is in `ui/src/components/task/TaskTimeline.tsx`, with
pure date/dependency helpers in `taskTimeline.ts` and local `task-timeline.css`.

## Input and states

`TaskTimelineItem` in `ui/src/api/task.ts` selects actual fields of Rust
`task_store/model.rs::TaskSummary`. It is a projection, not the complete DTO or
a transport adapter. The existing UI `TaskCatalogItem` and fetch remain separate.

```typescript
interface TaskSchedule {
  start_date?: string;
  end_date?: string;
  due_date?: string;
}
interface TaskTimelineItem {
  id: string;
  title: string;
  status: 'open' | 'in_progress' | 'completed' | 'cancelled';
  depends_on: readonly string[];
  schedule: TaskSchedule | null;
}
type TaskTimelineProps =
  | { state: 'loading' }
  | { state: 'error'; message: string }
  | {
      state: 'ready';
      items: readonly TaskTimelineItem[];
      onOpenTask?: (id: string) => void;
    };
```

Import the named `TaskTimeline` export. A native `TaskSummary[]` structurally
satisfies the input projection when its complete wire DTO has been decoded by
the consumer. Cursor, ownership, timestamps and audit dates are not inputs.

| State | Visible result |
| --- | --- |
| `loading` | Loading status, without previous rows |
| `error` | Alert with the supplied message, without previous rows |
| `ready` with no items | Empty state |
| Dated or partially dated items | Scheduled task rows and textual date facts |
| `schedule: null` | Separate **Without dates** section |
| Invalid schedule | Alert and identified task in **Invalid task dates**, without fabricated geometry |

Valid rows remain usable beside invalid rows. Received order is preserved within
each group. The parent consumer owns filters, stale responses, pagination,
forbidden-state classification and retry. No supplied callback means readable
titles and full IDs without inactive buttons. With a callback, native buttons
deliver the exact task/dependency ID; the parent decides navigation and access.

## Civil calendar

Rust `task_schedule.rs::TaskSchedule` emits canonical `YYYY-MM-DD` civil dates
in years 0000–9999. Absent inner fields are omitted in the response. A schedule
must contain at least one date, and an end cannot precede its start. These are
read response types; this component does not implement revision Keep/Clear/set.

`parseCivilDate` checks the ten-character shape, parses an ECMAScript ISO date
with an explicit UTC midnight, then compares the `toISOString()` roundtrip.
This rejects normalization and invalid calendar dates while preserving year
0000 leap days and years below 100. D3 7.9.0 `utcParse('%Y-%m-%d')` normalized
`0000-02-29` to March 1 in the observed runtime, so it is not the input parser.
The component implements no separate calendar or timezone conversion.

D3 `scaleUtc`, `utcDay.count` and `utcDay.offset` provide geometry and arithmetic.
Both interval endpoints are inclusive; equal start/end means one day, and the
full supported interval is 3652425 days. A bar ends at the following day's
visual boundary. This exclusive geometry edge is never an inferred stored date.

| Explicit dates | Representation |
| --- | --- |
| Start and end | Inclusive bar, exact endpoint facts and duration |
| Start only | Start marker, without end or duration |
| End only | End marker, without start or duration |
| Due only | Due marker, without interval or duration |
| Partial endpoint plus due | Separate markers for those actual dates |
| Start/end plus due | Bar and independent due marker, even outside the interval |
| Null schedule | Without dates, without a default calendar |

Only explicit loaded dates determine the extent. One day of visual padding
surrounds geometry; ticks remain within the actual extent and years 0–9999.
Adaptive D3 ticks target about six, use ISO labels and do not create per-day DOM
across unbounded year ranges. Boundary labels stay inside the calendar column.
Empty/unknown-field/malformed/inverted schedules become data errors. Raw date
payloads are not logged, and errors do not silently become undated tasks.

The parser uses the platform ISO calendar with canonical validation:
[ECMAScript Date](https://tc39.es/ecma262/multipage/numbers-and-dates.html#sec-date.parse).
Scaling and day arithmetic follow [D3 time scales](https://d3js.org/d3-scale/time)
and [D3 time intervals](https://d3js.org/d3-time).

## Dependencies, layout and accessibility

B with `depends_on: [A]` depends on A. Its row shows A's full ID and title when
loaded. An absent loaded target is labelled **Outside loaded tasks**; it is not
declared missing, blocked or inaccessible. An optional callback can open that
ID. Dependencies have no invented dates, positions, cycle diagnosis or automatic
date adjustment.

The renderer reuses EngineeringUI `WorkStatusBadge`, `WorkNotice`, `WorkEmpty`
and `LoadingSpinner`, Public Sans and semantic HSL tokens for light/dark themes.
Status has visible text. Partial date and due symbols have shape and a legend;
each row has complete textual date facts. Decorative graphics and axis are
hidden from assistive technology to avoid repeating those facts.

The chart uses sticky task labels, automatically growing rows and a contained,
focusable horizontal scroll region. Long titles/IDs wrap. Metadata is at least
12px; monospace is used for IDs. Native action buttons and the scroll region
have visible focus rings. There is no decorative animation, separate palette,
metric card or mutation control. Shared global reduced-motion rules also apply
to the loading spinner.

## Verification and integration limits

From `ui`, existing tools run the affected checks:

```text
npx --no-install vitest run src/components/task/taskTimeline.test.ts src/components/task/TaskTimeline.test.tsx
npx --no-install eslint src/components/task/TaskTimeline.tsx src/components/task/taskTimeline.ts src/components/task/TaskTimeline.test.tsx src/components/task/taskTimeline.test.ts src/api/task.ts
npm run build
```

Tests cover canonical and invalid dates, leap/century/boundary years, one-day and
full-range duration, due outside the interval, partial/undated/invalid groups,
loaded/outside dependencies, all statuses, controlled states and keyboard open
actions. The same focused tests run with UTC, America/Sao_Paulo and
America/New_York in the subprocess environment, without changing the host.

Private, explicitly labelled component fixtures support browser review at
desktop, 320px/390px, light/dark, enlarged rendering, keyboard focus/scroll and
static reduced-motion behavior. They do not stand in for a real persisted
catalogue or authenticated browser integration. Actual executed results and
limitations are recorded in `tsk-50bb`; parent `tsk-bf2d` retains complete-page
and end-to-end qualification.
