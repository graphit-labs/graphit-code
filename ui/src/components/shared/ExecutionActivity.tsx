import { executionActivity, type ExecutionEvent } from './executionActivityModel';

function Payload({ event, empty }: { event: ExecutionEvent; empty?: string }) {
  return <>
    {event.text && <pre>{event.text}</pre>}
    {event.detail && event.detail !== event.text && <pre>{event.detail}</pre>}
    {!event.text && !event.detail && empty && <p className="execution-muted">{empty}</p>}
  </>;
}

export function ExecutionActivity({ events, allowLegacyPairing = false }: { events: ExecutionEvent[]; allowLegacyPairing?: boolean }) {
  const items = executionActivity(events, allowLegacyPairing);
  if (!items.length) return <p className="execution-muted">No execution details yet.</p>;
  return <div className="work-timeline execution-activity" aria-label="Execution activity entries">
    {items.map(item => item.type === 'tool' ? <article key={item.key} aria-label={`Tool · ${item.call?.tool || item.result?.tool || 'Unnamed tool'}`}>
      <header className="execution-tool-heading">
        <strong>{item.call?.tool || item.result?.tool || 'Tool'}</strong>
        <small>{!item.call ? 'Unmatched result' : item.result ? 'Result received' : 'No result yet'}</small>
      </header>
      {(item.call?.tool_call_id || item.result?.tool_call_id) && <small className="execution-call-id">{item.call?.tool_call_id || item.result?.tool_call_id}</small>}
      {item.call && <details className="execution-tool-payload"><summary>Input</summary><Payload event={item.call} empty="No input recorded." /></details>}
      {item.result && <details className="execution-tool-payload"><summary>Result</summary><Payload event={item.result} empty="No output returned." /></details>}
      {!item.call && <p className="execution-muted">No matching call could be identified in this turn.</p>}
    </article> : <article key={item.key}>
      <small>{item.event.kind.replace(/_/g, ' ')}{item.event.state && ` · ${item.event.state}`}</small>
      <Payload event={item.event} />
    </article>)}
  </div>;
}
