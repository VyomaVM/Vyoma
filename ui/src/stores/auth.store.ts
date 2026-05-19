import { create } from 'zustand';

interface AuthState {
  token: string | null;
  setToken: (token: string) => void;
  clearToken: () => void;
}

// Initial token extraction from meta tag
const getTokenFromMeta = () => {
  const meta = document.querySelector('meta[name="vyoma-api-token"]');
  return meta?.getAttribute('content') || null;
};

export const useAuthStore = create<AuthState>((set) => ({
  token: getTokenFromMeta(),
  setToken: (token) => set({ token }),
  clearToken: () => set({ token: null }),
}));
