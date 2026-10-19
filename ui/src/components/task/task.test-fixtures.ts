import type { Task, TaskContext, TaskDetail, TaskSummary } from '@/api/task'
import type { SessionDetail, SessionHead, SessionHistoryPage } from '@/api/taskSession'
export const context: TaskContext = { project_dir: "/p", project_id: 'demo', project_version: 'main' }
export const createdAt = Date.parse('2026-10-11T12:34:56.789Z')
export const leaseExpiresAt = Date.parse('2026-10-11T13:34:56.789Z')
export const ownership = { owner: 'agent-a', claim_epoch: 1, claimed_at: createdAt, heartbeat_at: createdAt + 1000, lease_expires_at: leaseExpiresAt }
const check = { id: 'check1', kind: 'acceptance' as const, text: 'The UI shows the **observable outcome**.', status: 'passed' as const, verification: { evidence: 'Verified with `npm test`.', verified_by: 'agent-a', occurred_at: 12 }, supersession: null }
export const first: Task = { commit: null, specification: { id: 'task1', revision: 2, title: 'First task', description: '# Objective\n\nBuild the **first deterministic feature**.\n\n- Preserve history\n- Render rich fields\n\nRun `cargo test` and inspect [the contract](https://example.com/contract).', task_type: 'TASK', priority: 1, schedule: { start_date: '2024-02-28', end_date: '2024-03-01', due_date: '2024-03-02' }, checks: [check] }, session_id: 'session1', parent_id: null, depends_on: [], idempotency_key: 'first', status: 'in_progress', flagged: false, ownership, progress: { observation: { kind: 'progress', sequence: 1, observed_at: 12 }, summary: '**Core** landed.', next_step: 'Verify the **Task Explorer** UI.' }, created_at: createdAt, updated_at: 12 }
export const second: Task = { ...first, specification: { ...first.specification, id: 'task2', revision: 1, title: 'Second task', description: 'A follow-up.', schedule: null }, status: 'open', depends_on: ['task1'], progress: null, ownership: { owner: null, claim_epoch: 0, claimed_at: null, heartbeat_at: null, lease_expires_at: null } }
export function summary(task: Task): TaskSummary { return { id: task.specification.id, title: task.specification.title, session_id: task.session_id, task_type: task.specification.task_type, priority: task.specification.priority, status: task.status, parent_id: task.parent_id, depends_on: task.depends_on, schedule: task.specification.schedule, ownership: task.ownership, progress: task.progress?.observation ?? null, flagged: task.flagged, ready: false, revision: task.specification.revision, created_at: task.created_at, updated_at: task.updated_at } }
export const before = { ...first, specification: { ...first.specification, revision: 1, description: 'Initial **specification**.', schedule: null } }
export const historyPage = { before, events: [{ actor: 'agent-a', kind: { revised: { reason: 'Clarified the **observable behavior**.' } }, observed_at: 11, record: first }], next_cursor: null }
export const commentsPage = { comments: [{ id: 'comment1', task_id: 'task1', session_id: 'session1', idempotency_key: 'decision', sequence: 1, kind: 'decision' as const, body: 'Use the **native producer**.', actor: 'agent-a', at: 12, revision: 2 }], next_cursor: null }
export const detail: TaskDetail = { task: first, creation: { actor: 'planner', record: before }, last_event: historyPage.events[0], generation: { version: 7, physical_branch: null, inherited_target: null }, history_start_cursor: 'history-start-7', comments_start_cursor: 'comments-start-7', history: null, comments: null }
export const secondDetail: TaskDetail = { ...detail, task: second, creation: { actor: 'planner', record: second }, last_event: null, history_start_cursor: 'task2-history-7', comments_start_cursor: 'task2-comments-7' }

export const cursor = { root: context.project_dir, project_version: 'main', version: 2, filter: {}, offset: 1 }
export const sessionDetail: SessionDetail = {
 session: { id: 'session1', input: { idempotency_key: 's1', title: 'First session', description: 'Deliver the **native UI**.', strategy: 'Use the **actual producer**.' }, revision: 4, status: 'in_progress', ownership, progress: { observation: { kind: 'checkpoint', sequence: 1, observed_at: createdAt + 2000 }, summary: '**Checkpoint** saved.', next_step: 'Read **next action**.' }, created_at: createdAt, updated_at: createdAt + 3000 },
 creation: { actor: 'planner', record: { id: 'session1', input: { idempotency_key: 's1', title: 'Initial session', description: 'Initial request.', strategy: 'Initial strategy.' }, revision: 1, status: 'open', ownership: { owner: null, claim_epoch: 0, claimed_at: null, heartbeat_at: null, lease_expires_at: null }, progress: null, created_at: createdAt, updated_at: createdAt } },
 last_event: null, generation: { version: 7, physical_branch: null, inherited_target: null }, history_start_cursor: 'session-history-start', history: null,
}
const claimedSession = { ...sessionDetail.creation.record, revision: 2, status: 'in_progress' as const, ownership, updated_at: createdAt + 1000 }
const checkpointSession = { ...claimedSession, revision: 3, progress: sessionDetail.session.progress, updated_at: createdAt + 2000 }
export const sessionHistoryPage: SessionHistoryPage = { before: sessionDetail.creation.record, next_cursor: null, events: [
 { actor: 'agent-a', kind: 'claimed', observed_at: createdAt + 1000, record: claimedSession },
 { actor: 'agent-a', kind: 'progress', observed_at: createdAt + 2000, record: checkpointSession },
 { actor: 'agent-a', kind: { revised: { reason: 'Explicit scope.' } }, observed_at: createdAt + 3000, record: sessionDetail.session },
] }
sessionDetail.last_event = sessionHistoryPage.events[2]
export const sessionHead: SessionHead = { id: 'session1', title: 'First session', status: 'in_progress', revision: sessionDetail.session.revision, ownership, progress: sessionDetail.session.progress!.observation, completed_tasks: 0, total_tasks: 2, updated_at: sessionDetail.session.updated_at }
