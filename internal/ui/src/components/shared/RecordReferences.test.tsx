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

it('keeps both directions explicit when a complete record has no relationships', async () => {
  vi.mocked(api.get).mockResolvedValue({ complete: true, warnings: [], outgoing: [], incoming: [] });

  render(<MemoryRouter><RecordReferences kind="memory" id="m" /></MemoryRouter>);

  expect(await screen.findAllByText('No linked records.')).toHaveLength(2);
  expect(screen.getByRole('heading', { name: 'References' })).toBeTruthy();
  expect(screen.getByRole('heading', { name: 'Referenced by' })).toBeTruthy();
});

it('labels partial relationship results and their unavailable directions', async () => {
  vi.mocked(api.get).mockResolvedValue({
    complete: false,
    warnings: ['Memory references are unavailable.'],
    outgoing: [],
    incoming: [],
  });

  render(<MemoryRouter><RecordReferences kind="session" id="s" /></MemoryRouter>);

  expect(await screen.findByText('Partial reference view')).toBeTruthy();
  expect(screen.getByText('Memory references are unavailable.')).toBeTruthy();
  expect(screen.getAllByText('No links found in available sources.')).toHaveLength(2);
});

it('announces a failed relationship request without hiding the record section', async () => {
  vi.mocked(api.get).mockRejectedValue(new Error('reference store offline'));

  render(<MemoryRouter><RecordReferences kind="task" id="t" /></MemoryRouter>);

  const alert = await screen.findByRole('alert');
  expect(alert.textContent).toContain('References unavailable');
  expect(alert.textContent).toContain('reference store offline');
  expect(screen.getByRole('heading', { name: 'Record links' })).toBeTruthy();
});
