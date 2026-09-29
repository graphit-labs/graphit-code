import { render, screen, waitFor } from '@testing-library/react';
import { MemoryRouter, useLocation } from 'react-router-dom';
import { describe, expect, it } from 'vitest';
import { AuthCallbackNotice } from './AuthCallbackNotice';

function CurrentHash() {
  return <output data-testid="hash">{useLocation().hash}</output>;
}

function renderNotice(hash: string) {
  return render(<MemoryRouter initialEntries={[`/workspace${hash}`]}>
    <AuthCallbackNotice />
    <CurrentHash />
  </MemoryRouter>);
}

describe('Broker login callback notice', () => {
  it('shows the safe error and diagnostic reference, then removes the fragment', async () => {
    renderNotice('#login_error=token_exchange&reference=Abcdef012345_-XY');
    const alert = await screen.findByRole('alert');
    expect(alert.textContent).toContain('Não foi possível concluir o login');
    expect(alert.textContent).toContain('Use Entrar no cabeçalho para tentar novamente.');
    expect(alert.textContent).toContain('Abcdef012345_-XY');
    await waitFor(() => expect(screen.getByTestId('hash').textContent).toBe(''));
  });

  it('keeps an error message but discards a malformed reference', async () => {
    renderNotice('#login_error=login_invalid&reference=private-value');
    const alert = await screen.findByRole('alert');
    expect(alert.textContent).toContain('A resposta de autenticação não pôde ser validada.');
    expect(alert.textContent).not.toContain('private-value');
    await waitFor(() => expect(screen.getByTestId('hash').textContent).toBe(''));
  });

  it('does not show unknown error text', async () => {
    renderNotice('#login_error=private-error');
    await waitFor(() => expect(screen.getByTestId('hash').textContent).toBe(''));
    expect(screen.queryByRole('alert')).toBeNull();
    expect(screen.queryByText('private-error')).toBeNull();
  });
});
