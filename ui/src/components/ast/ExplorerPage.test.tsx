import { nativeAstApi } from '@/api/nativeAst';
import { TaskWorkspaceSelectors } from "@/components/task/TaskWorkspace";
import { nativeRecordContexts, NativeRecordReadError } from '@/api/nativeRecordRead';
import { useTaskWorkspace, stopTaskWorkspace } from '@/components/task/taskWorkspaceState';
import { nativeSchema, nativeSample, nativeQuery, nativeQueryWithRelation, neighborhoodPage, selection, installed, searchReply, resolvedQuery } from './nativeAst.test-fixtures';
import "@testing-library/jest-dom/vitest";
import "@/test/contextControls";
import { WorkspaceRefreshProvider } from "@/components/layout/WorkspaceRefreshProvider";
import { WorkspaceSelectors } from "@/components/layout/WorkspaceSelectors";
import { afterEach, beforeEach, it, expect, vi } from "vitest";
import { render, screen, waitFor, cleanup, act } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MemoryRouter, Routes, Route, useNavigate } from "react-router-dom";
import { importedAstHref } from '@/api/nativeRecordNavigation';
import { useAppStore } from "@/store/appStore";
import ExplorerPage from "./ExplorerPage";
import * as nativeJson from '@/api/nativeJson';
vi.mock("@/api/nativeRecordRead", async original => ({ ...await original<typeof import("@/api/nativeRecordRead")>(), nativeRecordContexts: vi.fn() }));
vi.mock("@/api/nativeAst", async original => ({ ...await original<typeof import("@/api/nativeAst")>(), nativeAstApi: { schema: vi.fn(), sample: vi.fn(), query: vi.fn(), neighborhood: vi.fn(), source: vi.fn(), search: vi.fn(), imports: vi.fn(), importedSchema: vi.fn(), importedSample: vi.fn(), importedQuery: vi.fn(), importedNeighborhood: vi.fn(), importedSource: vi.fn(), importedSearch: vi.fn() } }));
vi.mock("@/api/ast", () => ({
  astApi: {
    generateCypher: vi.fn(),
  },
}));

vi.mock("./CodePanel", () => ({
  CodePanel: ({ content }: { content: string }) => <pre>{content}</pre>,
}));

