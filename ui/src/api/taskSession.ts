import { taskRead, type LifecycleKind, type Ownership, type Progress, type ProgressObservation, type TaskSelection, type TaskStatus, type TaskGeneration, type TaskAuditPageRequest } from './task'
import type { NativeInteger } from './nativeJson'
export interface SessionInput { idempotency_key: string; title: string; description: string; strategy: string }
export interface Session { id: string; input: SessionInput; revision: NativeInteger; status: TaskStatus; ownership: Ownership; progress: Progress | null; created_at: NativeInteger; updated_at: NativeInteger }
export interface SessionHead { id: string; title: string; status: TaskStatus; revision: NativeInteger; ownership: Ownership; progress: ProgressObservation | null; completed_tasks: NativeInteger; total_tasks: NativeInteger; updated_at: NativeInteger }
export interface SessionEvent { actor: string; kind: LifecycleKind; observed_at: NativeInteger; record: Session }
export interface SessionHistoryPage { before: Session; events: SessionEvent[]; next_cursor: string | null }
export interface SessionDetail {
 session: Session; creation: { actor: string; record: Session }; last_event: SessionEvent | null;
 generation: TaskGeneration; history_start_cursor: string; history: SessionHistoryPage | null
}
export interface SessionGetOptions { history?: TaskAuditPageRequest; signal?: AbortSignal }
export const sessionApi = {
 heads: (selection: TaskSelection, signal?: AbortSignal) => taskRead<SessionHead[]>('session-heads', selection, undefined, signal),
 get: (selection: TaskSelection, id: string, { signal, ...sections }: SessionGetOptions = {}) => taskRead<SessionDetail | null>('session', selection, { id, ...sections }, signal),
}
