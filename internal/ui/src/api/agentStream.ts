import { openAPIStream } from './client';
export interface AgentProgress { kind: string; text?: string; tool?: string; detail?: string; at?: string }
export interface StreamOptions { signal?: AbortSignal; onProgress?: (event: AgentProgress) => void }

export async function readAgentStream<T>(response: Response, onProgress?: StreamOptions['onProgress']): Promise<T> {
  if (!response.ok) throw new Error(`Agent request failed (HTTP ${response.status})`);
  if (!response.headers.get('content-type')?.includes('text/event-stream')) {
    const result = await response.json();
    if (result.error) throw new Error(result.error);
    return result as T;
  }
  if (!response.body) throw new Error('Agent stream is unavailable');
  const reader = response.body.getReader(), decoder = new TextDecoder();
  let buffer = '', result: T | undefined;
  function consume(block: string) {
    let name = 'message'; const data: string[] = [];
    for (const line of block.split('\n')) {
      if (line.startsWith('event:')) name = line.slice(6).trim();
      if (line.startsWith('data:')) data.push(line.slice(5).trimStart());
    }
    if (!data.length) return;
    const value = JSON.parse(data.join('\n'));
    if (name === 'progress') onProgress?.(value);
    else if (name === 'final') result = value;
    else if (name === 'error') throw new Error(value.error || 'Agent request failed');
  }
  try {
    while (true) {
      const { value, done } = await reader.read();
      buffer += decoder.decode(value, { stream: !done });
      // Normalize complete CRLF pairs only; a CR can be split across chunks.
      buffer = buffer.replace(/\r\n/g, '\n');
      let end: number;
      while ((end = buffer.indexOf('\n\n')) >= 0) { consume(buffer.slice(0, end)); buffer = buffer.slice(end + 2); }
      if (done) break;
    }
    if (buffer.trim()) consume(buffer);
    if (result === undefined) throw new Error('The agent stream ended without a final response');
    return result;
  } finally { await reader.cancel().catch(() => {}); reader.releaseLock(); }
}
export async function postAgentStream<T>(path: string, body: unknown, options: StreamOptions = {}): Promise<T> {
  const response = await openAPIStream(path, body, options.signal);
  return readAgentStream<T>(response, options.onProgress);
}