beforeEach(() => {
 vi.resetAllMocks(); stopTaskWorkspace();
 vi.mocked(nativeRecordContexts).mockResolvedValue({ contexts: [selection], current: selection });
 for (const [name,value] of Object.entries({schema:nativeSchema,sample:nativeSample,query:nativeQuery,neighborhood:neighborhoodPage(),source:{origin:'indexed',selection:{source:'fn observed() {}\n',total_lines:1,start_line:1,end_line:1}},imports:{imports:[installed]},importedSchema:nativeSchema,importedSample:nativeSample,importedQuery:resolvedQuery,importedNeighborhood:neighborhoodPage(),importedSource:{origin:'indexed',selection:{source:'fn imported_observed() {}\n',total_lines:1,start_line:1,end_line:1}},importedSearch:searchReply})) vi.mocked(nativeAstApi[name as keyof typeof nativeAstApi]).mockResolvedValue(value as never);
 useAppStore.setState({activeProjectKey:'workspace:demo:/project',activeProjectOrigin:'workspace',activeProjectId:'demo',activeProjectDir:'/project',projectsError:'',loadProjects:vi.fn(async()=>{})});
});
afterEach(() => { cleanup(); vi.restoreAllMocks(); });
function setup() {
 render(<MemoryRouter initialEntries={[importedAstHref(installed)]}><WorkspaceRefreshProvider><WorkspaceSelectors/><Routes><Route path='/ast/explorer/:contextId?' element={<ExplorerPage/>}/></Routes></WorkspaceRefreshProvider></MemoryRouter>);
 return userEvent.setup();
}
async function ready(){await waitFor(()=>expect(nativeAstApi.importedSchema).toHaveBeenCalled());}
async function searchIndex(user:ReturnType<typeof userEvent.setup>,text='Anchor'){await ready();await user.type(screen.getByLabelText('Search indexed code'),text);await user.click(screen.getByRole('button',{name:'Search index'}));}
async function runQuery(user:ReturnType<typeof userEvent.setup>,text='RETURN n'){await ready();await user.click(screen.getByRole('tab',{name:'Query lab'}));await user.type(screen.getByLabelText('Cypher query'),text);await user.click(screen.getByRole('button',{name:'Run query'}));}
function scalar(text:string):import('@/api/nativeAst').AstQuery{return {columns:['name'],rows:[[{type:'STRING',value:text}]]};}
it('reads an exact installation and changes only to another observed version',async()=>{
 const other={...installed,version:'other/version',context:'library@other/version'};vi.mocked(nativeAstApi.imports).mockResolvedValue({imports:[installed,other]});
 const user=setup();await ready();expect(nativeAstApi.importedSchema).toHaveBeenCalledWith(installed,expect.any(AbortSignal));
 await user.click(screen.getByRole('combobox',{name:'AST artifact and version'}));await user.click(screen.getByRole('option',{name:other.id+' · '+other.version}));
 await waitFor(()=>expect(nativeAstApi.importedSchema).toHaveBeenLastCalledWith(other,expect.any(AbortSignal)));
});
it('searches without sample, resolves a graph anchor and reads source and entity',async()=>{
 const user=setup();await searchIndex(user);await user.click(await screen.findByRole('button',{name:/Anchor src\/file.rs/}));
 await waitFor(()=>expect(nativeAstApi.importedNeighborhood).toHaveBeenCalledTimes(2));
 expect(nativeAstApi.importedQuery).toHaveBeenCalledWith(installed,'MATCH (n:Function) WHERE n.entity_key = $observed_key RETURN n LIMIT 2',{observed_key:'observed-key-A'},expect.any(AbortSignal));
 await user.click(screen.getByRole('button',{name:'Inspect source & impact'}));await screen.findByText('fn imported_observed() {}');await user.click(screen.getByRole('button',{name:'Read entity source'}));
 expect(nativeAstApi.importedSource).toHaveBeenLastCalledWith(installed,{path:'src/file.rs',entity:'Anchor',entity_type:'Function'},expect.any(AbortSignal));
 await user.click(screen.getByRole('tab',{name:'Incoming'}));await waitFor(()=>expect(nativeAstApi.importedQuery).toHaveBeenLastCalledWith(installed,expect.stringContaining('WHERE anchor.entity_key = $anchor_key'),{anchor_key:'observed-key-A'},expect.any(AbortSignal)));
 expect(nativeAstApi.importedSample).not.toHaveBeenCalled();
});
it('retains the same file and exposes the causal entity error locally until an explicit retry succeeds',async()=>{
 const user=setup();await searchIndex(user);await user.click(await screen.findByRole('button',{name:/Anchor src\/file.rs/}));
 await user.click(screen.getByRole('button',{name:'Inspect source & impact'}));await screen.findByText('fn imported_observed() {}');
 vi.mocked(nativeAstApi.importedSource).mockRejectedValueOnce(new NativeRecordReadError(400,'FieldUnavailable'));
 await user.click(screen.getByRole('button',{name:'Read entity source'}));
 expect(await screen.findByRole('alert')).toHaveTextContent('FieldUnavailable');
 expect(screen.getByText('fn imported_observed() {}')).toBeTruthy();
 expect(screen.getByText('The previously loaded source remains visible.')).toBeTruthy();
 expect(screen.queryByText('No source open')).toBeNull();
 expect(nativeAstApi.importedSource).toHaveBeenLastCalledWith(installed,{path:'src/file.rs',entity:'Anchor',entity_type:'Function'},expect.any(AbortSignal));
 vi.mocked(nativeAstApi.importedSource).mockResolvedValueOnce({origin:'indexed',selection:{source:'fn selected_entity() {}\n',total_lines:528,start_line:188,end_line:190}});
 await user.click(screen.getByRole('button',{name:'Read entity source'}));
 expect(await screen.findByText('fn selected_entity() {}')).toBeTruthy();
 expect(screen.queryByText('fn imported_observed() {}')).toBeNull();
 expect(screen.queryByRole('alert')).toBeNull();
});
it('keeps installed readers independent of PROJECT changes',async()=>{
 const user=setup();await searchIndex(user);await screen.findByRole('button',{name:/Anchor src\/file.rs/});act(()=>useAppStore.setState({activeProjectKey:'workspace:next:/next',activeProjectDir:'/next'}));
 expect(screen.getByRole('button',{name:/Anchor src\/file.rs/})).toBeTruthy();expect(nativeAstApi.importedSchema).toHaveBeenCalledTimes(1);
});
it('refuses renderer identity when no primary key was observed',async()=>{
 vi.mocked(nativeAstApi.importedSearch).mockResolvedValue({...searchReply,results:[{...searchReply.results[0],entity_key:''}]});
 const user=setup();await searchIndex(user);await user.click(await screen.findByRole('button',{name:/Anchor src\/file.rs/}));await screen.findByText(/The primary key is unavailable/);
 expect(nativeAstApi.importedQuery).not.toHaveBeenCalled();expect(nativeAstApi.importedNeighborhood).not.toHaveBeenCalled();
});
it('refuses ambiguous published graph resolution',async()=>{
 const second=structuredClone(resolvedQuery.rows[0]); ((second[0].value as {id:{value:{row:string}}}).id.value).row='9007199254740994'; vi.mocked(nativeAstApi.importedQuery).mockResolvedValue({...resolvedQuery,rows:[resolvedQuery.rows[0],second]});
 const user=setup();await searchIndex(user);await user.click(await screen.findByRole('button',{name:/Anchor src\/file.rs/}));await screen.findByText(/unavailable or ambiguous/);expect(nativeAstApi.importedNeighborhood).not.toHaveBeenCalled();
});
it('loads a bounded sample once on first map entry with accessible controls',async()=>{
 const user=setup();await ready();await user.click(screen.getByRole('tab',{name:'Relationship map'}));await screen.findByRole('button',{name:/Anchor src\/file.rs/});
 expect(nativeAstApi.importedSample).toHaveBeenCalledTimes(1);expect(screen.getByLabelText('Organize by')).toBeTruthy();
 await user.click(screen.getByRole('tab',{name:'Query lab'}));await user.click(screen.getByRole('tab',{name:'Relationship map'}));expect(nativeAstApi.importedSample).toHaveBeenCalledTimes(1);
});
it('renders native mixed query cells without losing graph data',async()=>{
 vi.mocked(nativeAstApi.importedQuery).mockResolvedValue(nativeQuery);const user=setup();await runQuery(user);await screen.findByText('Query rows · 1');
 expect(screen.getByRole('cell',{name:'{"type":"INT64","value":"9007199254740993"}'})).toBeTruthy();expect(nativeAstApi.importedSample).not.toHaveBeenCalled();
});
it('opens the map during a query, exposes its error and preserves the draft',async()=>{
 let fail!:(e:Error)=>void;vi.mocked(nativeAstApi.importedQuery).mockImplementationOnce(()=>new Promise((_,reject)=>{fail=reject}));const user=setup();await runQuery(user,'MATCH (n) RETURN n');
 expect(screen.getByRole('tab',{name:'Relationship map'})).toHaveAttribute('aria-selected','true');await act(async()=>fail(new Error('Typed relationship required')));
 await screen.findByText('Typed relationship required');await user.click(screen.getByRole('tab',{name:'Query lab'}));expect(screen.getByLabelText('Cypher query')).toHaveValue('MATCH (n) RETURN n');
});
it('keeps scalar and empty query results without sample fallback',async()=>{
 vi.mocked(nativeAstApi.importedQuery).mockResolvedValueOnce(scalar('forty two')).mockResolvedValueOnce({columns:['name'],rows:[]});const user=setup();await runQuery(user);
 await screen.findByRole('cell',{name:'{"type":"STRING","value":"forty two"}'});await user.click(screen.getByRole('tab',{name:'Query lab'}));await user.click(screen.getByRole('button',{name:'Run query'}));await screen.findByText('Query rows · 0');expect(nativeAstApi.importedSample).not.toHaveBeenCalled();
});
it('distinguishes healthy empty search, causal 200 status and failure',async()=>{
 vi.mocked(nativeAstApi.importedSearch).mockResolvedValueOnce({...searchReply,results:[]}).mockResolvedValueOnce({status:'index_absent',results:[]}).mockRejectedValueOnce(new Error('Search unavailable'));
 const user=setup();await searchIndex(user,'missing');await screen.findByText('No indexed matches');
 await user.click(screen.getByRole('tab',{name:'Find & inspect'}));await user.click(screen.getByRole('button',{name:'Search index'}));expect((await screen.findAllByText(/index_absent/)).length).toBeGreaterThan(0);
 await user.click(screen.getByRole('tab',{name:'Find & inspect'}));await user.click(screen.getByRole('button',{name:'Search index'}));await screen.findByText('Search unavailable');expect(nativeAstApi.importedSample).not.toHaveBeenCalled();
});
it('retains hybrid lexical evidence and vector status without a fallback request',async()=>{
 vi.mocked(nativeAstApi.importedSearch).mockResolvedValue({...searchReply,mode:'hybrid',vector_status:'index_absent'});const user=setup();await ready();
 await user.click(screen.getByRole('combobox',{name:'Search mode'}));await user.click(screen.getByRole('option',{name:'hybrid'}));await searchIndex(user);
 await screen.findByText('Partial search results');expect(nativeAstApi.importedSearch).toHaveBeenCalledTimes(1);expect(nativeAstApi.importedSearch).toHaveBeenCalledWith(installed,{query:'Anchor',mode:'hybrid',metric:'cosine',k:20},expect.any(AbortSignal));
});
it('refreshes active search without replaying an earlier query',async()=>{
 const user=setup();await runQuery(user);await screen.findByText('Query rows · 1');await user.click(screen.getByRole('tab',{name:'Find & inspect'}));await searchIndex(user);await screen.findByRole('button',{name:/Anchor src\/file.rs/});
 await user.click(screen.getByRole('button',{name:'Refresh'}));await waitFor(()=>expect(nativeAstApi.importedSearch).toHaveBeenCalledTimes(2));expect(nativeAstApi.importedQuery).toHaveBeenCalledTimes(1);
});
it('keeps a newer search when an older query resolves last',async()=>{
 let finish!:(v:import('@/api/nativeAst').AstQuery)=>void;vi.mocked(nativeAstApi.importedQuery).mockImplementationOnce(()=>new Promise(resolve=>{finish=resolve}));const user=setup();await runQuery(user);
 await user.click(screen.getByRole('tab',{name:'Find & inspect'}));await searchIndex(user);await act(async()=>finish(scalar('Stale query')));expect(screen.queryByText(/Stale query/)).toBeNull();await screen.findByRole('button',{name:/Anchor src\/file.rs/});
});
it('recovers a causal query failure through header refresh',async()=>{
 vi.mocked(nativeAstApi.importedQuery).mockRejectedValueOnce(new Error('Temporary outage')).mockResolvedValueOnce(scalar('Recovered'));const user=setup();await runQuery(user);await screen.findByText('Temporary outage');
 await user.click(screen.getByRole('button',{name:'Refresh'}));await screen.findByRole('cell',{name:'{"type":"STRING","value":"Recovered"}'});expect(screen.queryByText('Temporary outage')).toBeNull();
});
it('lets header refresh supersede an older pending query',async()=>{
 let finish!:(v:import('@/api/nativeAst').AstQuery)=>void;vi.mocked(nativeAstApi.importedQuery).mockImplementationOnce(()=>new Promise(resolve=>{finish=resolve})).mockResolvedValueOnce(scalar('Fresh refresh'));const user=setup();await runQuery(user);
 await user.click(screen.getByRole('button',{name:'Refresh'}));await screen.findByRole('cell',{name:'{"type":"STRING","value":"Fresh refresh"}'});await act(async()=>finish(scalar('Stale original')));expect(screen.queryByText(/Stale original/)).toBeNull();
});
it('keeps a newer explicit query when an older refresh resolves last',async()=>{
 const user=setup();await runQuery(user,'RETURN first');await screen.findByText('Query rows · 1');let finish!:(v:import('@/api/nativeAst').AstQuery)=>void;
 vi.mocked(nativeAstApi.importedQuery).mockImplementationOnce(()=>new Promise(resolve=>{finish=resolve}));await user.click(screen.getByRole('button',{name:'Refresh'}));await waitFor(()=>expect(nativeAstApi.importedQuery).toHaveBeenCalledTimes(2));
 await user.click(screen.getByRole('tab',{name:'Query lab'}));await user.clear(screen.getByLabelText('Cypher query'));vi.mocked(nativeAstApi.importedQuery).mockResolvedValueOnce(scalar('New result'));await user.type(screen.getByLabelText('Cypher query'),'RETURN second');await user.click(screen.getByRole('button',{name:'Run query'}));
 await screen.findByRole('cell',{name:'{"type":"STRING","value":"New result"}'});await act(async()=>finish(scalar('Stale result')));expect(screen.queryByText(/Stale result/)).toBeNull();
});
it('cancels search and ignores its late output',async()=>{
 let finish!:(v:typeof searchReply)=>void;vi.mocked(nativeAstApi.importedSearch).mockImplementationOnce(()=>new Promise(resolve=>{finish=resolve}));const user=setup();await searchIndex(user);await user.click(screen.getByRole('button',{name:'Cancel search'}));
 expect(vi.mocked(nativeAstApi.importedSearch).mock.calls[0][2]?.aborted).toBe(true);await act(async()=>finish(searchReply));expect(screen.queryByRole('button',{name:/Anchor src\/file.rs/})).toBeNull();
});
function setupNative() {
 useTaskWorkspace.setState({ module: 'ast', contexts: [selection], selection, phase: 'ready' });
 render(<MemoryRouter initialEntries={['/ast/explorer/project']}><Routes><Route path='/ast/explorer/:contextId?' element={<ExplorerPage />} /></Routes></MemoryRouter>);
 return userEvent.setup();
}
async function inspectNativeFile(user: ReturnType<typeof userEvent.setup>) {
 await waitFor(() => expect(nativeAstApi.schema).toHaveBeenCalled());
 await user.click(screen.getByRole('tab', { name: 'Relationship map' }));
 await user.click(await screen.findByRole('button', { name: /Anchor src\/file.rs/ }));
 await user.click(screen.getByRole('button', { name: 'Inspect source & impact' }));
}
it.each([true, false])('keeps current search, neighborhood and Source usable after publication advances (PROJECT=%s)', async native => {
 const generation=nativeSchema.generation.map(byte=>255-byte);
 const query=structuredClone(resolvedQuery);
 (query.rows[0][0].value as {id:{value:{generation:number[]}}}).id.value.generation=generation;
 vi.mocked(native ? nativeAstApi.search : nativeAstApi.importedSearch).mockResolvedValue(searchReply);
 vi.mocked(native ? nativeAstApi.query : nativeAstApi.importedQuery).mockResolvedValue(query);
 vi.mocked(native ? nativeAstApi.neighborhood : nativeAstApi.importedNeighborhood).mockResolvedValue({...neighborhoodPage(),generation});
 const user=native?setupNative():setup();
 await waitFor(()=>expect(native?nativeAstApi.schema:nativeAstApi.importedSchema).toHaveBeenCalledTimes(1));
 await user.type(screen.getByLabelText('Search indexed code'),'Anchor');
 await user.click(screen.getByRole('button',{name:'Search index'}));
 await user.click(await screen.findByRole('button',{name:/Anchor src\/file.rs/}));
 await waitFor(()=>expect(native?nativeAstApi.neighborhood:nativeAstApi.importedNeighborhood).toHaveBeenCalledTimes(2));
 await user.click(screen.getByRole('button',{name:'Inspect source & impact'}));
 await screen.findByText(native?'fn observed() {}':'fn imported_observed() {}');
 await user.click(screen.getByRole('tab',{name:'Incoming'}));
 await waitFor(()=>expect(native?nativeAstApi.query:nativeAstApi.importedQuery).toHaveBeenLastCalledWith(native?{project_dir:selection.project_dir,project_version:selection.project_version}:installed,expect.stringContaining('WHERE anchor.entity_key = $anchor_key'),{anchor_key:'observed-key-A'},expect.any(AbortSignal)));
 expect(screen.queryByText(/The graph changed/)).toBeNull();
 expect(native?nativeAstApi.schema:nativeAstApi.importedSchema).toHaveBeenCalledTimes(1);
 expect(native?nativeAstApi.search:nativeAstApi.importedSearch).toHaveBeenCalledTimes(1);
});
it('shows available search results without cross-component generation metadata',async()=>{
 vi.mocked(nativeAstApi.importedSearch).mockResolvedValue(searchReply);
 const user=setup();await searchIndex(user);
 await screen.findByRole('button',{name:/Anchor src\/file.rs/});
 expect(screen.queryByText(/invalid graph generation/)).toBeNull();
 expect(nativeAstApi.importedQuery).not.toHaveBeenCalled();
});
it.each([true,false])('searches available text when the graph schema is unavailable (PROJECT=%s)',async native=>{
 vi.mocked(native?nativeAstApi.schema:nativeAstApi.importedSchema).mockRejectedValue(new NativeRecordReadError(404,'graph_absent'));
 vi.mocked(native?nativeAstApi.search:nativeAstApi.importedSearch).mockResolvedValue(searchReply);
 const user=native?setupNative():setup();
 await screen.findByText(/graph_absent/);
 await user.type(screen.getByLabelText('Search indexed code'),'Anchor');
 await user.click(screen.getByRole('button',{name:'Search index'}));
 await screen.findByRole('button',{name:/Anchor src\/file.rs/});
 expect(native?nativeAstApi.search:nativeAstApi.importedSearch).toHaveBeenCalledTimes(1);
 expect(native?nativeAstApi.query:nativeAstApi.importedQuery).not.toHaveBeenCalled();
});
it('reads FILE from the native Source DTO with literal Unicode path and observed physical origin', async () => {
 vi.mocked(nativeAstApi.source).mockResolvedValue({ origin: 'physical', selection: { source: 'literal physical source', total_lines: 200, start_line: 42, end_line: 42 } });
 const user = setupNative(); await inspectNativeFile(user);
 await screen.findByText('literal physical source');
 expect(nativeAstApi.source).toHaveBeenCalledWith({ project_dir: selection.project_dir, project_version: selection.project_version }, { path: 'src/file.rs' }, expect.any(AbortSignal));
 expect(screen.getByText(/Physical source read for the selected PROJECT/).textContent).toContain('physical');
 expect(screen.getByText('Native Source result')).toBeTruthy();
})
it('cancels a visible FILE request and ignores a late native Source result', async () => {
 let finish!: (value: import('@/api/nativeAst').AstSourceReply) => void;
 vi.mocked(nativeAstApi.source).mockImplementationOnce(() => new Promise(resolve => { finish = resolve; }));
 const user = setupNative(); await inspectNativeFile(user);
 await user.click(await screen.findByRole('button', { name: 'Cancel source' }));
 expect(screen.getByText('Source read stopped')).toBeTruthy();
 expect(vi.mocked(nativeAstApi.source).mock.calls[0][2]!.aborted).toBe(true);
 await act(async () => finish({ origin: 'indexed', selection: { source: 'late source must stay hidden', total_lines: 1, start_line: 1, end_line: 1 } }));
 expect(screen.queryByText('late source must stay hidden')).toBeNull();
})
it('fences a late FILE denial when only NativePath changes for the same project and version', async () => {
 let fail!: (cause: Error) => void;
 vi.mocked(nativeAstApi.source).mockImplementationOnce(() => new Promise((_resolve, reject) => { fail = reject; }));
 const user = setupNative(); await inspectNativeFile(user);
 const signal = vi.mocked(nativeAstApi.source).mock.calls[0][2]!;
 const next = { ...selection, project_dir: "/ÿq" };
 act(() => useTaskWorkspace.setState({ contexts: [next], selection: next }));
 await waitFor(() => expect(nativeAstApi.schema).toHaveBeenCalledTimes(2));
 await act(async () => fail(new NativeRecordReadError(403)));
 expect(signal.aborted).toBe(true);
 expect(useTaskWorkspace.getState().selection).toEqual(next);
 expect(screen.queryByText(/Source unavailable/)).toBeNull();
 expect(screen.queryByLabelText('Symbol investigation')).toBeNull();
})
it('aborts a FILE read when the inspection closes and discards its late output', async () => {
 let finish!: (value: import('@/api/nativeAst').AstSourceReply) => void;
 vi.mocked(nativeAstApi.source).mockImplementationOnce(() => new Promise(resolve => { finish = resolve; }));
 const user = setupNative(); await inspectNativeFile(user);
 await user.click(screen.getByRole('button', { name: 'Close inspection' }));
 expect(vi.mocked(nativeAstApi.source).mock.calls[0][2]!.aborted).toBe(true);
 await act(async () => finish({ origin: 'physical', selection: { source: 'closed source must stay hidden', total_lines: 1, start_line: 1, end_line: 1 } }));
 expect(screen.queryByText('closed source must stay hidden')).toBeNull();
 expect(screen.queryByLabelText('Symbol investigation')).toBeNull();
})
it('preserves a causal FILE denial and retries the same native selection without a GET fallback', async () => {
 vi.mocked(nativeAstApi.source).mockRejectedValueOnce(new NativeRecordReadError(403));
 const user = setupNative(); await inspectNativeFile(user);
 await screen.findByText(/Source unavailable.*403/);
 await user.click(screen.getByRole('button', { name: 'Load source' }));
 await screen.findByText('fn observed() {}');
 for (const [physical, input] of vi.mocked(nativeAstApi.source).mock.calls) {
  expect(physical).toEqual({ project_dir: selection.project_dir, project_version: selection.project_version });
  expect(input).toEqual({ path: 'src/file.rs' });
 }
})
it('aborts FILE when a new graph result replaces the inspection', async () => {
 let finish!: (value: import('@/api/nativeAst').AstSourceReply) => void;
 vi.mocked(nativeAstApi.source).mockImplementationOnce(() => new Promise(resolve => { finish = resolve; }));
 const user = setupNative(); await inspectNativeFile(user);
 const signal = vi.mocked(nativeAstApi.source).mock.calls[0][2]!;
 await user.click(screen.getByRole('tab', { name: 'Query lab' }));
 await user.type(screen.getByLabelText('Cypher query'), 'RETURN n');
 await user.click(screen.getByRole('button', { name: 'Run query' }));
 await screen.findByText('Query rows · 1');
 expect(signal.aborted).toBe(true);
 await act(async () => finish({ origin: 'indexed', selection: { source: 'replaced source must stay hidden', total_lines: 1, start_line: 1, end_line: 1 } }));
 expect(screen.queryByText('replaced source must stay hidden')).toBeNull();
})
it('runs PROJECT query as native typed rows without legacy graph or automatic sample', async () => {
 const user = setupNative();
 await waitFor(() => expect(nativeAstApi.schema).toHaveBeenCalled());
 await user.click(screen.getByRole('tab',{ name: 'Query lab' }));
 await user.type(screen.getByLabelText('Cypher query'),'RETURN 9007199254740993');
 await user.click(screen.getByRole('button',{ name: 'Run query' }));
 await screen.findByText('Query rows · 1');
 expect(nativeAstApi.query).toHaveBeenCalledWith({ project_dir: selection.project_dir, project_version: selection.project_version },'RETURN 9007199254740993',{},expect.any(AbortSignal));
 expect(nativeAstApi.sample).not.toHaveBeenCalled();
 expect(screen.getByRole('cell',{ name: '{"type":"INT64","value":"9007199254740993"}' })).toBeTruthy();
});
it('uses the sample observed PK for a native neighborhood even when UIDs collide', async () => {
 const user = setupNative(); await waitFor(() => expect(nativeAstApi.schema).toHaveBeenCalled());
 await user.click(screen.getByRole('tab',{ name: 'Relationship map' }));
 await user.click(await screen.findByRole('button',{ name: /Anchor src\/file.rs/ }));
 await waitFor(() => expect(nativeAstApi.neighborhood).toHaveBeenCalledTimes(2));
 for (const [physical,input] of vi.mocked(nativeAstApi.neighborhood).mock.calls) { expect(physical).toEqual({ project_dir: selection.project_dir, project_version: selection.project_version }); expect(input.anchor_key).toEqual({ type: 'string', value: 'observed-key-A' }); }
});
it('aborts and fences a late denied schema when the exact native version changes', async () => {
 let reject!: (cause: Error) => void; vi.mocked(nativeAstApi.schema).mockImplementationOnce(() => new Promise((_resolve,fail) => { reject = fail; }));
 setupNative(); const signal = vi.mocked(nativeAstApi.schema).mock.calls[0][1]!;
 const next = { ...selection, project_version: 'other literal version' };
 act(() => useTaskWorkspace.setState({ contexts: [next], selection: next }));
 await waitFor(() => expect(nativeAstApi.schema).toHaveBeenCalledTimes(2));
 await act(async () => reject(new NativeRecordReadError(403)));
 expect(signal.aborted).toBe(true); expect(useTaskWorkspace.getState().selection).toEqual(next); expect(screen.queryByText('Working context access denied')).toBeNull();
});
it('cancels a visible native query and ignores late output without sample fallback', async () => {
 let finish!: (value: typeof nativeQuery) => void; vi.mocked(nativeAstApi.query).mockImplementationOnce(() => new Promise(resolve => { finish = resolve; }));
 const user = setupNative(); await waitFor(() => expect(nativeAstApi.schema).toHaveBeenCalled());
 await user.click(screen.getByRole('tab',{ name: 'Query lab' })); await user.type(screen.getByLabelText('Cypher query'),'RETURN n'); await user.click(screen.getByRole('button',{ name: 'Run query' }));
 await user.click(screen.getByRole('button',{ name: 'Cancel graph request' }));
 expect(vi.mocked(nativeAstApi.query).mock.calls[0][3]?.aborted).toBe(true);
 await act(async () => finish(nativeQuery)); expect(screen.queryByText('Query rows · 1')).toBeNull(); expect(screen.getByText('Graph request stopped.')).toBeTruthy(); expect(nativeAstApi.sample).not.toHaveBeenCalled();
});

