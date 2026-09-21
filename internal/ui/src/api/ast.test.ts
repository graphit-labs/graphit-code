import {expect,it,vi} from 'vitest';
import {api} from './client';
import {astApi} from './ast';
vi.mock('./client',()=>({api:{delete:vi.fn().mockResolvedValue({status:'deleted',context:'shared/code'})}}));
it('unlinks the encoded AST context from the explicitly selected project',async()=>{
 await astApi.deleteContext('shared/code','/workspace/other project');
 expect(api.delete).toHaveBeenCalledWith('/api/context/shared%2Fcode?project_dir=%2Fworkspace%2Fother+project');
});
