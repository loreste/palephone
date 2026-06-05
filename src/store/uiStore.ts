import { create } from "zustand";
import type { Tab, Theme } from "@/types";

interface UiState {
  activeTab: Tab;
  theme: Theme;
  setActiveTab: (tab: Tab) => void;
  setTheme: (theme: Theme) => void;
  toggleTheme: () => void;
}

export const useUiStore = create<UiState>((set) => ({
  activeTab: "dialpad",
  theme: "dark",
  setActiveTab: (tab) => set({ activeTab: tab }),
  setTheme: (theme) => set({ theme }),
  toggleTheme: () =>
    set((state) => ({ theme: state.theme === "dark" ? "light" : "dark" })),
}));
