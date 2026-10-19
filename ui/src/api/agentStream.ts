import { openAPIStream } from './client';
import { parseNativeJson } from './nativeJson';
import { NativeRecordReadError } from './nativeRecordRead';
export interface AgentProgress { kind: string; text?: string; tool?: string; tool_call_id?: string; detail?: string; at?: string }
export interface StreamOptions { signal?: AbortSignal; onProgress?: (event: AgentProgress) => void }

function streamError(value: { error?: unknown }, status?: number): Error {
  if (typeof value.error === 'string' && value.error) return new Error(value.error);
  const code = value.error && typeof value.error === 'object' && 'code' in value.error && typeof value.error.code === 'string' ? value.error.code : null;
  if (status !== undefined) return new NativeRecordReadError(status, code);
  return new Error(code || 'Agent request failed');
}

export async function readAgentStream<T, P = AgentProgress>(response: Response, onProgress?: (event: P) => void): Promise<T> {
  if (!response.ok) {
    let failure: Error = new NativeRecordReadError(response.status);
    try {
      failure = streamError(parseNativeJson(await response.text()), response.status);
    } catch { /* Keep the HTTP status when a proxy returns a non-JSON error. */ }
    throw failure;
  }
  if (!response.headers.get('content-type')?.includes('text/event-stream')) {
    const result = parseNativeJson<T & { error?: unknown }>(await response.text());
    if (result.error) throw streamError(result);
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
    const value = parseNativeJson(data.join('\n'));
    if (name === 'progress') onProgress?.(value as P);
    else if (name === 'final') result = value as T;
    else if (name === 'error') throw streamError(value as { error?: unknown });
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
