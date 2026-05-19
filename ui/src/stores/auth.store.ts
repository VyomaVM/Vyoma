import { create } from 'zustand';

interface AuthState {
  token: string | null;
  setToken: (token: string) => void;
  clearToken: () => void;
}

// Initial token extraction from meta tag
const getTokenFromMeta = () => {
  const meta = document.querySelector('meta[name="vyoma-api-token"]');
  const token = meta?.getAttribute('content') || null;
  if (token) {
    document.cookie = `vyoma_token=${token}; path=/; SameSite=Lax`;
  }
  return token;
};

export const useAuthStore = create<AuthState>((set) => ({
  token: getTokenFromMeta(),
  setToken: (token) => {
    document.cookie = `vyoma_token=${token}; path=/; SameSite=Lax`;
    set({ token });
  },
  clearToken: () => {
    document.cookie = `vyoma_token=; path=/; expires=Thu, 01 Jan 1970 00:00:00 GMT`;
    set({ token: null });
  },
}));
