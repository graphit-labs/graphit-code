
import '@/test/contextControls'
import {afterEach,beforeEach,expect,it,vi} from 'vitest'
import {cleanup,render,screen,waitFor} from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import {MemoryRouter,useLocation} from 'react-router-dom'
import {nativeAstApi} from '@/api/nativeAst'
import {useTaskWorkspace,stopTaskWorkspace} from '@/components/task/taskWorkspaceState'
import {requestedRecordSelection,requestedImportedAstSource} from '@/api/nativeRecordNavigation'
import {selection,installed} from './nativeAst.test-fixtures'
import ContextsPage from './ContextsPage'
vi.mock('@/api/nativeAst',async original=>({...await original<typeof import('@/api/nativeAst')>(),nativeAstApi:{imports:vi.fn()}}))
afterEach(cleanup)
beforeEach(()=>{vi.resetAllMocks();stopTaskWorkspace();vi.mocked(nativeAstApi.imports).mockResolvedValue({imports:[installed]});})
function URI(){const location=useLocation();return <output aria-label='Current URI'>{location.pathname+location.search}</output>}
it('renders own AST PROJECT selections and preserves the literal physical tuple',async()=>{
 useTaskWorkspace.setState({module:'ast',contexts:[selection],selection,phase:'ready'});
 render(<MemoryRouter initialEntries={['/ast/contexts']}><ContextsPage/><URI/></MemoryRouter>);const user=userEvent.setup();
 await user.click(await screen.findByRole('button',{name:/observed-project · literal\/version/}));await user.click(screen.getByRole('button',{name:'Investigate this context'}));
 const url=new URL(screen.getByLabelText('Current URI').textContent!,window.location.origin);expect(url.pathname).toBe('/ast/explorer/project');expect(requestedRecordSelection(url.search)).toEqual(selection);
});
it('opens an observed installed source without PROJECT binding or a fabricated count',async()=>{
 useTaskWorkspace.setState({module:'ast',contexts:[selection],selection,phase:'ready'});
 render(<MemoryRouter initialEntries={['/ast/contexts']}><ContextsPage/><URI/></MemoryRouter>);const user=userEvent.setup();
 await user.click(await screen.findByRole('button',{name:/library%2Fliteral · exact\/version/}));await user.click(screen.getByRole('button',{name:'Investigate this context'}));
 const url=new URL(screen.getByLabelText('Current URI').textContent!,window.location.origin);expect(decodeURIComponent(url.pathname)).toBe('/ast/explorer/'+installed.context);
 expect(requestedImportedAstSource(url.search)).toEqual({id:installed.id,version:installed.version});expect(requestedRecordSelection(url.search)).toBeNull();expect(screen.queryByRole('button',{name:/Unlink/})).toBeNull();
});
it('loads installed contexts without PROJECT admission and retries a causal denial',async()=>{
 vi.mocked(nativeAstApi.imports).mockRejectedValueOnce(new Error('ReadAst denied'));
 render(<MemoryRouter initialEntries={['/ast/contexts?library=imported']}><ContextsPage/></MemoryRouter>);const user=userEvent.setup();
 await screen.findByText('ReadAst denied');expect(screen.queryByText('No contexts available')).toBeNull();await user.click(screen.getByRole('button',{name:'Retry catalogue'}));await screen.findByRole('button',{name:/library%2Fliteral · exact\/version/});
 expect(nativeAstApi.imports).toHaveBeenCalledTimes(2);
});
it('aborts the installation inventory on unmount',async()=>{
 vi.mocked(nativeAstApi.imports).mockImplementation(()=>new Promise(()=>{}));const view=render(<MemoryRouter initialEntries={['/ast/contexts?library=imported']}><ContextsPage/></MemoryRouter>);
 await waitFor(()=>expect(nativeAstApi.imports).toHaveBeenCalled()); const signal=vi.mocked(nativeAstApi.imports).mock.calls[0][0]!;view.unmount();expect(signal.aborted).toBe(true);
});
