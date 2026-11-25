import { useCallback, useEffect, useRef, useState } from 'react'
import { useLocation, useNavigate } from 'react-router-dom'
import { nativeKnowledgeApi, type NativeKnowledgeHeads, type NativeKnowledgePage, type KnowledgeEnvelope } from '@/api/nativeKnowledge'
import { importedKnowledgeHref, importedKnowledgeRoute, importedSourceKey, nativeSelectionKey, projectRecordHref, requestedImportedKnowledgeSource, requestedRecordSelection, sameProjectVersion } from '@/api/nativeRecordNavigation'
import type { TaskContext } from '@/api/task'
import { knowledgeReferenceBinding } from '@/api/nativeRecordReferences'
import { referenceDestinationError } from '@/api/nativeReferenceNavigation'
import { useTaskWorkspace } from '@/components/task/taskWorkspaceState'
import { usePageRefresh } from '@/components/layout/WorkspaceRefresh'
import { RecordReferences } from '@/components/shared/RecordReferences'
import { LoadingSpinner } from '@/components/shared/LoadingSpinner'
import { WorkEmpty, WorkHeader, WorkNotice, WorkPage, WorkSection } from '@/components/shared/EngineeringUI'
import { NativeKnowledgeDocument } from './NativeKnowledgeDocument'
import { ImportedKnowledge } from './ImportedKnowledge'
import { NativeKnowledgeSearch } from './NativeKnowledgeSearch'
import { NativeKnowledgeAsk } from './NativeKnowledgeAsk'
import { KnowledgePageCatalogue } from './KnowledgePageCatalogue'
import { useProviderMode } from '@/store/providerMode'

export default function WikiExplorerPage() {
  const location = useLocation();
  const provider = useProviderMode();
  const observedSelection = useTaskWorkspace(state => state.module === 'knowledge' && state.phase === 'ready' ? state.selection : null);
  const requestedPages = new URLSearchParams(location.search).getAll('page');
  if (requestedPages.length > 1) return <WorkNotice title="Invalid Knowledge page" tone="error">The requested page is ambiguous.</WorkNotice>;
  if (importedKnowledgeRoute(location.search)) {
    let source: ReturnType<typeof requestedImportedKnowledgeSource>;
    try {
      source = requestedImportedKnowledgeSource(location.search);
    } catch (cause) { return <WorkNotice title="Invalid imported Knowledge selection" tone="error">{cause instanceof Error ? cause.message : String(cause)}</WorkNotice>; }
    return <ImportedKnowledge key={importedSourceKey(source)} source={source} slug={requestedPages[0] ?? null} />;
  }
  let selection: TaskContext | null;
  try {
    const requested = requestedRecordSelection(location.search);
    selection = !requested || sameProjectVersion(observedSelection, requested) ? observedSelection : null;
  }
  catch (cause) { return <WorkNotice title="Invalid Knowledge selection" tone="error">{cause instanceof Error ? cause.message : String(cause)}</WorkNotice>; }
  if (selection) return <NativeProjectKnowledge key={nativeSelectionKey(selection)} selection={selection} slug={requestedPages[0] ?? null} />;
  return <WorkPage><WorkHeader title="Knowledge library" description="Read maintained project guidance." actions={provider !== 'local' && <a className="work-button" href={importedKnowledgeHref(null)}>Imported Knowledge</a>} /><WorkEmpty title="Select a working context">Choose a project and version with Knowledge access.</WorkEmpty></WorkPage>;
}

