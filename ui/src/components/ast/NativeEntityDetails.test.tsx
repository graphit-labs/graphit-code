import '@testing-library/jest-dom/vitest'
import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { expect, it, vi } from 'vitest'
import type { GraphNode } from '@/api/ast'
import { parseNativeJson } from '@/api/nativeJson'
import { NativeEntityDetails } from './NativeEntityDetails'

const node: GraphNode = {
  id:'renderer-only',name:'parseRequest',label:'Function',type:'Function',file:'src/request.rs',line:9007199254740993n,
  nativeIdentity:{table:7,row:'18446744073709551615'},
  nativeKey:{type:'string',value:'opaque [not JSON] $ literal'},
  properties:{uid:'public [opaque] identity',lang:'rust',grammar:'tree-sitter-rust',extent_status:'Exact',end_line:9007199254740994n},
}

it('summarizes observed fields and preserves opaque identities and complete exact technical data', async () => {
  const user = userEvent.setup()
  render(<NativeEntityDetails node={node} />)
  expect(screen.getByText('Source file').nextElementSibling?.textContent).toBe(node.file)
  expect(screen.getByText('Start line').nextElementSibling?.textContent).toBe('9007199254740993')
  const disclosure = screen.getByText('Technical entity details').closest('details')!
  expect(disclosure.open).toBe(false)
  await user.click(screen.getByText('Technical entity details'))
  expect(screen.getByText('Grammar').nextElementSibling?.textContent).toBe('tree-sitter-rust')
  const copy = vi.spyOn(navigator.clipboard,'writeText')
  await user.click(screen.getByRole('button',{name:'Copy primary key'}))
  expect(copy).toHaveBeenLastCalledWith(node.nativeKey!.value)
  await screen.findByText('Primary key copied.')
  await user.click(screen.getByRole('button',{name:'Copy UID'}))
  expect(copy).toHaveBeenLastCalledWith(node.properties!.uid)
  await user.click(screen.getByRole('button',{name:'Copy entity details'}))
  const copied = parseNativeJson(copy.mock.calls.at(-1)![0])
  expect(copied).toEqual({name:node.name,type:node.label,file:node.file,line:node.line,scope:null,uid:node.properties!.uid,primary_key:node.nativeKey,native_identity:node.nativeIdentity,properties:node.properties})
})

it('distinguishes an absent UID from an empty UID and reports clipboard errors without claiming success', async () => {
  const user = userEvent.setup()
  const view = render(<NativeEntityDetails node={{...node,properties:{}}} />)
  await user.click(screen.getByText('Technical entity details'))
  expect(screen.getByRole('button',{name:'Copy UID'})).toBeDisabled()
  view.rerender(<NativeEntityDetails node={{...node,properties:{uid:''}}} />)
  expect(screen.getByText('The recorded UID is an empty string.')).toBeTruthy()
  const copy = vi.spyOn(navigator.clipboard,'writeText').mockRejectedValueOnce(new Error('Clipboard denied'))
  await user.click(screen.getByRole('button',{name:'Copy UID'}))
  expect(copy).toHaveBeenCalledWith('')
  await screen.findByText('Clipboard denied')
  expect(screen.queryByRole('status')).toBeNull()
})

it('keeps a long opaque UID compact, copies it literally and exposes the complete value for manual selection', async () => {
  const user = userEvent.setup()
  const uid = '["opaque",{"symbol":"' + 'x'.repeat(100) + '"}]'
  render(<NativeEntityDetails node={{...node,properties:{...node.properties,uid}}} scope="Graphit / feature/identity" />)
  expect(screen.getByText('Entity name').nextElementSibling?.textContent).toBe('parseRequest')
  expect(screen.getByText('Scope').nextElementSibling?.textContent).toBe('Graphit / feature/identity')
  const compact = screen.getByTitle(uid)
  expect(compact.textContent!.length).toBeLessThan(70)
  const disclosure = screen.getByText('Technical entity details').closest('details')!
  expect(disclosure.open).toBe(false)
  const copy = vi.spyOn(navigator.clipboard,'writeText')
  await user.click(screen.getByRole('button',{name:'Copy UID'}))
  expect(copy).toHaveBeenLastCalledWith(uid)
  await screen.findByText('UID copied.')
  await user.click(screen.getByText('Technical entity details'))
  expect(screen.getByText('Complete UID').nextElementSibling?.textContent).toBe(uid)
  await user.click(screen.getByRole('button',{name:'Copy entity details'}))
  const details = parseNativeJson(copy.mock.calls.at(-1)![0]) as Record<string,unknown>
  expect(details.uid).toBe(uid)
  expect(details.scope).toBe('Graphit / feature/identity')
  expect(details).not.toHaveProperty('renderer_id')
})

it('keeps UID copying keyboard accessible outside the collapsed technical details', async () => {
 const user=userEvent.setup();render(<NativeEntityDetails node={node}/>);
 await user.tab();expect(screen.getByRole('button',{name:'Copy UID'})).toHaveFocus();
 const copy=vi.spyOn(navigator.clipboard,'writeText');
 await user.keyboard('{Enter}');expect(copy).toHaveBeenCalledWith(node.properties!.uid);
 expect(screen.getByRole('button',{name:'Copy UID'})).toHaveFocus();
 expect(screen.getByText('Technical entity details').closest('details')).not.toHaveAttribute('open');
});
