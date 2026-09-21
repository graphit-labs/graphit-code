import {expect,it,vi} from 'vitest';
import {api} from './client';
import {astApi} from './ast';
vi.mock('./client',()=>({api:{get:vi.fn().mockResolvedValue({nodes:[],links:[]}),delete:vi.fn().mockResolvedValue({status:'deleted',context:'shared/code'})}}));
it('unlinks the encoded AST context from the explicitly selected project',async()=>{
 await astApi.deleteContext('shared/code','/workspace/other project');
 expect(api.delete).toHaveBeenCalledWith('/api/context/shared%2Fcode?project_dir=%2Fworkspace%2Fother+project');
});

it('forwards neighborhood cancellation to HTTP without adding the signal to the query string', async () => {
 const controller = new AbortController();
 await astApi.getGraph({ context:'shared/code', project_dir:'/example', cypher_query:'MATCH (n) RETURN n', signal:controller.signal });
 expect(api.get).toHaveBeenCalledWith('/api/graph?context=shared%2Fcode&cypher_query=MATCH+%28n%29+RETURN+n&project_dir=%2Fexample', {signal:controller.signal});
});
