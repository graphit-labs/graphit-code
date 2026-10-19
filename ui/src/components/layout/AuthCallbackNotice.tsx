import { useEffect, useState } from 'react';
import { useLocation, useNavigate } from 'react-router-dom';
import { WorkNotice } from '@/components/shared/EngineeringUI';

const loginErrors = {
  login_expired: {
    title: 'Login expirado',
    detail: 'O tempo para concluir o login terminou.',
  },
  login_invalid: {
    title: 'Não foi possível confirmar o login',
    detail: 'A resposta de autenticação não pôde ser validada.',
  },
  broker_unavailable: {
    title: 'Broker indisponível',
    detail: 'Não foi possível consultar o Broker.',
  },
  token_exchange: {
    title: 'Não foi possível concluir o login',
    detail: 'A troca de credenciais com o Broker falhou.',
  },
  session_unavailable: {
    title: 'Não foi possível iniciar sua sessão',
    detail: 'O login foi confirmado, mas a sessão não pôde ser criada.',
  },
} as const;

type LoginError = keyof typeof loginErrors;
type Notice = { code: LoginError; reference: string };

function readNotice(hash: string): Notice | null {
  const params = new URLSearchParams(hash.replace(/^#/, ''));
  const code = params.get('login_error');
  if (!code || !Object.prototype.hasOwnProperty.call(loginErrors, code)) return null;
  const rawReference = params.get('reference') || '';
  return { code: code as LoginError, reference: /^[A-Za-z0-9_-]{16}$/.test(rawReference) ? rawReference : '' };
}

export function AuthCallbackNotice() {
  const location = useLocation();
  const navigate = useNavigate();
  const [notice] = useState<Notice | null>(() => readNotice(location.hash));

  useEffect(() => {
    const params = new URLSearchParams(location.hash.replace(/^#/, ''));
    if (!params.has('login_error')) return;
    void navigate({ pathname: location.pathname, search: location.search, hash: '' }, { replace: true });
  }, [location.hash, location.pathname, location.search, navigate]);

  if (!notice) return null;
  const error = loginErrors[notice.code];
  return <div className="auth-callback-banner">
    <WorkNotice title={error.title} tone="error">
      <p>{error.detail} Use Entrar no cabeçalho para tentar novamente.</p>
      {notice.reference && <p className="mt-2 text-muted-foreground">Referência para suporte: <code className="font-mono">{notice.reference}</code></p>}
    </WorkNotice>
  </div>;
}