function SwitchToNative() { const navigate = useNavigate(); return <button onClick={() => { useTaskWorkspace.setState({ module: 'ast', contexts: [selection], selection, phase: 'ready' }); navigate('/ast/explorer/project'); }}>Switch to native fixture</button>; }
it('fences a delayed imported Retry response after switching to the native physical context', async () => {
 let finish!: (value: typeof nativeSchema) => void;
 const original = nativeSchema; vi.mocked(nativeAstApi.importedSchema).mockClear().mockRejectedValueOnce(new Error('Imported schema failed')).mockImplementationOnce(() => new Promise(resolve => { finish = resolve; }));
 render(<MemoryRouter initialEntries={[importedAstHref(installed)]}><SwitchToNative /><Routes><Route path='/ast/explorer/:contextId?' element={<ExplorerPage />} /></Routes></MemoryRouter>);
 const user = userEvent.setup(); await screen.findByText('Imported schema failed'); await user.click(screen.getByRole('button',{name:'Retry request'}));
 await user.click(screen.getByRole('button',{name:'Switch to native fixture'})); await waitFor(() => expect(nativeAstApi.schema).toHaveBeenCalled());
 await act(async () => finish({ ...original, nodes: [{ ...nativeSchema.nodes[0], label: 'Stale retry label' }] }));
 expect(screen.queryByText('Stale retry label')).toBeNull(); expect(screen.queryByText('Imported schema failed')).toBeNull(); expect(useTaskWorkspace.getState().selection).toEqual(selection);
});
it('does not replay a native relation anchor from an older generation during header refresh', async () => {
 const physical = { ...selection, project_dir: "/p" };
 useTaskWorkspace.setState({ module:'ast',contexts:[physical],selection:physical,phase:'ready' }); vi.mocked(nativeRecordContexts).mockResolvedValue({ contexts:[physical],current:physical });
 render(<MemoryRouter initialEntries={['/ast/explorer/project']}><WorkspaceRefreshProvider><TaskWorkspaceSelectors module='ast'/><Routes><Route path='/ast/explorer/:contextId?' element={<ExplorerPage/>}/></Routes></WorkspaceRefreshProvider></MemoryRouter>);
 const user = userEvent.setup(); await waitFor(() => expect(nativeAstApi.schema).toHaveBeenCalled()); await user.click(screen.getByRole('tab',{name:'Relationship map'}));
 await user.click(await screen.findByRole('button',{name:/Anchor src\/file.rs/})); await user.click(screen.getByRole('button',{name:'Inspect source & impact'})); await user.click(screen.getByRole('tab',{name:'Incoming'}));
 await waitFor(() => expect(nativeAstApi.query).toHaveBeenCalledTimes(1));
 const nextGeneration = nativeSchema.generation.map(byte => 255-byte);
 vi.mocked(nativeAstApi.schema).mockResolvedValue({ ...nativeSchema,generation:nextGeneration }); vi.mocked(nativeAstApi.sample).mockResolvedValue({ ...nativeSample,generation:nextGeneration });
 await user.click(screen.getByRole('button',{name:'Refresh'})); await waitFor(() => expect(nativeAstApi.sample).toHaveBeenCalledTimes(2));
 expect(nativeAstApi.query).toHaveBeenCalledTimes(1); expect(screen.queryByLabelText('Symbol investigation')).toBeNull();
});