function NativeProjectKnowledge({ selection: capturedSelection, slug }: { selection: TaskContext; slug: string | null }) {
  const provider = useProviderMode();
  const [selection] = useState(capturedSelection);
  const navigate = useNavigate();
  const location = useLocation();
  const [heads, setHeads] = useState<NativeKnowledgeHeads | null>(null);
  const [headsError, setHeadsError] = useState('');
  const [page, setPage] = useState<{ slug: string; value: KnowledgeEnvelope<NativeKnowledgePage> } | null>(null);
  const [pageError, setPageError] = useState<{ slug: string; message: string } | null>(null);
  const headsController = useRef<AbortController | null>(null);
  const pageController = useRef<AbortController | null>(null);
  const loadHeads = useCallback(async () => {
    headsController.current?.abort();
    const controller = new AbortController(); headsController.current = controller;
    setHeads(null); setHeadsError('');
    try {
      const response = await nativeKnowledgeApi.heads(selection, controller.signal);
      if (!controller.signal.aborted) setHeads(response.native);
    } catch (cause) {
      if (!controller.signal.aborted) setHeadsError(cause instanceof Error ? cause.message : String(cause));
    }
  }, [selection]);
  const loadPage = useCallback(async () => {
    pageController.current?.abort();
    if (slug === null) return;
    const controller = new AbortController(); pageController.current = controller;
    setPage(null); setPageError(null);
    try {
      const response = await nativeKnowledgeApi.page(selection, slug, controller.signal);
      if (!controller.signal.aborted) setPage({ slug, value: response });
    } catch (cause) {
      if (!controller.signal.aborted) setPageError({ slug, message: cause instanceof Error ? cause.message : String(cause) });
    }
  }, [selection, slug]);
  useEffect(() => {
    let cancelled = false;
    queueMicrotask(() => { if (!cancelled) void loadHeads(); });
    return () => { cancelled = true; headsController.current?.abort(); };
  }, [loadHeads]);
  useEffect(() => {
    let cancelled = false;
    queueMicrotask(() => { if (!cancelled) void loadPage(); });
    return () => { cancelled = true; pageController.current?.abort(); };
  }, [loadPage]);
  usePageRefresh(async () => { await Promise.all([loadHeads(), loadPage()]); });
  const currentEnvelope = page?.slug === slug ? page.value : null;
  const currentPage = currentEnvelope?.native ?? null;
  const referenceBinding = currentEnvelope ? knowledgeReferenceBinding(currentEnvelope.selection, currentEnvelope.source, currentEnvelope.focused_source) : null;
  const destinationError = referenceDestinationError(location.search, referenceBinding, currentPage?.state === 'published' && currentPage.page.state === 'found');
  const currentError = pageError?.slug === slug ? pageError.message : '';
  const readerOrigin = location.state?.knowledgeReaderOrigin === 'search' ? 'search' : 'catalogue'
  const openPage = (target: string, origin = readerOrigin) => navigate(projectRecordHref('knowledge', target, selection), {state:{knowledgeReaderOrigin:origin}});
  const libraryHref = (origin = readerOrigin, focusIndex = false) => {
    const url = new URL(projectRecordHref('knowledge', '', selection), window.location.origin);
    url.searchParams.delete('page');
    navigate(url.pathname + url.search, {state:{knowledgeReaderOrigin:origin,knowledgeIndexFocus:focusIndex}});
  };
  const reader = <>
{destinationError ? <WorkNotice title="Reference destination unavailable" tone="error">{destinationError}</WorkNotice> : currentError ? <WorkNotice title="Unable to read document" tone="error">{currentError}<button className="work-button" onClick={() => void loadPage()}>Retry document</button></WorkNotice>
        : !currentPage ? <LoadingSpinner label="Loading Knowledge document…" />
        : currentPage.state === 'no_corpus' ? <WorkEmpty title="No Knowledge corpus has been published" />
        : currentPage.page.state === 'published_empty' ? <WorkEmpty title="The Knowledge publication is empty" />
        : currentPage.page.state === 'page_absent' ? <WorkEmpty title="Knowledge page not found">{slug}</WorkEmpty>
        : <><NativeKnowledgeDocument key={slug} heads={heads?.state === 'published' ? heads.heads : undefined} publication={currentPage} openPage={openPage} pageHref={target => projectRecordHref('knowledge', target, selection)} readerOrigin={readerOrigin} fragment={location.hash} browseIndex={() => libraryHref('catalogue', true)} /><RecordReferences binding={referenceBinding} /></>}
  </>
  return <WorkPage className="knowledge-library">
    <WorkHeader title="Knowledge library" description="Read maintained guidance and its authoritative publication metadata." actions={<>{provider !== 'local' && <button className="work-button" onClick={() => navigate(importedKnowledgeHref(null))}>Imported Knowledge</button>}{slug !== null && <button className="work-button" onClick={() => libraryHref()}>Back to library</button>}</>} />
    <NativeKnowledgeSearch scope={{ kind: 'project', selection }} openPage={target => openPage(target, 'search')} reader={readerOrigin === 'search' ? {slug,content:reader,label:'Knowledge document',close:() => libraryHref('search')} : undefined} />
    <WorkSection title="Read the guide">
      {headsError ? <WorkNotice title="Unable to read Knowledge" tone="error">{headsError}<button className="work-button" onClick={() => void loadHeads()}>Retry library</button></WorkNotice>
        : !heads ? <LoadingSpinner label="Loading Knowledge pages…" />
        : heads.state === 'no_corpus' ? <WorkEmpty title="No Knowledge corpus has been published" />
        : <>
          {heads.heads.length === 0 && <WorkEmpty title="The Knowledge publication is empty" />}</>}
      <KnowledgePageCatalogue heads={heads?.state === 'published' ? heads.heads : null} slug={readerOrigin === 'catalogue' ? slug : null} restoreFocus={slug === null} focusIndex={slug === null && location.state?.knowledgeIndexFocus === true}
        onSelect={target => target === null ? libraryHref('catalogue') : openPage(target, 'catalogue')}
        label="Published Knowledge pages" readerLabel="Knowledge document">
        {reader}
      </KnowledgePageCatalogue>
    </WorkSection>
    <NativeKnowledgeAsk key={nativeSelectionKey(selection)} selection={selection} heads={heads?.state === 'published' ? heads.heads : null} />
  </WorkPage>;
}
