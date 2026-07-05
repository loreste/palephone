import { create } from "zustand";

interface ServerStoreState {
  baseUrl: string | null;
  token: string | null;
  tokenExpiresAt: string | null;
  connected: boolean;
  userRole: string | null; // "admin" or "user"
  userDisplayName: string | null;

  setConnection: (baseUrl: string, token: string, expiresAt?: string | null, role?: string | null, displayName?: string | null) => void;
  setIdentity: (role?: string | null, displayName?: string | null) => void;
  updateToken: (token: string, expiresAt: string) => void;
  disconnect: () => void;
  isAdmin: () => boolean;
}

export const useServerStore = create<ServerStoreState>((set, get) => ({
  baseUrl: null,
  token: null,
  tokenExpiresAt: null,
  connected: false,
  userRole: null,
  userDisplayName: null,

  setConnection: (baseUrl, token, expiresAt = null, role, displayName) =>
    set((state) => ({
      baseUrl,
      token,
      tokenExpiresAt: expiresAt,
      connected: true,
      userRole: role ?? state.userRole,
      userDisplayName: displayName ?? state.userDisplayName,
    })),

  setIdentity: (role = null, displayName = null) =>
    set({ userRole: role, userDisplayName: displayName }),

  updateToken: (token, expiresAt) =>
    set({ token, tokenExpiresAt: expiresAt }),

  disconnect: () =>
    set({ baseUrl: null, token: null, tokenExpiresAt: null, connected: false, userRole: null, userDisplayName: null }),

  isAdmin: () => get().userRole === "admin",
}));