it('keeps a literal percent-encoded import identity after the router decodes its URI once', async () => {
 const context = installed.context;
 render(<MemoryRouter initialEntries={['/ast/explorer/'+encodeURIComponent(context)]}><Routes><Route path='/ast/explorer/:contextId?' element={<ExplorerPage/>}/></Routes></MemoryRouter>);
 await waitFor(()=>expect(nativeAstApi.importedSchema).toHaveBeenCalledWith(installed,expect.any(AbortSignal)));
});

it('rejects an imported intent for the PROJECT context before either native or legacy reads', async () => {
 render(<MemoryRouter initialEntries={['/ast/explorer/project?library=imported']}><Routes><Route path='/ast/explorer/:contextId?' element={<ExplorerPage/>}/></Routes></MemoryRouter>);
 await screen.findByText('A PROJECT graph cannot be selected as an imported index.'); expect(nativeAstApi.schema).not.toHaveBeenCalled();
});

it('preserves native typed query rows while schema metadata is still pending', async () => {
 vi.mocked(nativeAstApi.query).mockResolvedValueOnce(nativeQueryWithRelation);
 vi.mocked(nativeAstApi.schema).mockImplementationOnce(() => new Promise(() => {}));
 const user = setupNative(); await user.click(screen.getByRole('tab',{name:'Query lab'})); await user.type(screen.getByLabelText('Cypher query'),'RETURN n'); await user.click(screen.getByRole('button',{name:'Run query'}));
 await screen.findByText('Query rows · 1'); expect(screen.getByRole('cell',{name:'{"type":"INT64","value":"9007199254740993"}'})).toBeTruthy(); expect(screen.getByRole('cell',{name:JSON.stringify(nativeQueryWithRelation.rows[0][2])})).toBeTruthy(); expect(nativeAstApi.sample).not.toHaveBeenCalled();
});

