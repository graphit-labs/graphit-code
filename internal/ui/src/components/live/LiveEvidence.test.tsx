import { useState } from 'react';
import { afterEach, expect, it, vi } from 'vitest';
import { act, cleanup, render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { api } from '@/api/client';
import { LiveAnswer } from './LiveAnswer';
import { LiveEvidence } from './LiveEvidence';
vi.mock('@/api/client', () => ({ api: { get: vi.fn() } }));
vi.mock('@/hooks/useTheme', () => ({ useTheme: () => ({ theme: 'light' }) }));
afterEach(() => { cleanup(); vi.resetAllMocks(); });

function View({ sessionId = 'A', content = '**Answer** [[alpha:rules]] [[beta:rules]] [[alias:rules]]' }: { sessionId?: string; content?: string }) {
  const [value, setValue] = useState('output');
  return <LiveEvidence sessionId={sessionId} value={value} onChange={setValue}
    output={onLink => <><input aria-label="Saved draft" defaultValue="continue here" /><LiveAnswer content={content} onLink={onLink} /></>}
    activity={<p>Existing activity</p>} />;
}
const page = (context = 'alpha', path = 'rules', title = 'Rules', content = '**Source** [[next]]') => ({ context, path, title, content, artifact_id: `artifact-${context}`, version: '1.2' });
const output = () => screen.getByRole('tabpanel', { name: 'Agent output' });

it('opens nested sources in reusable tabs and preserves output, draft and activity', async () => {
  const user = userEvent.setup();
  vi.mocked(api.get).mockResolvedValueOnce(page()).mockResolvedValueOnce(page('alpha', 'next', 'Next', 'Resolved'));
  render(<View />);
  const originalOutput = output(); originalOutput.scrollTop = 83;
  await user.click(within(output()).getAllByRole('button', { name: 'rules' })[0]);
  expect(await screen.findByText('Source')).toBeTruthy();
  expect(screen.queryByRole('dialog')).toBeNull();
  expect(screen.getByRole('tab', { name: 'Rules' }).getAttribute('aria-selected')).toBe('true');
  expect(screen.getByText('artifact-alpha')).toBeTruthy();
  expect(screen.getByText('1.2')).toBeTruthy();
  await user.click(screen.getByRole('button', { name: 'next' }));
  await screen.findByText('Resolved');
  expect(api.get).toHaveBeenLastCalledWith(expect.stringContaining('A/knowledge/page?page=next&context=alpha'), expect.objectContaining({ signal: expect.any(AbortSignal) }));
  expect(screen.getAllByRole('tab')).toHaveLength(4);
  await user.click(screen.getByRole('tab', { name: 'Agent output' }));
  expect(output()).toBe(originalOutput); expect(output().scrollTop).toBe(83);
  expect((screen.getByLabelText('Saved draft') as HTMLInputElement).value).toBe('continue here');
  await user.click(within(output()).getAllByRole('button', { name: 'rules' })[0]);
  expect(api.get).toHaveBeenCalledTimes(2);
  await user.click(screen.getByRole('tab', { name: 'Execution activity' }));
  expect(screen.getByText('Existing activity')).toBeTruthy();
  await user.click(screen.getByRole('tab', { name: 'Next' }));
  await user.click(screen.getByRole('button', { name: 'Close tab Next' }));
  expect(screen.queryByRole('tab', { name: 'Next' })).toBeNull();
  expect(screen.getByRole('tab', { name: 'Agent output' }).getAttribute('aria-selected')).toBe('true');
});

it('keeps different contexts separate and merges resolved aliases', async () => {
  const user = userEvent.setup();
  vi.mocked(api.get).mockResolvedValueOnce(page()).mockResolvedValueOnce(page('beta')).mockResolvedValueOnce(page());
  render(<View />);
  for (const index of [0, 1, 2]) {
    await user.click(within(output()).getAllByRole('button', { name: 'rules' })[index]);
    await waitFor(() => expect(screen.queryByRole('status')).toBeNull());
    await user.click(screen.getByRole('tab', { name: 'Agent output' }));
  }
  expect(screen.getAllByRole('tab')).toHaveLength(4);
  expect(screen.getByRole('tab', { name: 'Rules · alpha' })).toBeTruthy();
  expect(screen.getByRole('tab', { name: 'Rules · beta' })).toBeTruthy();
  await user.click(within(output()).getAllByRole('button', { name: 'rules' })[2]);
  expect(api.get).toHaveBeenCalledTimes(3);
  expect(screen.getByRole('tab', { name: 'Rules · alpha' }).getAttribute('aria-selected')).toBe('true');
});

it('offers context choice and retries source errors within the tab', async () => {
  const user = userEvent.setup();
  vi.mocked(api.get).mockResolvedValueOnce({ candidates: [page(), page('beta')] }).mockRejectedValueOnce(new Error('Source removed')).mockResolvedValueOnce(page('beta'));
  render(<View content="[[rules]]" />);
  await user.click(screen.getByRole('button', { name: 'rules' }));
  await user.click(await screen.findByRole('button', { name: 'Rules · beta' }));
  expect(await screen.findByText('Source removed')).toBeTruthy();
  await user.click(screen.getByRole('button', { name: 'Try again' }));
  await screen.findByText('Source');
  expect(api.get).toHaveBeenLastCalledWith(expect.stringContaining('page=rules&context=beta'), expect.anything());
  expect(screen.getAllByRole('tab')).toHaveLength(3);
});

it('aborts closed readers and ignores their late result', async () => {
  let resolve!: (value: unknown) => void;
  vi.mocked(api.get).mockImplementation(() => new Promise(r => { resolve = r; }));
  const user = userEvent.setup(); render(<View content="[[rules]]" />);
  await user.click(screen.getByRole('button', { name: 'rules' }));
  const signal = vi.mocked(api.get).mock.calls[0][1]?.signal;
  await user.click(screen.getByRole('button', { name: 'Close tab rules' }));
  expect(signal?.aborted).toBe(true);
  await act(async () => resolve(page()));
  expect(screen.getAllByRole('tab')).toHaveLength(2);
  expect(screen.queryByText('Source')).toBeNull();
});

it('clears readers when the investigation changes', async () => {
  let resolve!: (value: unknown) => void;
  vi.mocked(api.get).mockImplementation(() => new Promise(r => { resolve = r; }));
  const user = userEvent.setup(), view = render(<View content="[[rules]]" />);
  await user.click(screen.getByRole('button', { name: 'rules' }));
  const signal = vi.mocked(api.get).mock.calls[0][1]?.signal;
  view.rerender(<View sessionId="B" content="Different answer" />);
  expect(signal?.aborted).toBe(true);
  await act(async () => resolve(page()));
  expect(screen.getAllByRole('tab')).toHaveLength(2);
  expect(screen.getByText('Different answer')).toBeTruthy();
  expect(screen.queryByText('Source')).toBeNull();
});

it('keeps the latest selection when different source requests finish out of order', async () => {
  const resolve: Array<(value: unknown) => void> = [];
  vi.mocked(api.get).mockImplementation(() => new Promise(r => resolve.push(r)));
  const user = userEvent.setup(); render(<View />);
  await user.click(within(output()).getAllByRole('button', { name: 'rules' })[0]);
  await user.click(screen.getByRole('tab', { name: 'Agent output' }));
  await user.click(within(output()).getAllByRole('button', { name: 'rules' })[1]);
  await act(async () => resolve[1](page('beta')));
  await act(async () => resolve[0](page()));
  expect(screen.getByRole('tab', { name: 'Rules · beta' }).getAttribute('aria-selected')).toBe('true');
  screen.getByRole('tab', { name: 'Rules · beta' }).focus();
  await user.keyboard('{Home}');
  expect(screen.getByRole('tab', { name: 'Agent output' }).getAttribute('aria-selected')).toBe('true');
  await user.keyboard('{End}');
  expect(screen.getByRole('tab', { name: 'Rules · beta' }).getAttribute('aria-selected')).toBe('true');
});


it('closes an inactive source directly from its tab without changing the active source', async () => {
  const user = userEvent.setup();
  vi.mocked(api.get).mockResolvedValueOnce(page()).mockResolvedValueOnce(page('beta'));
  const {container} = render(<View />);
  await user.click(within(output()).getAllByRole('button', {name:'rules'})[0]);
  await screen.findByText('Source');
  await user.click(screen.getByRole('tab',{name:'Agent output'}));
  await user.click(within(output()).getAllByRole('button', {name:'rules'})[1]);
  const active = await screen.findByRole('tab',{name:'Rules · beta'});
  const row = screen.getByRole('tablist', {name:'Run evidence'});
  expect(within(row).getAllByRole('button', {name:/^Close tab/})).toHaveLength(2);
  expect(container.querySelector('button button')).toBeNull();
  await user.click(within(row).getByRole('button',{name:'Close tab Rules · alpha'}));
  expect(screen.getAllByRole('tab')).toHaveLength(3);
  expect(active.getAttribute('aria-selected')).toBe('true');
  expect(document.activeElement).toBe(active);
  expect((screen.getByLabelText('Saved draft') as HTMLInputElement).value).toBe('continue here');
  await user.tab();
  expect(document.activeElement).toBe(screen.getByRole('button',{name:'Close tab Rules'}));
  await user.keyboard('{Enter}');
  expect(screen.getAllByRole('tab')).toHaveLength(2);
  expect(document.activeElement).toBe(screen.getByRole('tab',{name:'Agent output'}));
});
