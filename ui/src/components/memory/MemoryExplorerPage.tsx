import { RecordReferences } from '@/components/shared/RecordReferences'
import { projectReferenceBinding, userMemoryReferenceBinding, type ReferenceBinding } from '@/api/nativeRecordReferences'
import { referenceDestinationError } from '@/api/nativeReferenceNavigation'
import { StyledSelect } from "@/components/shared/StyledSelect";
import { usePageRefresh, refreshAll } from "@/components/layout/WorkspaceRefresh";
import {
  WorkBadge,
  WorkPage,
  WorkHeader,
  WorkSection,
  WorkSearch,
  WorkEmpty,
  WorkTabs,
  WorkNotice,
  FactList,
} from "@/components/shared/EngineeringUI";
import { ModalPortal } from "@/components/shared/ModalPortal";
import { useCallback, useEffect, useLayoutEffect, useRef, useState } from "react";
import { useLocation, useNavigate, useParams } from "react-router-dom";
import { Flag, ShieldCheck } from "lucide-react";

import { nativeMemoryMutationApi, type MemoryDraft, type MemoryMutationResult } from '@/api/nativeMemoryMutation';
type MemoryScope = 'project' | 'user';
import { nativeMemoryApi, type NativeMemory, type NativeMemoryHistory, type NativeMemoryList, type MemoryEnvelope, type MemoryReadScope, type MemoryClassification, type NativeMemorySearch, type MemorySearchMode } from '@/api/nativeMemory'
import { encodeProjectLocator, nativeSelectionKey, projectRecordHref, requestedRecordSelection, sameProjectVersion, userMemoryHref } from '@/api/nativeRecordNavigation'
import { nativeTimestamp, stringifyNativeJson } from '@/api/nativeJson'
import { useTaskWorkspace } from '@/components/task/taskWorkspaceState'
import { MarkdownContent } from '@/components/wiki/WikiMarkdown'
import { LoadingSpinner } from "@/components/shared/LoadingSpinner";
import { MemoryMarkdown } from "./MemoryMarkdown";

const memoryTypes = [
  "fact",
  "decision",
  "convention",
  "correction",
  "tension",
  "skill",
];

function TypeBadge({ type }: { type: string }) {
  return <WorkBadge>{type || "untyped"}</WorkBadge>;
}

function MemoryFlags({ important, mandatory }: { important: boolean; mandatory: boolean }) {
  return <div className="flex flex-wrap items-center gap-1.5">
    {important && <WorkBadge tone="warning" title="Important"><Flag aria-hidden="true" />Important</WorkBadge>}
    {mandatory && <WorkBadge tone="info" title="Mandatory at session start"><ShieldCheck aria-hidden="true" />Mandatory</WorkBadge>}
  </div>;
}

interface MemoryFormProps {
  initial?: NativeMemory;
  saving: boolean;
  notice?: React.ReactNode;
  onClose: () => void;
  onSave: (value: MemoryDraft) => void;
}