it('does not label unavailable search as healthy zero hits',async()=>{
 vi.mocked(nativeAstApi.importedSearch).mockResolvedValue({status:'index_absent',results:[]});
 const user=setup();await searchIndex(user);await screen.findByText('Search unavailable');
 expect(screen.queryByText('No indexed matches')).toBeNull();expect(nativeAstApi.importedSearch).toHaveBeenCalledTimes(1);
});
it('revalidates the installed catalogue during refresh and clears a removed exact binding',async()=>{
 const user=setup();await searchIndex(user);await screen.findByRole('button',{name:/Anchor src\/file.rs/});vi.mocked(nativeAstApi.imports).mockResolvedValue({imports:[]});
 await user.click(screen.getByRole('button',{name:'Refresh'}));await screen.findByText('The exact imported AST context was not returned by the installed catalogue.');
 expect(screen.queryByRole('button',{name:/Anchor src\/file.rs/})).toBeNull();expect(nativeAstApi.importedSearch).toHaveBeenCalledTimes(1);expect(nativeAstApi.importedSchema).toHaveBeenCalledTimes(1);
});
it('aborts open Source during refresh and never replays the captured old inspection',async()=>{
 let finish!:(v:import('@/api/nativeAst').AstSourceReply)=>void;vi.mocked(nativeAstApi.importedSource).mockImplementationOnce(()=>new Promise(resolve=>{finish=resolve}));
 const user=setup();await ready();await user.click(screen.getByRole('tab',{name:'Relationship map'}));await user.click(await screen.findByRole('button',{name:/Anchor src\/file.rs/}));await user.click(screen.getByRole('button',{name:'Inspect source & impact'}));
 const signal=vi.mocked(nativeAstApi.importedSource).mock.calls[0][2]!;await user.click(screen.getByRole('button',{name:'Refresh'}));await waitFor(()=>expect(nativeAstApi.importedSample).toHaveBeenCalledTimes(2));
 expect(signal.aborted).toBe(true);expect(nativeAstApi.importedSource).toHaveBeenCalledTimes(1);expect(screen.queryByLabelText('Symbol investigation')).toBeNull();
 await act(async()=>finish({origin:'indexed',selection:{source:'late old source',total_lines:1}}));expect(screen.queryByText('late old source')).toBeNull();
});
it('shows published file observations separately from the literal Source response',async()=>{
 const observationSchema={...nativeSchema,nodes:[...nativeSchema.nodes,{label:'AstFileObservation',primary_key:'path',rows:'1',columns:[['path','STRING'],['resolution_outcomes_json','STRING'],['capabilities_json','STRING'],['diagnostics_json','STRING']] as [string,string][],language_counts:[]}]};
 vi.mocked(nativeAstApi.importedSchema).mockResolvedValue(observationSchema);
 const properties={path:'src/file.rs',lang:'rust',resolution_outcomes_json:'[{"target":{"state":"ObservedOnly"},"evidence":"published"}]',capabilities_json:'{"resolver":"ObservedOnly"}',diagnostics_json:'[]'};
 const observation:import('@/api/nativeAst').AstQuery={columns:['f'],rows:[[{type:'NODE',value:{id:{type:'INTERNAL_ID',value:{generation:nativeSchema.generation,table:8,row:'42'}},label:{type:'STRING',value:'AstFileObservation'},properties:{type:'PROPERTIES',value:Object.fromEntries(Object.entries(properties).map(([key,value])=>[key,{type:'STRING',value}]))}}}]]};
 vi.mocked(nativeAstApi.importedQuery).mockImplementation(async(_source,query)=>query.includes('AstFileObservation')?observation:resolvedQuery);
 const user=setup();await searchIndex(user);await user.click(await screen.findByRole('button',{name:/Anchor src\/file.rs/}));await user.click(screen.getByRole('button',{name:'Inspect source & impact'}));
 await screen.findByText('fn imported_observed() {}');const summary=await screen.findByText('Published file observations');
 expect(summary.parentElement).not.toHaveAttribute('open');expect(vi.mocked(nativeAstApi.importedQuery).mock.calls.filter(([,query])=>query.includes('AstFileObservation'))).toHaveLength(0);
 await user.click(summary);expect((await screen.findAllByText(/ObservedOnly/)).length).toBeGreaterThan(0);
 expect(nativeAstApi.importedQuery).toHaveBeenCalledWith(installed,'MATCH (f:AstFileObservation) WHERE f.path = $source_path RETURN f LIMIT 2',{source_path:'src/file.rs'},expect.any(AbortSignal));
});

