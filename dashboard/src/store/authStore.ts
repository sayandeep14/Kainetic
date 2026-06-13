import { create } from 'zustand';
import { persist } from 'zustand/middleware';

interface AuthState {
  token: string | null;
  teamId: string | null;
  login: (token: string, teamId: string) => void;
  logout: () => void;
  isAuthenticated: () => boolean;
}

export const useAuthStore = create<AuthState>()(
  persist(
    (set, get) => ({
      token: null,
      teamId: null,
      login: (token, teamId) => {
        localStorage.setItem('kc_access_token', token);
        set({ token, teamId });
      },
      logout: () => {
        localStorage.removeItem('kc_access_token');
        set({ token: null, teamId: null });
      },
      isAuthenticated: () => get().token !== null,
    }),
    { name: 'kc-auth' },
  ),
);