function MemoryForm({ initial, saving, notice, onClose, onSave }: MemoryFormProps) {
  const [title, setTitle] = useState(initial?.title ?? "");
  const [body, setBody] = useState(initial?.body ?? "");
  const [type, setType] = useState<string>(initial?.attributes.classification ?? "fact");
  const [tags, setTags] = useState<string[]>(initial?.tags ?? []);
  const [important, setImportant] = useState(initial?.attributes.important ?? false);
  const [mandatory, setMandatory] = useState(initial?.attributes.mandatory ?? false);
  const [reason, setReason] = useState("Updated from Memory explorer");
  const valid = title.trim() !== "" && body.trim() !== "" && tags.every(tag => tag.trim() !== '') && (!initial || reason.trim() !== "");

  const submit = (event: React.FormEvent) => {
    event.preventDefault();
    if (!valid) return;
    const value: MemoryDraft = {
      title, body, classification: type as MemoryClassification, important, mandatory,
      tags, reason,
    };
    onSave(value);
  };

  return (
    <ModalPortal onClose={onClose}>
      <div className="work-modal-backdrop">
        <form
          role="dialog"
          aria-modal="true"
          aria-label={initial ? "Edit memory" : "Create memory"}
          onSubmit={submit}
          className="work-dialog"
        >
          <header>
            <div>
              <h2>
                {initial ? "Refine this memory" : "Capture a durable memory"}
              </h2>
              <p>
                Record what future work needs to know, with enough context to
                apply it correctly.
              </p>
            </div>
            <button
              className="work-button"
              type="button"
              aria-label="Close memory form"
              onClick={onClose}
            >
              Close
            </button>
          </header>
          {notice}
          <div className="work-editor-layout">
            <section className="work-form">
              <label className="work-field">
                <span>Title</span>
                <input
                  aria-label="Memory title"
                  autoFocus
                  value={title}
                  onChange={(e) => setTitle(e.target.value)}
                  placeholder="A concise statement of the lesson or decision"
                />
              </label>
              <label className="work-field">
                <span>Content</span>
                <textarea
                  aria-label="Memory content"
                  value={body}
                  onChange={(e) => setBody(e.target.value)}
                  rows={14}
                />
                <small>
                  Markdown supported. Include scope, rationale and the
                  circumstances where this applies.
                </small>
              </label>
            </section>
            <aside className="work-form">
              <label className="work-field">
                <span>Type</span>
                <StyledSelect
                  aria-label="Memory type"
                  value={type}
                  onChange={(e) => setType(e.target.value)}
                >
                  {memoryTypes.map((t) => (
                    <option key={t}>{t}</option>
                  ))}
                </StyledSelect>
              </label>
              <div className="work-field" role="group" aria-label="Memory tags">
                <span>Tags</span>
                {tags.map((tag, index) => <label key={index} className="work-field"><span>Tag {index + 1}</span><textarea aria-label={'Tag ' + (index + 1)} value={tag} onChange={event => setTags(values => values.map((value, item) => item === index ? event.target.value : value))} /><button className="work-button" type="button" aria-label={'Remove tag ' + (index + 1)} disabled={saving} onClick={() => setTags(values => values.filter((_value, item) => item !== index))}>Remove tag</button></label>)}
                <button className="work-button" type="button" disabled={saving} onClick={() => setTags(values => [...values, ''])}>Add tag</button>
              </div>
              {initial && <label className="work-field"><span>Revision reason</span><input aria-label="Memory revision reason" value={reason} onChange={event => setReason(event.target.value)} /></label>}
              <label className="work-check">
                <input
                  aria-label="Important memory"
                  type="checkbox"
                  checked={important}
                  onChange={(e) => setImportant(e.target.checked)}
                />
                <span>
                  <strong>Important</strong>
                  <small>Promoted for retrieval.</small>
                </span>
              </label>
              <label className="work-check">
                <input
                  aria-label="Mandatory memory"
                  type="checkbox"
                  checked={mandatory}
                  onChange={(e) => setMandatory(e.target.checked)}
                />
                <span>
                  <strong>Mandatory</strong>
                  <small>Loaded in full at session start.</small>
                </span>
              </label>
              {initial && (
                <WorkNotice title="A traceable revision">
                  Content and tag changes retain their history.
                </WorkNotice>
              )}
            </aside>
          </div>
          <footer>
            <button className="work-button" type="button" onClick={onClose}>
              Cancel
            </button>
            <button className="work-button primary" disabled={!valid || saving}>
              {saving ? "Saving…" : initial ? "Save revision" : "Create memory"}
            </button>
          </footer>
        </form>
      </div>
    </ModalPortal>
  );
}

function MemoryRemovalForm({ memory, saving, notice, onClose, onRemove }: {
  memory: NativeMemory; saving: boolean; notice: React.ReactNode; onClose: () => void; onRemove: (reason: string) => void
}) {
  const [reason, setReason] = useState('')
  return <ModalPortal onClose={onClose}><div className="work-modal-backdrop">
    <form role="dialog" aria-modal="true" aria-label="Remove memory" className="work-dialog"
      onSubmit={event => { event.preventDefault(); if (!saving && reason.trim()) onRemove(reason) }}>
      <header><div><h2>Remove this memory?</h2><p>Its final revision will remain in history.</p></div>
        <button type="button" className="work-button" disabled={saving} onClick={onClose}>Close</button></header>
      <section className="work-form">
        <p><strong>{memory.title}</strong> · revision {memory.revision}</p>
        <label className="work-field"><span>Removal reason</span><input aria-label="Memory removal reason" autoFocus required
          disabled={saving} value={reason} onChange={event => setReason(event.target.value)} /></label>
        {notice}
      </section>
      <footer><button type="button" className="work-button" disabled={saving} onClick={onClose}>Cancel</button>
        <button type="submit" className="work-button danger" disabled={saving || !reason.trim()}>{saving ? 'Removing…' : 'Remove memory'}</button></footer>
    </form>
  </div></ModalPortal>
}

