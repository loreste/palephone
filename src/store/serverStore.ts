import { create } from "zustand";

interface ServerStoreState {
  baseUrl: string | null;
  token: string | null;
  tokenExpiresAt: string | null;
  connected: boolean;

  setConnection: (baseUrl: string, token: string, expiresAt?: string | null) => void;
  updateToken: (token: string, expiresAt: string) => void;
  disconnect: () => void;
}

export const useServerStore = create<ServerStoreState>((set) => ({
  baseUrl: null,
  token: null,
  tokenExpiresAt: null,
  connected: false,

  setConnection: (baseUrl, token, expiresAt = null) =>
    set({ baseUrl, token, tokenExpiresAt: expiresAt, connected: true }),

  updateToken: (token, expiresAt) =>
    set({ token, tokenExpiresAt: expiresAt }),

  disconnect: () =>
    set({ baseUrl: null, token: null, tokenExpiresAt: null, connected: false }),
}));
