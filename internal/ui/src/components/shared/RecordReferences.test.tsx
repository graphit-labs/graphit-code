import '@testing-library/jest-dom/vitest';
import { beforeEach, afterEach, expect, it, vi } from 'vitest';
import { render, screen, cleanup } from '@testing-library/react';
import { MemoryRouter } from 'react-router-dom';
import { api } from '@/api/client';
import { useAppStore } from '@/store/appStore';
import { RecordReferences } from './RecordReferences';
vi.mock('@/api/client', () => ({ api: { get: vi.fn() } }));
beforeEach(() => { vi.resetAllMocks(); useAppStore.setState({ activeProjectDir: '/p' }); });
afterEach(cleanup);
it('renders persisted links and backlinks, keeping unresolved identities noninteractive', async () => {
  const source = {kind: 'task', id: 't', title: 'Delivery', href: '/task/explorer/t'};
  vi.mocked(api.get).mockResolvedValue({complete: true, warnings: [], outgoing: [
    {source, target: {kind: 'memory', id: 'm', title: 'Decision', href: '/memory/explorer/project/m'}, relation: 'supports', fields: []},
    {source, target: {kind: 'knowledge', id: 'missing', title: 'missing'}, relation: 'implements', fields: []}
  ], incoming: [{source: {kind: 'session', id: 's', title: 'Request', href: '/task/sessions/s'}, target: source, relation: 'relates_to', fields: []}]});
  render(<MemoryRouter><RecordReferences kind="task" id="t" /></MemoryRouter>);
  expect(await screen.findByRole('link', {name: /Decision/})).toHaveAttribute('href', '/memory/explorer/project/m');
  expect(screen.getByRole('link', {name: /Request/})).toHaveAttribute('href', '/task/sessions/s');
  expect(screen.getByText(/Target unavailable in this context/)).toBeTruthy();
  expect(screen.queryByRole('link', {name: /missing/})).toBeNull();
  expect(api.get).toHaveBeenCalledWith(expect.stringContaining('kind=task&id=t'));
});
