import '@/test/contextControls';
import { cleanup, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { MemoryRouter } from 'react-router-dom';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { useAppStore } from '@/store/appStore';
import WorkspacePage from './WorkspacePage';
import EcosystemDashboard from './EcosystemDashboard';
vi.mock('@/hooks/useTheme',()=>({useTheme:()=>({theme:'light'})}));
vi.mock('./WorkspaceNow',()=>({WorkspaceNow:()=>null}));
const description='**Project purpose**\n\n- Preserve `decisions`\n- Read the [contract](https://example.test/contract)';
beforeEach(()=>{
 const workspace={id:'p',name:'**Literal name**',dir:'/project',description,registered_at:'2026-09-21',cluster:{}};
 useAppStore.setState({
  projectsLoaded:true,activeProjectKey:'workspace:p:/project',activeProjectOrigin:'workspace',activeProjectId:'p',
  activeProjectDir:'/project',projects:[workspace],projectCatalog:[{id:'p',name:workspace.name,description,cluster:{},workspace}],
 });
});
afterEach(cleanup);
function expectDescription(){
 expect(screen.getByText('Project purpose').tagName).toBe('STRONG');
 expect(screen.getByText('decisions').tagName).toBe('CODE');
 expect(screen.getByText('decisions').closest('li')).toBeTruthy();
 const link=screen.getByRole('link',{name:'contract'});
 expect(link.getAttribute('href')).toBe('https://example.test/contract');expect(link.closest('button')).toBeNull();
}
it('renders project Markdown in Workspace while metadata remains literal',async()=>{
 const user=userEvent.setup();render(<MemoryRouter><WorkspacePage/></MemoryRouter>);
 await user.click(screen.getByRole('tab',{name:'Continue work'}));
 expectDescription();expect(screen.getByText('/project')).toBeTruthy();
});
it('renders project Markdown in the ecosystem inspector while its title remains literal',()=>{
 render(<EcosystemDashboard/>);expectDescription();
 expect(screen.getByRole('heading',{name:'**Literal name**'})).toBeTruthy();
});
