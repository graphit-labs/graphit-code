import '@/test/contextControls';
import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { ArtifactCard } from './ArtifactCard';
import { SubmitModal } from './modals/SubmitModal';
import UploadPage from './UploadPage';
import { hubApi, type InstalledArtifact, type RegistryEntry } from '@/api/hub';
import { useAppStore } from '@/store/appStore';
vi.mock('@/hooks/useTheme',()=>({useTheme:()=>({theme:'light'})}));
vi.mock('@/api/hub',()=>({hubApi:{getGitAuthor:vi.fn(async()=>({author:'Author'})),upload:vi.fn()}}));
const description='**Purpose**\n\n- Preserve `contracts`\n- Read the [guide](https://example.test/guide)';
const artifact={local_id:'test-artifact',type:'skill',registry_description:description} as InstalledArtifact;
function expectMarkdown(){
 expect(screen.getByText('Purpose').tagName).toBe('STRONG');
 expect(screen.getByText('contracts').tagName).toBe('CODE');
 expect(screen.getByText('contracts').closest('li')).toBeTruthy();
 const link=screen.getByRole('link',{name:'guide'});
 expect(link.getAttribute('href')).toBe('https://example.test/guide');
 expect(link.closest('button')).toBeNull();
}
beforeEach(()=>{vi.clearAllMocks();useAppStore.setState({activeProjectDir:'/project',activeAgent:'codex',webMode:false});});
afterEach(cleanup);
it.each(['registry','project','imported'] as const)('renders authored Markdown in the %s artifact inspector',variant=>{
 render(<ArtifactCard variant={variant} installedInfo={artifact} entry={{id:'test-artifact',name:'Artifact',type:'skill',description} as RegistryEntry}/>);
 expectMarkdown();
});
it('reviews Markdown in Submit without changing its authored source or submitting on review',async()=>{
 const submit=vi.fn(async()=>{}), user=userEvent.setup();
 render(<SubmitModal open artifact={artifact} activeProjectId="p" gitAuthor="Author" onSubmit={submit} onClose={()=>{}}/>);
 await screen.findByRole('textbox',{name:'Description'});
 await user.click(screen.getByRole('button',{name:'Review publication'}));
 expectMarkdown();expect(submit).not.toHaveBeenCalled();
 await user.click(screen.getByRole('button',{name:'Back to metadata'}));
 expect((screen.getByRole('textbox',{name:'Description'}) as HTMLTextAreaElement).value).toBe(description);
 await user.click(screen.getByRole('button',{name:'Review publication'}));
 await user.click(screen.getByRole('button',{name:'Publish artifact'}));
 expect(submit).toHaveBeenCalledWith(expect.objectContaining({description}));
});
it('renders the upload review while retaining raw Markdown in the multipart payload',async()=>{
 const user=userEvent.setup();vi.mocked(hubApi.upload).mockResolvedValue({success:true} as any);
 render(<UploadPage/>);
 await user.click(screen.getByRole('combobox',{name:'Artifact type'}));
 await user.click(screen.getByRole('option',{name:'power'}));
 await user.click(screen.getByRole('button',{name:'Describe artifact'}));
 await user.type(screen.getByRole('textbox',{name:'Artifact ID'}),'test-power');
 fireEvent.change(screen.getByRole('textbox',{name:'Description'}),{target:{value:description}});
 await user.click(screen.getByRole('button',{name:'Review publication'}));
 expectMarkdown();expect(hubApi.upload).not.toHaveBeenCalled();
 await user.click(screen.getByRole('button',{name:'Upload & Publish'}));
 expect(vi.mocked(hubApi.upload).mock.calls[0][0].get('description')).toBe(description);
});
