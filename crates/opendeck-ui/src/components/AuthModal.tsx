import React, { useEffect, useRef, useCallback } from 'react';

interface AuthModalProps {
  onLogin: (token: string) => void;
  onClose: () => void;
}

export default function AuthModal({ onLogin, onClose }: AuthModalProps) {
  const modalRef = useRef<HTMLDivElement>(null);
  const previousActiveElement = useRef<HTMLElement | null>(null);

  const handleLogin = useCallback((e: React.FormEvent<HTMLFormElement>) => {
    e.preventDefault();
    const formData = new FormData(e.currentTarget);
    const username = formData.get('username') as string;
    const password = formData.get('password') as string;
    if (!username || !password) return;
    // Placeholder: validação simples
    onLogin(`dev-token-${username}`);
  }, [onLogin]);

  useEffect(() => {
    // Focus trap: salvar elemento ativo anterior
    previousActiveElement.current = document.activeElement as HTMLElement;

    const firstFocusable = modalRef.current?.querySelector<HTMLElement>(
      'input, select, button, [href], [tabindex]:not([tabindex="-1"])'
    );
    firstFocusable?.focus();

    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        onClose();
        return;
      }

      if (e.key === 'Tab') {
        const focusableElements = modalRef.current?.querySelectorAll<HTMLElement>(
          'input, select, button, [href], [tabindex]:not([tabindex="-1"])'
        );
        if (!focusableElements || focusableElements.length === 0) return;

        const first = focusableElements[0];
        const last = focusableElements[focusableElements.length - 1];

        if (e.shiftKey && document.activeElement === first) {
          e.preventDefault();
          last.focus();
        } else if (!e.shiftKey && document.activeElement === last) {
          e.preventDefault();
          first.focus();
        }
      }
    };

    document.addEventListener('keydown', handleKeyDown);
    document.body.style.overflow = 'hidden';

    return () => {
      document.removeEventListener('keydown', handleKeyDown);
      document.body.style.overflow = '';
      previousActiveElement.current?.focus();
    };
  }, [onClose]);

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-sm"
      onClick={(e) => {
        if (e.target === e.currentTarget) onClose();
      }}
    >
      <div
        ref={modalRef}
        className="bg-[#1a1a1a] border border-[#333] rounded-lg p-6 w-96 shadow-2xl"
        role="dialog"
        aria-modal="true"
      >
        <h2 className="text-lg font-bold text-white mb-4">Entrar no OpenDeck</h2>
        <form onSubmit={handleLogin} className="space-y-3">
          <div>
            <label className="block text-sm text-[#ccc] mb-1">Usuário</label>
            <input
              name="username"
              type="text"
              required
              className="w-full bg-[#2a2a2a] border border-[#444] rounded px-3 py-2 text-white focus:border-[#3b82f6] focus:outline-none transition-colors"
              placeholder="usuário"
            />
          </div>
          <div>
            <label className="block text-sm text-[#ccc] mb-1">Senha</label>
            <input
              name="password"
              type="password"
              required
              className="w-full bg-[#2a2a2a] border border-[#444] rounded px-3 py-2 text-white focus:border-[#3b82f6] focus:outline-none transition-colors"
              placeholder="••••••••"
            />
          </div>
          <button
            type="submit"
            className="w-full bg-[#3b82f6] hover:bg-[#2563eb] text-white py-2 rounded font-medium transition-colors"
          >
            Entrar
          </button>
        </form>
        <p className="text-[11px] text-[#666] mt-3 text-center">
          Modo de desenvolvimento — qualquer usuário/senha é aceito.
        </p>
      </div>
    </div>
  );
}
