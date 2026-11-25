import type { AgentProgress } from '@/api/agentStream';

export type ExecutionEvent = AgentProgress & { seq?: number; state?: string };
export type ExecutionItem =
  | { type: 'event'; key: number; event: ExecutionEvent }
  | { type: 'tool'; key: number; call?: ExecutionEvent; result?: ExecutionEvent };

// Correlation is scoped to a turn. Sequence numbers identify rows, never tool calls.
export function executionActivity(events: ExecutionEvent[], allowLegacyPairing = false): ExecutionItem[] {
  const items: ExecutionItem[] = [];
  let identified = new Map<string, Extract<ExecutionItem, { type: 'tool' }>>();
  let pending: Extract<ExecutionItem, { type: 'tool' }>[] = [];
  const reset = () => { identified = new Map(); pending = []; };
  events.forEach((event, index) => {
    if (event.kind === 'prompt') { reset(); return; }
    if (event.kind === 'text' || event.kind === 'session') return;
    if (event.kind === 'tool_use' || event.kind === 'tool_result') {
      const id = event.tool_call_id;
      let item = id ? identified.get(id) : undefined;
      if (!id && event.kind === 'tool_result' && event.tool && allowLegacyPairing) {
        const matches = pending.filter(p => !p.result && p.call?.tool === event.tool);
        if (matches.length === 1) item = matches[0];
      }
      if (!item) {
        item = { type: 'tool', key: index };
        items.push(item);
        if (id) identified.set(id, item);
        else if (event.kind === 'tool_use') pending.push(item);
      }
      if (event.kind === 'tool_use') {
        // Some adapters send both active and completed snapshots of a call.
        item.call = { ...item.call, ...event, detail: event.detail || item.call?.detail, text: event.text || item.call?.text };
      } else item.result = event; // Presence, including empty output, means a result arrived.
      return;
    }
    if (event.text?.trim() || event.detail?.trim() || event.state || ['error', 'done', 'turn_done', 'cancelled'].includes(event.kind)) {
      items.push({ type: 'event', key: index, event });
    }
    if (event.kind === 'turn_done' || event.kind === 'done') reset();
  });
  return items;
}