it('preserves an authored query through an exact installed version switch and executes only explicitly',async()=>{
 const other={...installed,version:'next literal version',context:'library@next literal version'};
 vi.mocked(nativeAstApi.imports).mockResolvedValue({imports:[installed,other]});
 const user=setup();await ready();await user.click(screen.getByRole('tab',{name:'Query lab'}));
 await user.type(screen.getByLabelText('Cypher query'),'MATCH (n:Function) RETURN n');
 await user.click(screen.getByRole('combobox',{name:'AST artifact and version'}));await user.click(screen.getByRole('option',{name:other.id+' · '+other.version}));
 await waitFor(()=>expect(nativeAstApi.importedSchema).toHaveBeenLastCalledWith(other,expect.any(AbortSignal)));
 expect(screen.getByLabelText('Cypher query')).toHaveValue('MATCH (n:Function) RETURN n');
 expect(nativeAstApi.importedQuery).not.toHaveBeenCalled();
 await user.click(screen.getByRole('button',{name:'Run query'}));
 await waitFor(()=>expect(nativeAstApi.importedQuery).toHaveBeenCalledWith(other,'MATCH (n:Function) RETURN n',{},expect.any(AbortSignal)));
});

it('reads a literal PROJECT PATH without a graph node or entity and displays real physical origin',async()=>{
 vi.mocked(nativeAstApi.source).mockResolvedValue({origin:'physical',selection:{source:'actual physical file bytes\n',total_lines:1,start_line:1,end_line:1}});
 const user=setupNative();await user.type(screen.getByLabelText('File path'),' raw.unknown ');await user.click(screen.getByRole('button',{name:'Read source file'}));
 await screen.findByText('actual physical file bytes');
 expect(nativeAstApi.source).toHaveBeenCalledWith({project_dir:selection.project_dir,project_version:selection.project_version},{path:' raw.unknown '},expect.any(AbortSignal));
 expect(nativeAstApi.sample).not.toHaveBeenCalled();expect(nativeAstApi.query).not.toHaveBeenCalled();expect(screen.queryByLabelText('Symbol investigation')).toBeNull();expect(screen.getByLabelText('File source')).toBeTruthy();
 expect(screen.getByText(/Physical source read for the selected PROJECT/)).toBeTruthy();
});
it('clears the previous source when a different file read fails',async()=>{
 const user=setupNative();await user.type(screen.getByLabelText('File path'),'first.rs');await user.click(screen.getByRole('button',{name:'Read source file'}));
 await screen.findByText('fn observed() {}');
 vi.mocked(nativeAstApi.source).mockRejectedValueOnce(new NativeRecordReadError(403,'Denied'));
 await user.clear(screen.getByLabelText('File path'));await user.type(screen.getByLabelText('File path'),'second.rs');await user.click(screen.getByRole('button',{name:'Read source file'}));
 expect(await screen.findByRole('alert')).toHaveTextContent('Denied');expect(screen.queryByText('fn observed() {}')).toBeNull();expect(screen.getByText('No source open')).toBeTruthy();
 expect(screen.queryByText('The previously loaded source remains visible.')).toBeNull();
 expect(nativeAstApi.source).toHaveBeenLastCalledWith({project_dir:selection.project_dir,project_version:selection.project_version},{path:'second.rs'},expect.any(AbortSignal));
});
it('cancels a PATH read and discards a genuine late Source payload',async()=>{
 let finish!:(value:import('@/api/nativeAst').AstSourceReply)=>void;vi.mocked(nativeAstApi.source).mockImplementationOnce(()=>new Promise(resolve=>{finish=resolve}));
 const user=setupNative();await user.type(screen.getByLabelText('File path'),'raw.unknown');await user.click(screen.getByRole('button',{name:'Read source file'}));await user.click(screen.getByRole('button',{name:'Cancel source'}));
 expect(vi.mocked(nativeAstApi.source).mock.calls[0][2]!.aborted).toBe(true);await act(async()=>finish({origin:'physical',selection:{source:'cancelled old file',total_lines:1}}));expect(screen.queryByText('cancelled old file')).toBeNull();expect(screen.getByText('Source read stopped')).toBeTruthy();
});
it('retries the captured PATH after denial even when the authored next file path changes',async()=>{
 vi.mocked(nativeAstApi.source).mockRejectedValueOnce(new NativeRecordReadError(403));
 const user=setupNative();await user.type(screen.getByLabelText('File path'),'raw.unknown');await user.click(screen.getByRole('button',{name:'Read source file'}));await screen.findByText(/Source unavailable.*403/);
 await user.clear(screen.getByLabelText('File path'));await user.type(screen.getByLabelText('File path'),'other.unknown');await user.click(screen.getByRole('button',{name:'Load source'}));await screen.findByText('fn observed() {}');
 expect(nativeAstApi.source).toHaveBeenLastCalledWith({project_dir:selection.project_dir,project_version:selection.project_version},{path:'raw.unknown'},expect.any(AbortSignal));
});
it('does not expose PROJECT filesystem PATH controls for an admitted imported artifact',async()=>{
 setup();await ready();expect(screen.queryByLabelText('File path')).toBeNull();expect(nativeAstApi.source).not.toHaveBeenCalled();
});