function MemoryDetail({ trace, scopeLabel, referenceBinding, onBack, onEdit, onRemove }: {
  trace: NativeMemoryHistory; scopeLabel: string; referenceBinding: ReferenceBinding | null; onBack: () => void; onEdit: () => void; onRemove: () => void
}) {
  const [selectedRevision, setSelectedRevision] = useState(String(trace.current.revision))
  const versions = [trace.creation.memory, ...trace.revisions.map(revision => revision.after)]
  const selected = versions.find(memory => String(memory.revision) === selectedRevision) ?? trace.current
  return <article className="work-dossier" aria-label="Memory record">
    <header className="dossier-header"><div><div className="dossier-status">
      <TypeBadge type={selected.attributes.classification} /><WorkBadge>{trace.deleted ? 'Deleted' : selected.revision === trace.current.revision ? 'Current' : 'Historical'}</WorkBadge>
      <MemoryFlags important={selected.attributes.important} mandatory={selected.attributes.mandatory} />
    </div><h2>{selected.title}</h2><small><code>{selected.id}</code> · revision {selected.revision}</small></div>
    <div className="work-actions"><button className="work-button" onClick={onBack}>Memories</button>
      {!trace.deleted && <><button className="work-button" onClick={onEdit}>Edit</button><button className="work-button danger" onClick={onRemove}>Remove</button></>}
    </div></header>
    <div className="memory-revisions"><h3>Revision history</h3><div role="group" aria-label="Memory revisions">
      {versions.map(memory => <button key={String(memory.revision)} className="work-button" aria-pressed={memory.revision === selected.revision} onClick={() => setSelectedRevision(String(memory.revision))}>Revision {memory.revision}</button>)}
    </div></div>
    <div className="dossier-layout"><div className="dossier-content">
      {trace.deleted && <WorkNotice title="Memory deleted">This is the retained history of a deleted memory.</WorkNotice>}
      {selected.revision !== trace.current.revision && <WorkNotice title="Historical revision">You are reading a previous version. Editing updates the current memory.</WorkNotice>}
      <WorkSection title="Recorded guidance"><MarkdownContent content={selected.body} /></WorkSection>
      {selected.tags.length > 0 && <WorkSection title="Classification"><div className="work-actions">{selected.tags.map((tag, index) => <WorkBadge key={index}>{tag}</WorkBadge>)}</div></WorkSection>}
      <WorkSection title="Creation and changes"><details className="work-disclosure"><summary>Creation</summary><pre className="work-code">{stringifyNativeJson(trace.creation, 2)}</pre></details>
        {trace.revisions.map((revision, index) => <details className="work-disclosure" key={index}><summary>Revision {revision.after.revision} · {revision.actor}</summary><pre className="work-code">{stringifyNativeJson(revision, 2)}</pre></details>)}
      </WorkSection>
      <details className="work-disclosure"><summary>Complete history document</summary><pre className="work-code">{stringifyNativeJson(trace, 2)}</pre></details>
    </div><aside className="dossier-context"><h3>Authoritative metadata</h3><FactList items={[
      ['Scope', scopeLabel], ['Updated by', selected.updated_by], ['Created', nativeTimestamp(selected.created_at)], ['Updated', nativeTimestamp(selected.updated_at)], ['Reference generation', selected.reference_generation]
    ]} /><WorkSection title="Recorded references">{selected.references.length ? selected.references.map((reference, index) => <pre className="work-code" key={index}>{stringifyNativeJson(reference, 2)}</pre>) : <p>No references are recorded in this version.</p>}</WorkSection>
      <p>Record links describe the current memory.</p><RecordReferences binding={referenceBinding} />
      <details className="work-disclosure"><summary>Publication</summary><pre className="work-code">{stringifyNativeJson(trace.generation, 2)}</pre></details>
    </aside></div>
  </article>
}

