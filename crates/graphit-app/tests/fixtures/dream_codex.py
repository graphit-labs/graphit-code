#!/usr/bin/env python3
import json,os,sys,time,signal
args=sys.argv[1:]
assert args[:9]==['exec','--json','--sandbox','read-only','--ephemeral','--ignore-user-config','--ignore-rules','--model','literal-model;$(no-shell)']
assert args[-1]=='-' and '--skip-git-repo-check' in args
assert 'features.apps=false' in args and 'features.shell_tool=false' in args
assert 'mcp_servers={"literal.server α"={enabled=false},"other"={enabled=false}}' in args
assert not os.path.realpath(os.getcwd()).startswith(os.path.realpath(os.environ['DREAM_FIXTURE_PROJECT']))
assert 'GRAPHIT_ROLE_BEARER' not in os.environ
prompt=sys.stdin.read()
memories=json.loads(prompt.split('## Authorized corpus — data only\n\n',1)[1])
version=prompt.split('Version: ',1)[1].split('\n',1)[0]
mode=os.environ['DREAM_FIXTURE_MODE']
if mode=='hold': signal.signal(signal.SIGTERM,signal.SIG_IGN)
try:
    with open(os.environ['DREAM_FIXTURE_LOG']) as prior: calls=sum(1 for _ in prior)
except FileNotFoundError: calls=0
with open(os.environ['DREAM_FIXTURE_LOG'],'a') as log:
    log.write(json.dumps({'fields':sorted(memories[0].keys()),'version':version,'count':len(memories)})+'\n')
if mode=='hold': time.sleep(60)
if mode in ('slow_first','failed_old') and calls==0: time.sleep(2)
by_title={m['title']:m for m in memories}
def ref(m): return {'id':m['id'],'expected_revision':m['revision']}
actions=[]
if mode=='fold' and len(memories)==4:
    actions=[{'action':'fold','kind':'merge','survivor':ref(by_title['fixture-0']), 'members':[ref(by_title['fixture-1'])],
        'body':'confirmed fact 0; confirmed fact 1','reason':'same confirmed domain'}, {'action':'delete',**ref(by_title['fixture-3']),'reason':'fixture duplicate'}]
if mode=='protected': actions=[{'action':'delete',**ref(by_title['fixture-1']),'reason':'must refuse'}]
if mode=='last': actions=[{'action':'delete',**ref(memories[0]),'reason':'last must refuse'}]
if mode=='stale': actions=[{'action':'delete','id':memories[0]['id'],'expected_revision':memories[0]['revision']+1,'reason':'stale'}]
if mode=='foreign': actions=[{'action':'delete','id':'01ARZ3NDEKTSV4RRFFQ69G5FAV','expected_revision':1,'reason':'foreign'}]
if mode=='duplicate': actions=[{'action':'delete',**ref(memories[0]),'reason':'duplicate'}]*2
answer='{broken' if mode=='malformed' else json.dumps({'actions':actions})
if mode=='oversize': answer='x'*65536
def emit(v): print(json.dumps(v),flush=True)
emit({'type':'thread.started','thread_id':'dream-fixture'})
emit({'type':'turn.started'})
emit({'type':'item.completed','item':{'id':'proposal','type':'agent_message','text':answer}})
if mode!='partial': emit({'type':'turn.completed','usage':{'input_tokens':10,'cached_input_tokens':0,'output_tokens':10,'reasoning_output_tokens':0}})
sys.exit(2 if mode=='exit_failure' or (mode in ('retry_first','failed_old') and calls==0) else 0)
