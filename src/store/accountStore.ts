import { create } from "zustand";
import type { RegState, SipAccount } from "@/types";

interface AccountState {
  account: SipAccount | null;
  regState: RegState;
  regError: string | null;
  setAccount: (account: SipAccount | null) => void;
  setRegState: (state: RegState, error?: string | null) => void;
}

export const useAccountStore = create<AccountState>((set) => ({
  account: null,
  regState: "none",
  regError: null,
  setAccount: (account) => set({ account }),
  setRegState: (regState, error = null) =>
    set({ regState, regError: error }),
}));
