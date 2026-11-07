import { useState } from 'react'
import type { GraphNode } from '@/api/ast'
import { stringifyNativeJson } from '@/api/nativeJson'
import { FactList, WorkNotice } from '@/components/shared/EngineeringUI'
import './native-entity-details.css'

const observed = (value: unknown) => typeof value === 'string' || typeof value === 'number' || typeof value === 'bigint' ? String(value) : null

export function NativeEntityDetails({ node, scope }: { node: GraphNode; scope?: string }) {
  const properties = node.properties ?? {}
  const uid = typeof properties.uid === 'string' ? properties.uid : null
  const details = stringifyNativeJson({
    name: node.name, type: node.label, file: node.file ?? observed(properties.path),
    line: node.line ?? observed(properties.start_line), scope: scope ?? null,
    uid, primary_key: node.nativeKey ?? null, native_identity: node.nativeIdentity ?? null,
    properties,
  }, 2)
  const [feedback, setFeedback] = useState<{ details: string; error?: string; copied?: string } | null>(null)
  const copy = async (label: string, value: string) => {
    try { await navigator.clipboard.writeText(value); setFeedback({ details, copied: `${label} copied.` }) }
    catch (cause) { setFeedback({ details, error: cause instanceof Error ? cause.message : 'Clipboard unavailable. Select the value manually.' }) }
  }
  const extra: Array<[string, string]> = [
    ['Language', observed(properties.lang) ?? observed(properties.language)],
    ['Grammar', observed(properties.grammar)],
    ['Extent status', observed(properties.extent_status)],
    ['End line', observed(properties.end_line)],
    ['Source version', observed(properties.raw_version)],
  ].filter((item): item is [string, string] => item[1] !== null)
  return <div className="native-entity-details">
    <FactList items={[
      ['Entity name', node.name], ...(scope ? [['Scope', scope] as [string, string]] : []),
      ['Entity type', node.label], ['Source file', node.file || observed(properties.path)],
      ['Start line', node.line ?? observed(properties.start_line)],
    ]} />
    <div className="native-entity-identity">
      <dl><dt>UID</dt><dd>{uid === null ? 'Unavailable' : <code title={uid}>{uid.length > 64 ? `${uid.slice(0, 40)}…${uid.slice(-16)}` : uid}</code>}</dd></dl>
      <button className="work-button" disabled={uid === null} onClick={() => void copy('UID', uid!)}>Copy UID</button>
    </div>
    {feedback?.details === details && (feedback.error
      ? <WorkNotice title="Unable to copy entity" tone="error">{feedback.error}</WorkNotice>
      : <p role="status">{feedback.copied}</p>)}
    <details className="work-disclosure">
      <summary>Technical entity details</summary>
      {extra.length > 0 && <FactList items={extra} />}
      <dl>
        <dt>Complete UID</dt><dd>{uid === null ? 'Unavailable' : <code>{uid}</code>}</dd>
        {node.nativeKey && <><dt>Primary key</dt><dd><code>{node.nativeKey.value}</code> ({node.nativeKey.type})</dd></>}
        {node.nativeIdentity && <>
          <dt>Native table</dt><dd>{node.nativeIdentity.table}</dd>
          <dt>Native row</dt><dd><code>{node.nativeIdentity.row}</code></dd>
        </>}
      </dl>
      {uid === '' && <p>The recorded UID is an empty string.</p>}
      <div className="work-actions">
        <button className="work-button" disabled={!node.nativeKey} onClick={() => void copy('Primary key', node.nativeKey!.value)}>Copy primary key</button>
        <button className="work-button" onClick={() => void copy('Entity details', details)}>Copy entity details</button>
      </div>
      <pre className="work-code">{details}</pre>
    </details>
  </div>
}
