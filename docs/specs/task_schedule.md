---
references:
  - target: {type: task, id: tsk-b6cd}
    relation: implements
  - target: {type: task, id: tsk-f0ec}
    relation: derived_from
---

# Explicit Task calendar dates

CalendarDate represents a civil day in exact YYYY-MM-DD form, with four ASCII year digits from 0000 through 9999. Chrono NaiveDate validates the actual calendar, including leap days. Whitespace, signed/wider years, shortened components, timestamps, invalid days and normalization are rejected. Dates have no timezone and do not become instants.

TaskSchedule contains optional start_date, end_date and due_date. At least one actual date is required; an empty object or all-null object rejects. When both endpoints exist, end_date must not precede start_date. Due date is independent and may fall outside the interval. Unknown and duplicate JSON fields reject. Serialization omits absent inner dates.

A complete interval has an inclusive duration of end minus start plus one civil day. A same-day interval lasts one day. A partial interval or due-only schedule has no duration: a view must display labelled markers rather than infer a missing endpoint.

TaskSpecification.schedule is Option<TaskSchedule>, with None meaning unscheduled. RevisionCommand.schedule uses FieldUpdate<Option<TaskSchedule>>:

| Input in a consumer DTO | Create | Revise |
|---|---|---|
| schedule omitted | None | Keep |
| schedule: null | None | Set(None), clear |
| Valid schedule object | Some(schedule) | Set(Some(schedule)), replace whole object |
| Empty/all-null or invalid object | Error | Error |

For a revision DTO, use #[serde(default)] on its FieldUpdate field to distinguish omission from explicit null. FieldUpdate deserializes each present value through its real typed codec. The unit fixtures demonstrate this typed-field contract; they are not live transport DTOs or persistence tests. The future native service must use these codecs and preserve ordinary fencing, auditing and request-error behavior.

Changing only schedule preserves check results and historical evidence while advancing revision/audit. Title, description and task_type changes still reset active checks. The planner copies schedule through verification and supersession. Audit timestamps and dependency edges never create or shift an agenda.

Use task_schedule::{CalendarDate, TaskSchedule} to construct validated data. The constructor takes three Option<CalendarDate> values; its accessors return those values unchanged. inclusive_duration_days() returns a value only for two endpoints. TaskSchedule fields are private, so invalid or empty schedules cannot be constructed by bypassing validation.
