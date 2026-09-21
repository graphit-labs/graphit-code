import { afterEach, expect, it, vi } from 'vitest';
import { cleanup, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { LiveAnswer } from './LiveAnswer';
import { transcriptFromEvents } from './LiveSearchPage';
vi.mock('@/hooks/useTheme', () => ({ useTheme: () => ({ theme: 'light' }) }));
afterEach(cleanup);

it('renders separate streamed messages as paragraphs without splitting Markdown deltas', async () => {
  const onLink = vi.fn(), user = userEvent.setup();
  const chunks = ['Vou consultar as fontes.', '\n\nEstou usando **Know', 'ledge** e [[context:Guide]].'];
  const events = chunks.map((text, i) => ({ seq: i + 1, kind: 'text' as const, text, at: '' }));
  const { rerender } = render(<LiveAnswer content={transcriptFromEvents(events.slice(0, 1))[0].answer} onLink={onLink} />);
  rerender(<LiveAnswer content={transcriptFromEvents(events)[0].answer} onLink={onLink} />);
  const first = screen.getByText('Vou consultar as fontes.');
  const second = screen.getByText('Knowledge').closest('p');
  expect(first.tagName).toBe('P');
  expect(second).not.toBe(first);
  expect(second?.textContent).toBe('Estou usando Knowledge e Guide.');
  expect(screen.getByText('Knowledge').tagName).toBe('STRONG');
  await user.click(screen.getByRole('button', { name: 'Guide' }));
  expect(onLink).toHaveBeenCalledWith('context:Guide');
});

it('shortens qualified citation labels but sends their full identity', async () => {
  const onLink = vi.fn(), user = userEvent.setup();
  render(<LiveAnswer onLink={onLink} content={'**Answer** [[graphit-code-429b:AI_Engine_Specification]] [Read the contract](wiki://beta%3ARules) [[ADR:_Decision]]'} />);
  expect(screen.getByText('Answer').tagName).toBe('STRONG');
  await user.click(screen.getByRole('button', { name: 'AI Engine Specification' }));
  expect(onLink).toHaveBeenLastCalledWith('graphit-code-429b:AI_Engine_Specification');
  expect(screen.getByRole('button', { name: 'AI Engine Specification' }).title).toBe('graphit-code-429b:AI_Engine_Specification');
  await user.click(screen.getByRole('button', { name: 'Read the contract' }));
  expect(onLink).toHaveBeenLastCalledWith('beta:Rules');
  expect(screen.getByRole('button', { name: 'ADR: Decision' })).toBeTruthy();
  expect(screen.queryByRole('dialog')).toBeNull();
});