it.each([
 { native: false, option: 'Off', rerank: false },
 { native: false, option: 'On', rerank: true },
 { native: true, option: 'Off', rerank: false },
 { native: true, option: 'On', rerank: true },
])('sends explicit $option reranking for native=$native without changing the source identity', async ({ native, option, rerank }) => {
 vi.mocked(nativeAstApi.search).mockResolvedValue(searchReply);
 const user = native ? setupNative() : setup();
 await waitFor(() => expect(native ? nativeAstApi.schema : nativeAstApi.importedSchema).toHaveBeenCalled());
 await user.click(screen.getByRole('combobox', { name: 'AST reranking' })); await user.click(screen.getByRole('option', { name: option }));
 await user.type(screen.getByLabelText('Search indexed code'), 'Anchor'); await user.click(screen.getByRole('button', { name: 'Search index' }));
 expect(await screen.findByRole('button', { name: /Anchor src\/file.rs/ })).toHaveTextContent('rust');
 expect(native ? nativeAstApi.search : nativeAstApi.importedSearch).toHaveBeenCalledWith(native ? { project_dir: selection.project_dir, project_version: selection.project_version } : installed, { query: 'Anchor', mode: 'semantic', metric: 'cosine', k: 20, rerank }, expect.any(AbortSignal));
});
it('preserves backend search order and displays rerank scores separately without client ranking', async () => {
 vi.mocked(nativeAstApi.importedSearch).mockResolvedValue({ ...searchReply, results: [
  { ...searchReply.results[0], name: 'Zulu', lexical_score: 2, distance: 0.25, score: 0.5, rerank_score: 0 },
  { ...searchReply.results[0], entity_key: 'observed-key-B', uid: 'other-uid', data_key: 'other-data', name: 'Alpha', rerank_score: 1 },
 ] });
 const user = setup(); await searchIndex(user); await screen.findByText('Rerank score: 0');
 const results = screen.getAllByRole('button').filter(button => /^#[12] (Zulu|Alpha)/.test(button.textContent || ''));
 expect(results.map(button => button.textContent?.match(/^#[12] (Zulu|Alpha)/)?.[0])).toEqual(['#1 Zulu', '#2 Alpha']);
 const details = screen.getByText('Native search response').parentElement?.querySelector('pre')?.textContent;
 expect(details).toContain('"lexical_score": 2'); expect(details).toContain('"distance": 0.25'); expect(details).toContain('"score": 0.5');
 expect(details).toContain('"rerank_score": 0'); expect(details).toContain('"entity_key": "observed-key-A"');
});
it.each(['reranking', 'query'] as const)('aborts and invalidates pending AST search when %s changes', async change => {
 let finish!: (value: typeof searchReply) => void;
 vi.mocked(nativeAstApi.importedSearch).mockImplementationOnce(() => new Promise(resolve => { finish = resolve; }));
 const user = setup(); await searchIndex(user); const signal = vi.mocked(nativeAstApi.importedSearch).mock.calls[0][2]!;
 await user.click(screen.getByRole('tab', { name: 'Find & inspect' }));
 if (change === 'reranking') { await user.click(screen.getByRole('combobox', { name: 'AST reranking' })); await user.click(screen.getByRole('option', { name: 'On' })); }
 else await user.type(screen.getByLabelText('Search indexed code'), ' changed');
 expect(signal.aborted).toBe(true);
 await act(async () => finish(searchReply));
 expect(screen.queryByRole('button', { name: /Anchor src\/file.rs/ })).toBeNull();
 expect(screen.queryByText('Native search response')).toBeNull();
 await user.click(screen.getByRole('button', { name: 'Search index' })); await screen.findByRole('button', { name: /Anchor src\/file.rs/ });
 expect(vi.mocked(nativeAstApi.importedSearch).mock.calls[1][1]).toEqual({ query: change === 'query' ? 'Anchor changed' : 'Anchor', mode: 'semantic', metric: 'cosine', k: 20, ...(change === 'reranking' ? { rerank: true } : {}) });
});
it('keeps a requested reranker failure visible without a second fallback search', async () => {
 vi.mocked(nativeAstApi.importedSearch).mockRejectedValue(new Error('rerank_service_unavailable'));
 const user = setup(); await ready(); await user.click(screen.getByRole('combobox', { name: 'AST reranking' })); await user.click(screen.getByRole('option', { name: 'On' }));
 await searchIndex(user); await screen.findByText('rerank_service_unavailable');
 expect(nativeAstApi.importedSearch).toHaveBeenCalledTimes(1); expect(nativeAstApi.importedSample).not.toHaveBeenCalled();
});


const observationSchema: import('@/api/nativeAst').AstSchema = {
 ...nativeSchema,
 nodes: [...nativeSchema.nodes, { label: 'AstFileObservation', primary_key: 'path', rows: '1', columns: [['path', 'STRING'], ['resolution_outcomes_json', 'STRING'], ['diagnostics_json', 'STRING'], ['capabilities_json', 'STRING']], language_counts: [] }],
};
function observationReply(properties: Record<string, string> = { diagnostics_json: '[]' }): import('@/api/nativeAst').AstQuery {
 return { columns: ['f'], rows: [[{ type: 'NODE', value: {
  id: { type: 'INTERNAL_ID', value: { generation: nativeSchema.generation, table: 8, row: '1' } },
  label: { type: 'STRING', value: 'AstFileObservation' },
  properties: { type: 'PROPERTIES', value: Object.fromEntries(Object.entries({ path: 'src/file.rs', ...properties }).map(([key, value]) => [key, { type: 'STRING', value }])) },
 } }]] };
}
async function readObservationSource(user: ReturnType<typeof userEvent.setup>, path = 'src/file.rs') {
 await waitFor(() => expect(nativeAstApi.schema).toHaveBeenCalled());
 await user.clear(screen.getByLabelText('File path'));
 await user.type(screen.getByLabelText('File path'), path);
 await user.click(screen.getByRole('button', { name: 'Read source file' }));
 await screen.findByText('fn observed() {}');
}
it('shows Source immediately and queries complete lossless observations only after opening the disclosure', async () => {
 vi.mocked(nativeAstApi.schema).mockResolvedValue(observationSchema);
 const records = JSON.stringify(Array.from({ length: 200 }, (_, index) => 'observation-' + index));
 const json = '{"exact":9007199254740993}';
 vi.mocked(nativeAstApi.query).mockResolvedValue(observationReply({ resolution_outcomes_json: records, diagnostics_json: '[]', capabilities_json: json }));
 const parsing = vi.spyOn(nativeJson, 'parseNativeJson');
 const user = setupNative(); await readObservationSource(user);
 const summary = screen.getByText('Published file observations');
 expect(summary.tagName).toBe('SUMMARY'); expect(summary.parentElement).not.toHaveAttribute('open');
 expect(screen.queryByRole('button', { name: 'Cancel source' })).toBeNull();
 expect(nativeAstApi.query).not.toHaveBeenCalled(); expect(parsing).not.toHaveBeenCalledWith(records); expect(parsing).not.toHaveBeenCalledWith(json);
 expect(screen.getByText('fn observed() {}').compareDocumentPosition(summary) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
 await user.click(summary);
 await screen.findByText(/observation-199/);
 expect(nativeAstApi.query).toHaveBeenCalledWith({ project_dir: selection.project_dir, project_version: selection.project_version }, 'MATCH (f:AstFileObservation) WHERE f.path = $source_path RETURN f LIMIT 2', { source_path: 'src/file.rs' }, expect.any(AbortSignal));
 const output = summary.parentElement!.querySelector('pre')!.textContent!;
 expect(output).toContain('"resolution_outcomes_json": ['); expect(output).toContain('observation-0'); expect(output).toContain('observation-199'); expect(output).toContain('9007199254740993');
 expect(screen.getByText('Native Source result')).toBeTruthy();
});
it.each(['denied', 'invalid JSON', 'empty error'] as const)('keeps Source available after observations are %s and retries only those observations', async failure => {
 vi.mocked(nativeAstApi.schema).mockResolvedValue(observationSchema);
 if (failure === 'denied') vi.mocked(nativeAstApi.query).mockRejectedValueOnce(new NativeRecordReadError(403));
 else if (failure === 'empty error') vi.mocked(nativeAstApi.query).mockRejectedValueOnce(new Error(''));
 else vi.mocked(nativeAstApi.query).mockResolvedValueOnce(observationReply({ diagnostics_json: '{invalid' }));
 vi.mocked(nativeAstApi.query).mockResolvedValue(observationReply({ diagnostics_json: '["retried evidence"]' }));
 const user = setupNative(); await readObservationSource(user); await user.click(screen.getByText('Published file observations'));
 await screen.findByText('Published observations unavailable'); expect(screen.getByText('fn observed() {}')).toBeTruthy();
 if (failure === 'denied') expect(screen.getByText(/403/)).toBeTruthy();
 await user.click(screen.getByRole('button', { name: 'Retry observations' })); await screen.findByText(/retried evidence/);
 expect(nativeAstApi.source).toHaveBeenCalledTimes(1); expect(nativeAstApi.query).toHaveBeenCalledTimes(2);
 for (const [binding, , parameters] of vi.mocked(nativeAstApi.query).mock.calls) { expect(binding).toEqual({ project_dir: selection.project_dir, project_version: selection.project_version }); expect(parameters).toEqual({ source_path: 'src/file.rs' }); }
});
it.each(['empty', 'ambiguous', 'advanced'] as const)('preserves the %s observation response without inventing evidence', async outcome => {
 vi.mocked(nativeAstApi.schema).mockResolvedValue(observationSchema);
 const reply = observationReply();
 if (outcome === 'empty') reply.rows = [];
 else {
  const node = reply.rows[0][0].value as { id: { value: { generation: number[]; row: string } } };
  if (outcome === 'advanced') node.id.value.generation = Array.from({ length: 32 }, () => 255);
  else { const duplicate = structuredClone(reply.rows[0]); (duplicate[0].value as typeof node).id.value.row = '2'; reply.rows.push(duplicate); }
 }
 vi.mocked(nativeAstApi.query).mockResolvedValue(reply);
 const user = setupNative(); await readObservationSource(user); await user.click(screen.getByText('Published file observations'));
 if (outcome === 'empty') await screen.findByText('No published file observations for this source.');
 else if (outcome === 'ambiguous') { await screen.findByText('Published observations unavailable'); expect(screen.getByText(/ambiguous/)).toBeTruthy(); }
 else { await waitFor(()=>expect(screen.getByText(/diagnostics_json/)).toBeTruthy()); expect(screen.queryByText('Published observations unavailable')).toBeNull(); }
 expect(screen.getByText('fn observed() {}')).toBeTruthy(); expect(nativeAstApi.query).toHaveBeenCalledTimes(1);
});
it.each(['disclosure', 'Source', 'scope'] as const)('aborts observations when %s changes and never parses the late payload', async change => {
 vi.mocked(nativeAstApi.schema).mockResolvedValue(observationSchema);
 let finish!: (value: import('@/api/nativeAst').AstQuery) => void;
 vi.mocked(nativeAstApi.query).mockImplementationOnce(() => new Promise(resolve => { finish = resolve; }));
 const parsing = vi.spyOn(nativeJson, 'parseNativeJson');
 const user = setupNative(); await readObservationSource(user);
 const summary = screen.getByText('Published file observations'); await user.click(summary);
 await screen.findByRole('button', { name: 'Cancel observations' }); const signal = vi.mocked(nativeAstApi.query).mock.calls[0][3]!;
 expect(screen.getByText('fn observed() {}')).toBeTruthy();
 if (change === 'disclosure') { await user.click(screen.getByRole('button', { name: 'Cancel observations' })); expect(summary).toHaveFocus(); expect(summary.parentElement).not.toHaveAttribute('open'); }
 else if (change === 'Source') await readObservationSource(user, 'src/other.rs');
 else { const next = { ...selection, project_dir: "/ÿq" }; act(() => useTaskWorkspace.setState({ contexts: [next], selection: next })); await waitFor(() => expect(nativeAstApi.schema).toHaveBeenCalledTimes(2)); }
 expect(signal.aborted).toBe(true);
 await act(async () => finish(observationReply({ diagnostics_json: '["late observation must stay hidden"]' })));
 expect(parsing).not.toHaveBeenCalledWith('["late observation must stay hidden"]'); expect(screen.queryByText(/late observation must stay hidden/)).toBeNull();
 if (change === 'disclosure' || change === 'Source') expect(screen.getByText('Published file observations').parentElement).not.toHaveAttribute('open');
});
