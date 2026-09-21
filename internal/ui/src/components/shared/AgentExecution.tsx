import type { AgentProgress } from '@/api/agentStream';
import { ExecutionActivity } from './ExecutionActivity';
export type ExecutionOutcome = 'idle' | 'completed' | 'failed' | 'cancelled';
export function appendProgress(events: AgentProgress[], event: AgentProgress): AgentProgress[] {
  return [...events.slice(-299), { ...event, text: event.text?.slice(-12000), detail: event.detail?.slice(-12000) }];
}

export function executionEventLabel(event: AgentProgress): string | undefined {
  if (['prep', 'status'].includes(event.kind) && event.text?.trim()) return event.text.trim().replace(/(?:\.{3}|…)$/, '');
  if (event.kind === 'tool_use' && event.tool) return `Using ${event.tool}`;
  if (event.kind === 'tool_result') return event.tool ? `Received ${event.tool} result` : 'Processing tool result';
  if (event.kind === 'thinking' && event.text?.trim()) return 'Processing';
  if (event.kind === 'stdout' && event.text?.trim()) return 'Receiving agent output';
  if (event.kind === 'text' && event.text?.trim()) return 'Writing the answer';
}

export function currentExecutionLabel(events: AgentProgress[]): string {
  for (let index = events.length - 1; index >= 0; index--) {
    const label = executionEventLabel(events[index]);
    if (label) return label;
  }
  return 'Waiting for the agent to start';
}

export function AgentExecution({ events, running, onCancel, outcome = 'idle', currentEvent }: { events: AgentProgress[]; running: boolean; onCancel?: () => void; outcome?: ExecutionOutcome; currentEvent?: AgentProgress }) {
  if (!running && !events.length && outcome === 'idle') return null;
  const terminal = { idle: 'Agent execution', completed: 'Execution complete', failed: 'Execution failed', cancelled: 'Execution stopped' };
  const label = running ? currentExecutionLabel(currentEvent ? [...events, currentEvent] : events) : terminal[outcome];
  return <div className="agent-execution">
    <div className="agent-execution-status-row">
      <p className="agent-execution-status" role="status" aria-live="polite" aria-atomic="true">
        {label}{running && <span className="execution-dots" aria-hidden="true"><span>.</span><span>.</span><span>.</span></span>}
      </p>
      {running && onCancel && <button className="work-button" onClick={onCancel}>Stop generation</button>}
    </div>
    {events.length > 0 && <details className="work-disclosure">
      <summary>Execution details</summary>
      <div className="agent-execution-output" aria-label="Agent execution output">
        <ExecutionActivity events={events.slice(-300).map(event => ({ ...event, text: event.text?.slice(-12000), detail: event.detail?.slice(-12000) }))} />
      </div>
    </details>}
  </div>;
}