export default function MemoryExplorerPage() {
  const navigate = useNavigate();
  const { scopeId, memoryId } = useParams<{
    scopeId?: string;
    memoryId?: string;
  }>();
  const scope: MemoryScope = scopeId === "user" ? "user" : "project";
  const location = useLocation()
  const observedSelection = useTaskWorkspace(state => state.module === 'memory' && state.phase === 'ready' ? state.selection : null)
  const parseRoute = () => {
    if (scope === 'user') {
      const scopes = new URLSearchParams(location.search).getAll('scope_id')
      return { search: location.search, scope, selection: null, error: scopes.length > 1 ? 'The requested User Memory scope is ambiguous' : '', userScope: scopes[0] }
    }
    try { return { search: location.search, scope, selection: requestedRecordSelection(location.search), error: '', userScope: undefined } }
    catch (cause) { return { search: location.search, scope, selection: null, error: cause instanceof Error ? cause.message : 'The record selection is invalid', userScope: undefined } }
  }
  const [requested, setRequested] = useState(parseRoute)
  if (requested.search !== location.search || requested.scope !== scope) setRequested(parseRoute())
  const selectionError = requested.error
  const selection = scope === 'project' && (!requested.selection || sameProjectVersion(observedSelection, requested.selection)) ? observedSelection : null
  const requestedUserScope = requested.userScope
  const scopeKey = scope === 'project' ? nativeSelectionKey(selection) : stringifyNativeJson(['user', requestedUserScope])
  const [catalog, setCatalog] = useState<{ key: string; value: MemoryEnvelope<NativeMemoryList | NativeMemorySearch> } | null>(null)
  const [trace, setTrace] = useState<{ key: string; id: string; value: MemoryEnvelope<NativeMemoryHistory | null> } | null>(null)
  const [query, setQuery] = useState('')
  const [searchMode, setSearchMode] = useState<MemorySearchMode>('fts')
  const [includeHistory, setIncludeHistory] = useState(false)
  const [type, setType] = useState('all')
  const [tag, setTag] = useState('all')
  const [important, setImportant] = useState('all')
  const [mandatory, setMandatory] = useState('all')
  const [catalogFailure, setCatalogFailure] = useState<{ key: string; message: string } | null>(null)
  const [traceFailure, setTraceFailure] = useState<{ key: string; id: string; message: string } | null>(null)
  const [writePending, setWritePending] = useState<{ key: string; busy: boolean } | null>(null)
  const saving = writePending?.key === scopeKey && writePending.busy
  const [form, setForm] = useState<'create' | 'edit' | null>(null)
  const catalogReader = useRef<AbortController | null>(null)
  const detailReader = useRef<AbortController | null>(null)
  const catalogueKey = stringifyNativeJson([scopeKey, query, searchMode, includeHistory, type, tag, important, mandatory])
  const currentCatalog = catalog?.key === catalogueKey ? catalog.value : null
  const catalogError = catalogFailure?.key === catalogueKey ? catalogFailure.message : ''
  const loading = currentCatalog === null && !catalogError
  const publication = currentCatalog?.native ?? null
  const searchPublication = publication && 'mode' in publication ? publication : null
  const hits = searchPublication?.state === 'published' ? searchPublication.hits : []
  const memories = publication?.state === 'published' ? 'memories' in publication ? publication.memories : publication.hits.map(hit => hit.current) : []
  const tags = [...new Set(memories.flatMap(memory => memory.tags))]
  const currentTrace = trace?.key === scopeKey && trace.id === memoryId ? trace.value : null
  const detailTrace = currentTrace?.native ?? null
  const removalKey = stringifyNativeJson([scopeKey, memoryId, detailTrace?.current.revision, detailTrace?.deleted, currentTrace && 'scope_id' in currentTrace ? currentTrace.scope_id : null])
  const [removal, setRemoval] = useState<{ key: string; memory: NativeMemory; scope: MemoryReadScope } | null>(null)
  if (removal && removal.key !== removalKey) setRemoval(null)
  const currentRemoval = removal?.key === removalKey ? removal : null
  const detailError = traceFailure?.key === scopeKey && traceFailure.id === memoryId ? traceFailure.message : ''
  const referenceBinding = detailTrace && currentTrace ? 'selection' in currentTrace ? projectReferenceBinding(currentTrace.selection, 'memory', detailTrace.current.id) : userMemoryReferenceBinding(currentTrace.scope_id, detailTrace.current.id) : null
  const destinationError = referenceDestinationError(location.search, referenceBinding, !!detailTrace)
  const detailLoading = !!memoryId && currentTrace === null && !detailError
  const href = (id?: string, nextScope: MemoryScope = scope) => {
    if (nextScope === 'user') {
      const observed = currentCatalog && 'scope_id' in currentCatalog ? currentCatalog.scope_id : requestedUserScope
      return id && observed ? userMemoryHref(id, observed) : '/memory/explorer/user'
    }
    if (!selection) return '/memory/explorer/project'
    return id ? projectRecordHref('memory', id, selection) : '/memory/explorer/project?' + new URLSearchParams({ selection: encodeProjectLocator(selection) })
  }
  const loadCatalog = useCallback(async () => {
    catalogReader.current?.abort()
    const controller = new AbortController(); catalogReader.current = controller
    setCatalog(null); setCatalogFailure(null)
    const requestScope: MemoryReadScope | null = selectionError ? null : scope === 'user' ? { kind: 'user', requested_scope_id: requestedUserScope } : selection ? { kind: 'project', selection } : null
    if (!requestScope) return
    try {
      const filter = {
        ...(type === 'all' ? {} : { classification: type as MemoryClassification }), ...(tag === 'all' ? {} : { tag }),
        ...(important === 'all' ? {} : { important: important === 'true' }), ...(mandatory === 'all' ? {} : { mandatory: mandatory === 'true' })
      }
      const value = query === '' ? await nativeMemoryApi.list(requestScope, filter, controller.signal)
        : await nativeMemoryApi.search(requestScope, { text: query, top_k: 20, mode: searchMode, filter, include_deleted_history: includeHistory }, controller.signal)
      if (!controller.signal.aborted) setCatalog({ key: catalogueKey, value })
    } catch (cause) { if (!controller.signal.aborted) { setCatalog(null); setCatalogFailure({ key: catalogueKey, message: cause instanceof Error ? cause.message || cause.name : 'Unable to read memories' }) } }
  }, [catalogueKey, scope, requestedUserScope, selection, selectionError, query, searchMode, includeHistory, type, tag, important, mandatory, setCatalog, setCatalogFailure])
  const loadTrace = useCallback(async (id: string) => {
    detailReader.current?.abort()
    const controller = new AbortController(); detailReader.current = controller
    setTrace(null); setTraceFailure(null)
    const requestScope: MemoryReadScope | null = selectionError ? null : scope === 'user' ? { kind: 'user', requested_scope_id: requestedUserScope } : selection ? { kind: 'project', selection } : null
    if (!requestScope) return
    try {
      const value = await nativeMemoryApi.history(requestScope, id, controller.signal)
      if (!controller.signal.aborted) setTrace({ key: scopeKey, id, value })
    } catch (cause) { if (!controller.signal.aborted) { setTrace(null); setTraceFailure({ key: scopeKey, id, message: cause instanceof Error ? cause.message : 'Unable to read memory history' }) } }
  }, [scopeKey, scope, requestedUserScope, selection, selectionError, setTrace, setTraceFailure])
  useEffect(() => {
    let active = true
    const timer = query === '' ? null : window.setTimeout(() => { if (active) void loadCatalog() }, 250)
    if (timer === null) queueMicrotask(() => { if (active) void loadCatalog() })
    return () => { active = false; if (timer !== null) window.clearTimeout(timer); catalogReader.current?.abort() }
  }, [loadCatalog, query])
  useEffect(() => {
    let active = true
    queueMicrotask(() => { if (active) { setForm(null); if (memoryId) void loadTrace(memoryId); else setTrace(null) } })
    return () => { active = false; detailReader.current?.abort() }
  }, [loadTrace, memoryId])
  const selectMemory = (item: NativeMemory) => { if (item.id === memoryId) { if (!detailTrace) void loadTrace(item.id) } else navigate(href(item.id)) }
  const showList = () => { setTrace(null); navigate(href()) }
  const changeScope = (next: MemoryScope) => { setTrace(null); setCatalog(null); setForm(null); setQuery(''); setType('all'); setTag('all'); setImportant('all'); setMandatory('all'); navigate(href(undefined, next)) }

  const [mutation, setMutation] = useState<{ key: string; nextKey?: string; result?: MemoryMutationResult; error?: string } | null>(null)
  const mutationRequest = useRef<AbortController | null>(null)
  const mutationKey = useRef<{ payload: string; id: string } | null>(null)
  const activeScope = useRef(scopeKey)
  const activeFocus = useRef(memoryId)
  useLayoutEffect(() => { activeScope.current = scopeKey }, [scopeKey])
  useLayoutEffect(() => { activeFocus.current = memoryId }, [memoryId])
  useEffect(() => () => { mutationRequest.current?.abort() }, [scopeKey])
  const writeScope = (): MemoryReadScope => scope === 'project' ? { kind: 'project', selection: selection! } : {
    kind: 'user', requested_scope_id: currentTrace && 'scope_id' in currentTrace ? currentTrace.scope_id : currentCatalog && 'scope_id' in currentCatalog ? currentCatalog.scope_id : requestedUserScope,
  }
  const operationKey = (operation: string, input: unknown) => {
    const payload = stringifyNativeJson([scopeKey, operation, input])
    if (mutationKey.current?.payload !== payload) mutationKey.current = { payload, id: crypto.randomUUID() }
    return mutationKey.current.id
  }
  const acceptMutation = async (result: MemoryMutationResult, removing = false) => {
    if (activeScope.current !== scopeKey) return
    const receipt = result.delivery === 'receipt' ? result.envelope.native : result.receipt
    if (receipt.state === 'completed' || receipt.state === 'confirmed') mutationKey.current = null
    if (removing && activeFocus.current !== memoryId) return
    const observedUser = result.delivery === 'receipt' && 'scope_id' in result.envelope ? result.envelope.scope_id : currentCatalog && 'scope_id' in currentCatalog ? currentCatalog.scope_id : undefined
    setMutation({ key: scopeKey, nextKey: scope === 'user' && observedUser ? stringifyNativeJson(['user', observedUser]) : undefined, result })
    if (activeFocus.current !== memoryId || receipt.state === 'uncertain' || receipt.state === 'not_applied') return
    setForm(null)
    setRemoval(null)
    if (currentCatalog || currentTrace) {
      if (removing) { setTrace(null); navigate(href(), { replace: true }) }
      else if (receipt.id) {
        const target = scope === 'user' && result.delivery === 'receipt' && 'scope_id' in result.envelope ? userMemoryHref(receipt.id, result.envelope.scope_id) : href(receipt.id)
        navigate(target, { replace: true })
      }
      await Promise.all([loadCatalog(), !removing && receipt.id === memoryId ? loadTrace(receipt.id) : Promise.resolve()])
    }
  }
  const save = async (value: MemoryDraft) => {
    if (saving || selectionError || (scope === 'project' && !selection)) return
    const editing = form === 'edit'
    if (editing && (!detailTrace?.current || detailTrace.deleted)) return
    if (scope === 'user' && requestedUserScope !== undefined && !currentCatalog && !currentTrace) {
      setMutation({ key: scopeKey, error: 'Read this User Memory namespace before writing to its addressed records.' }); return
    }
    setWritePending({ key: scopeKey, busy: true }); setMutation(null)
    const controller = new AbortController(); mutationRequest.current = controller
    try {
      const input = editing ? { id: detailTrace!.current.id, expected_revision: detailTrace!.current.revision, reason: value.reason, title: value.title, body: value.body, classification: value.classification, important: value.important, mandatory: value.mandatory, tags: value.tags }
        : { title: value.title, body: value.body, attributes: { classification: value.classification, important: value.important, mandatory: value.mandatory }, tags: value.tags, references: [] }
      const idempotency_key = operationKey(editing ? 'update' : 'create', input)
      const result = editing
        ? await nativeMemoryMutationApi.update(writeScope(), { ...input, id: detailTrace!.current.id, expected_revision: detailTrace!.current.revision, reason: value.reason, idempotency_key }, controller.signal)
        : await nativeMemoryMutationApi.create(writeScope(), { title: value.title, body: value.body, attributes: { classification: value.classification, important: value.important, mandatory: value.mandatory }, tags: value.tags, references: [], idempotency_key }, controller.signal)
      await acceptMutation(result)
    } catch (cause) {
      if (activeScope.current === scopeKey) setMutation({ key: scopeKey, error: (cause instanceof Error ? cause.message : 'Unable to save memory') + '. Inspect the record before submitting again; a failed delivery can follow an applied write.' })
    } finally { if (activeScope.current === scopeKey) setWritePending({ key: scopeKey, busy: false }) }
  }
  const remove = async (reason: string) => {
    if (saving || !currentRemoval || !reason.trim() || !detailTrace?.current || detailTrace.deleted) return
    setWritePending({ key: scopeKey, busy: true }); setMutation(null)
    const controller = new AbortController(); mutationRequest.current = controller
    const input = { id: currentRemoval.memory.id, expected_revision: currentRemoval.memory.revision, reason }
    try {
      const result = await nativeMemoryMutationApi.remove(currentRemoval.scope, { ...input, idempotency_key: operationKey('delete', input) }, controller.signal)
      await acceptMutation(result, true)
    } catch (cause) {
      if (activeScope.current === scopeKey && activeFocus.current === memoryId) setMutation({ key: scopeKey, error: (cause instanceof Error ? cause.message : 'Unable to remove memory') + '. Inspect the record before submitting again.' })
    } finally { if (activeScope.current === scopeKey) setWritePending({ key: scopeKey, busy: false }) }
  }

  usePageRefresh(() => refreshAll([loadCatalog(), memoryId ? loadTrace(memoryId) : Promise.resolve()]));
  if (selectionError || (scope === 'project' && !selection)) {
    return <WorkPage><WorkHeader title={scope === 'user' ? 'User memory' : 'Project memory'} description="Keep decisions, conventions and lessons available beyond a single conversation." />
      <WorkTabs value={scope} onChange={id => changeScope(id as MemoryScope)} items={[["project", "Project"], ["user", "User"]]} label="Memory scope" />
      {selectionError ? <WorkNotice title="Unable to select memory scope" tone="error">{selectionError}</WorkNotice> : <WorkEmpty title="Select a working context">Choose the project and version for this record.</WorkEmpty>}
    </WorkPage>
  }
  return (
    <WorkPage>
      <WorkHeader
        title={scope === "user" ? "Personal memory" : "Project memory"}
        description="Keep decisions, conventions and lessons available beyond a single conversation."
        actions={
          <>

            <button
              className="work-button primary"
              title="Create memory"
              onClick={() => { setMutation(null); setForm("create") }}
            >
              Create memory
            </button>
          </>
        }
      />
      {mutation && !currentRemoval && !form && (mutation.key === scopeKey || mutation.nextKey === scopeKey) && <MemoryMutationNotice mutation={mutation} />}
      <WorkTabs
        value={scope}
        onChange={(id) => changeScope(id as MemoryScope)}
        items={[
          ["project", "Project"],
          ["user", "User"],
        ]}
        label="Memory scope"
      />
      <div className="work-toolbar">
        <WorkSearch label="Search memories" value={query} onChange={setQuery} />
        <StyledSelect aria-label="Memory search method" value={searchMode} onChange={event => setSearchMode(event.target.value as MemorySearchMode)}><option value="fts">Keyword</option><option value="semantic">Semantic</option><option value="hybrid">Hybrid</option></StyledSelect>
        {query !== '' && <label className="work-check"><input type="checkbox" aria-label="Search retained revisions" checked={includeHistory} onChange={event => setIncludeHistory(event.target.checked)} />Search retained revisions</label>}
        <StyledSelect
          aria-label="Filter memory type"
          value={type}
          onChange={(e) => setType(e.target.value)}
        >
          <option value="all">All types</option>
          {memoryTypes.map((t) => (
            <option key={t}>{t}</option>
          ))}
        </StyledSelect>
        <StyledSelect
          aria-label="Filter memory tag"
          value={tag}
          onChange={(e) => setTag(e.target.value)}
        >
          <option value="all">All tags</option>
          {tags.map((t) => (
            <option key={t}>{t}</option>
          ))}
        </StyledSelect>
        <StyledSelect
          aria-label="Filter importance"
          value={important}
          onChange={(e) => setImportant(e.target.value)}
        >
          <option value="all">Any importance</option>
          <option value="true">Important</option>
          <option value="false">Not important</option>
        </StyledSelect>
        <StyledSelect
          aria-label="Filter mandatory"
          value={mandatory}
          onChange={(e) => setMandatory(e.target.value)}
        >
          <option value="all">Any startup policy</option>
          <option value="true">Mandatory</option>
          <option value="false">Not mandatory</option>
        </StyledSelect>
      </div>
      {catalogError && <WorkNotice title="Unable to read memories" tone="error">{catalogError}<button className="work-button" onClick={() => void loadCatalog()}>Retry</button></WorkNotice>}
      <section aria-label="Memory catalogue">
        <div className="work-catalogue-summary">
          <span>{publication?.state === 'published' ? `${memories.length} ${searchPublication ? 'search results' : 'current memories'}` : publication?.state === 'no_corpus' ? 'No published Memory corpus' : loading ? 'Loading memories…' : 'Memory data unavailable'}</span>
          <span>History stays attached to each record.</span>
        </div>
        {loading ? (
          <LoadingSpinner label="Loading memories…" />
        ) : memories.length ? (
          <div className="work-table-wrap work-catalogue">
            <table className="work-table">
              <thead>
                <tr>
                  <th>Guidance</th>
                  <th>Type</th>
                  <th>Policy</th>
                  <th>Updated</th>
                </tr>
              </thead>
              <tbody>
                {memories.map((item, index) => (
                  <tr
                    key={searchPublication ? item.id + ":" + index : item.id}
                    className={memoryId === item.id ? "selected" : ""}
                  >
                    <td>
                      <button
                        className="record-title"
                        onClick={() => selectMemory(item)}
                      >
                        {item.title}
                      </button>
                      <div className="markdown-preview"><MemoryMarkdown content={item.body} /></div>
                      {hits[index] && <details className="work-disclosure"><summary>Search match evidence</summary><pre className="work-code">{stringifyNativeJson(hits[index], 2)}</pre></details>}
                    </td>
                    <td>
                      <TypeBadge type={item.attributes.classification} />
                    </td>
                    <td>
                      <MemoryFlags
                        important={item.attributes.important}
                        mandatory={item.attributes.mandatory}
                      />
                    </td>
                    <td>
                      {nativeTimestamp(item.updated_at)}
                      <small>Revision {item.revision}</small>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        ) : searchPublication?.state === 'unavailable' ? <WorkNotice title="Memory search unavailable">{searchPublication.reason === 'model_unavailable' ? 'The search model is unavailable. Use keyword search or retry after configuring a model.' : searchPublication.reason === 'missing_coverage' ? 'This publication has no compatible embedding coverage. Use keyword search or refresh its embeddings.' : 'The embedding identity does not match this publication. Use keyword search or refresh its embeddings.'}<button className="work-button" onClick={() => void loadCatalog()}>Retry search</button></WorkNotice>
        : searchPublication?.state === 'published_empty' ? <WorkEmpty title="The Memory search publication is empty" />
        : publication?.state === 'no_corpus' ? <WorkEmpty title="No Memory corpus has been published">This context has no published memories.</WorkEmpty> : publication?.state === 'published' ? (
          <WorkEmpty title="No memories match this view">
            Adjust filters or capture a new durable memory.
          </WorkEmpty>
        ) : null}
      </section>
      {searchPublication && <details className="work-disclosure"><summary>Complete search result</summary><pre className="work-code">{stringifyNativeJson(searchPublication, 2)}</pre></details>}
      {detailError && <WorkNotice title="Unable to read memory" tone="error">{detailError}<button className="work-button" onClick={() => memoryId && void loadTrace(memoryId)}>Retry memory</button></WorkNotice>}
      {destinationError ? <WorkNotice title="Reference destination unavailable" tone="error">{destinationError}</WorkNotice> : detailLoading ? <LoadingSpinner label="Loading memory history…" /> : memoryId && currentTrace && !detailTrace ? <WorkEmpty title="Memory not found">The selected memory has no retained history.</WorkEmpty> : detailTrace ? (
        <MemoryDetail
          key={scopeKey + ":" + detailTrace.current.id + ":" + String(detailTrace.current.revision)}
          trace={detailTrace}
          referenceBinding={referenceBinding}
          scopeLabel={scope === 'user' && currentTrace && 'scope_id' in currentTrace ? currentTrace.scope_id : selection?.project_id ?? ''}
          onBack={showList}
          onEdit={() => { setMutation(null); setForm("edit") }}
          onRemove={() => { setMutation(null); setRemoval({ key: removalKey, memory: detailTrace.current, scope: writeScope() }) }}
        />
      ) : !loading && memories.length > 0 ? (
        <WorkEmpty title="Select a memory">
          Choose a current record to inspect its guidance, scope and complete
          revision trace.
        </WorkEmpty>
      ) : null}
      {currentRemoval && <MemoryRemovalForm key={removalKey} memory={currentRemoval.memory} saving={saving}
        notice={mutation?.key === scopeKey ? <MemoryMutationNotice mutation={mutation} /> : null}
        onClose={() => { if (!saving) setRemoval(null) }} onRemove={reason => void remove(reason)} />}
      {form && (
        <MemoryForm
          notice={mutation && (mutation.key === scopeKey || mutation.nextKey === scopeKey) ? <MemoryMutationNotice mutation={mutation} /> : null}
          key={scopeKey + ":" + form + "-" + (detailTrace ? String(detailTrace.current.revision) : "new")}
          initial={form === "edit" ? detailTrace?.current : undefined}
          saving={saving}
          onClose={() => setForm(null)}
          onSave={(value) => void save(value)}
        />
      )}
    </WorkPage>
  );
}

function MemoryMutationNotice({ mutation }: { mutation: { result?: MemoryMutationResult; error?: string } }) {
  if (mutation.error) return <WorkNotice title="Unable to complete Memory write" tone="error">{mutation.error}</WorkNotice>
  if (!mutation.result) return null
  const result = mutation.result
  const receipt = result.delivery === 'receipt' ? result.envelope.native : result.receipt
  const indexVersion = result.delivery === 'receipt' ? result.envelope.native.index_publication?.sdk_version : result.receipt.index_sdk_version
  return <WorkNotice title={receipt.state === 'not_applied' ? 'Memory write not applied' : receipt.state === 'uncertain' ? 'Memory write outcome uncertain' : receipt.state === 'confirmed' ? 'Memory write effect confirmed' : 'Memory write completed'} tone={receipt.state === 'uncertain' || receipt.state === 'not_applied' ? 'neutral' : 'success'}>
    <p>{receipt.state === 'not_applied' ? 'The search index was published, but the Memory change was not applied. Review the receipt before trying again.' : receipt.state === 'uncertain' ? 'Inspect the original record and recovery receipt before making another change.' : 'The write was applied. Delivery or completion errors do not mean it should be repeated.'}</p>
    <FactList items={[["Memory ID", receipt.id], ["Revision", receipt.revision], ["SDK version", receipt.sdk_version], ...(indexVersion === undefined ? [] : [["Index SDK version", indexVersion] as [string, React.ReactNode]]), ["Completion error", receipt.completion_error], ["Delivery error", receipt.delivery_error], ["Response error", result.delivery === 'headers' ? result.receipt.response_error : null]]} />
    <details className="work-disclosure"><summary>Write receipt</summary><pre className="work-code">{stringifyNativeJson(result, 2)}</pre></details>
  </WorkNotice>
}
