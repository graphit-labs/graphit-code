import '@/test/contextControls';
import { cleanup, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { ArtifactCard } from './ArtifactCard';
import { SubmitModal } from './modals/SubmitModal';
import { type InstalledArtifact } from '@/api/hub';
import { useAppStore } from '@/store/appStore';
vi.mock('@/hooks/useTheme',()=>({useTheme:()=>({theme:'light'})}));
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
it.each(['catalog','installed'] as const)('renders authored Markdown in the %s artifact inspector',kind=>{
 const common={canAct:false,unavailableReason:'Read only',onClose:()=>{}};
 if(kind==='catalog') render(<ArtifactCard {...common} kind="catalog" source={{transport:'broker',provider:'corporate'}} producer="publisher" onInstall={()=>{}} canUnpublish={false} unpublishReason="Read only" onUnpublish={()=>{}} entry={{artifact_type:'skill',id:'test-artifact',name:'Artifact',description,tags:[],author:null,latest:null,versions:['release-1']}}/>);
 else render(<ArtifactCard {...common} kind="installed" scope={{kind:'project',project_id:'project',project_version:'feature/a'}} binding="codex" onAction={()=>{}} entry={{artifact_type:'skill',id:'test-artifact',version:'release-1',producer:'publisher',target:{kind:'shared_adapter',adapter:'codex'},source_origin:'HubCache',origin:{transport:'broker',provider:'corporate'},alias:null,entry_name:null,display:{name:'Artifact',description,tags:[]}}}/>);
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
