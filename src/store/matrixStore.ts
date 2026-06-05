import { create } from "zustand";

export type MatrixAuthState = "logged_out" | "logging_in" | "logged_in" | "sync_error";

interface MatrixStoreState {
  authState: MatrixAuthState;
  userId: string | null;
  displayName: string | null;
  setAuthState: (state: MatrixAuthState, userId?: string | null, displayName?: string | null) => void;
}

export const useMatrixStore = create<MatrixStoreState>((set) => ({
  authState: "logged_out",
  userId: null,
  displayName: null,
  setAuthState: (authState, userId = null, displayName = null) =>
    set({ authState, userId, displayName }),
}));
