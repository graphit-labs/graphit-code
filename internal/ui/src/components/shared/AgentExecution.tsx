import type { AgentProgress } from '@/api/agentStream';
export function appendProgress(events: AgentProgress[], event: AgentProgress): AgentProgress[] {
  return [...events.slice(-299), { ...event, text: event.text?.slice(-12000), detail: event.detail?.slice(-12000) }];
}
export function AgentExecution({ events, running, onCancel }: { events: AgentProgress[]; running: boolean; onCancel?: () => void }) {
  if (!running && !events.length) return null;
  return <div className="agent-execution">
    <details className="work-disclosure" open={running || undefined}>
      <summary>{running ? 'Agent working…' : 'Agent execution'} · {events.length} recent events</summary>
      <div className="agent-execution-output" aria-label="Agent execution output">
        {events.map((event, index) => <div key={index}><small>{event.kind}{event.tool && ` · ${event.tool}`}</small><pre>{event.text || event.detail || 'Working…'}</pre></div>)}
        {!events.length && <p role="status">Waiting for the agent to start…</p>}
      </div>
    </details>
    {running && onCancel && <button className="work-button" onClick={onCancel}>Stop generation</button>}
  </div>;
}
