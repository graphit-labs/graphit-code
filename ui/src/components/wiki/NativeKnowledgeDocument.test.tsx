import { render, screen, within } from '@testing-library/react'
import '@testing-library/jest-dom/vitest'
import userEvent from '@testing-library/user-event'
import { expect, it, vi } from 'vitest'
import type { NativeKnowledgePage } from '@/api/nativeKnowledge'
import { NativeKnowledgeDocument } from './NativeKnowledgeDocument'

vi.mock('@/hooks/useTheme', () => ({ useTheme: () => ({ theme: 'light' }) }))

function publication(body: string, title = 'Guia 日本語'): Extract<NativeKnowledgePage, {state:'published'}> {
  return {state:'published',generation:{corpus_uri:'fixture-corpus',version:1n,physical_branch:null,inherited_target:null},page:{state:'found',document:{
    slug:'guide',title,body,kind:'guide',summary:'A guide',summary_truncated:false,description:null,source_path:'docs/guide.md',physical_digest:null,compilation_digest:Array(32).fill(7),recipe:'authored-document-v1',recipe_parameters:{fallback_title:'guide',kind_on_absent:'guide',max_summary_bytes:1024n},updated_at:null,stale_since:null,stale_after:null,resolved_wiki_targets:[],
  }}}
}

it('shows one title and keeps its focus while Raw, Copy and the complete publication retain the literal document', async () => {
  const user = userEvent.setup()
  const body = '---\r\ntitle: Guia 日本語\r\n---\r\n\r\n# Guia 日本語\r\n\r\n## Section\r\n\r\nNative **body**.\r\n'
  const native = publication(body)
  render(<NativeKnowledgeDocument publication={native} openPage={vi.fn()} />)
  const headings = screen.getAllByRole('heading', {level:1,name:'Guia 日本語'})
  expect(headings).toHaveLength(1)
  expect(headings[0]).toHaveFocus()
  expect(headings[0]).toHaveAttribute('tabindex','-1')
  expect(screen.getByRole('heading', {name:'Section'})).toBeTruthy()
  const bodySection = screen.getByRole('heading', {name:'Document body'}).closest('section')!
  expect(bodySection.textContent).not.toContain('title: Guia 日本語')
  await user.click(screen.getByRole('button', {name:'Raw body'}))
  expect(bodySection.querySelector('pre')!.textContent).toBe(body)
  const copy = vi.spyOn(navigator.clipboard,'writeText')
  await user.click(screen.getByRole('button', {name:'Copy Markdown'}))
  expect(copy).toHaveBeenCalledWith(body)
  const complete = screen.getByText('Complete publication document').closest('details')!
  expect(complete.querySelector('pre')!.textContent).toContain('title: Guia 日本語\\r\\n')
  await user.click(screen.getByRole('button', {name:'Rendered document'}))
  expect(within(bodySection).queryByRole('heading', {level:1})).toBeNull()
})

it.each([
  ['Body without a title.', 'Guia 日本語', null],
  ['# A different authored title\n\nBody.', 'Guia 日本語', 'A different authored title'],
  ['Body without a title.', '', null],
])('retains the metadata title and focus when the body has no duplicate H1: %s', (body,title,authoredTitle) => {
  render(<NativeKnowledgeDocument publication={publication(body,title)} openPage={vi.fn()} />)
  const metadata = screen.getByRole('heading', {level:1,name:title || 'guide'})
  expect(metadata).toHaveFocus()
  if (authoredTitle) expect(screen.getByRole('heading', {level:1,name:authoredTitle})).toBeTruthy()
})
