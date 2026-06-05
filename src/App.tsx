import { useEffect } from "react";
import { AppShell } from "@/components/shell/AppShell";
import { useUiStore } from "@/store/uiStore";
import { useSipEvents } from "@/hooks/useSipEvents";
import { useRingtone } from "@/hooks/useRingtone";
import { useConfigLoader } from "@/hooks/useConfigLoader";
import { useMatrixEvents } from "@/hooks/useMatrixEvents";

export default function App() {
  const theme = useUiStore((s) => s.theme);

  // Load persisted config on startup (account, theme, etc.)
  useConfigLoader();
  // Mount SIP event listeners — bridges Rust backend events to Zustand stores
  useSipEvents();
  // Mount Matrix event listeners — chat, rooms, auth state
  useMatrixEvents();
  // Play ringtone on incoming calls
  useRingtone();

  useEffect(() => {
    const root = document.documentElement;
    root.classList.remove("dark", "light");
    root.classList.add(theme);
  }, [theme]);

  return <AppShell />;
}
