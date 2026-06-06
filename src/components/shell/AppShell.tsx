import { useState, useCallback, useEffect } from "react";
import { TitleBar } from "./TitleBar";
import { StatusBar } from "./StatusBar";
import { BottomNav } from "./BottomNav";
import { useUiStore } from "@/store/uiStore";
import { isMobile } from "@/hooks/usePlatform";
import { useCallStore } from "@/store/callStore";
import { DialpadView } from "@/components/dialpad/DialpadView";
import { SettingsView } from "@/components/settings/SettingsView";
import { RecentCallsList } from "@/components/recent/RecentCallsList";
import { ChatView } from "@/components/chat/ChatView";
import { PeopleView } from "@/components/people/PeopleView";
import { FilesView } from "@/components/files/FilesView";
import { AdminView } from "@/components/admin/AdminView";
import { ActiveCallView } from "@/components/call/ActiveCallView";
import { IncomingCallOverlay } from "@/components/call/IncomingCallOverlay";
import { CommandPalette } from "@/components/shared/CommandPalette";
import { SetupWizard } from "@/components/auth/SetupWizard";
import { ToastContainer } from "@/components/ui/Toast";
import { useKeyboardShortcuts } from "@/hooks/useKeyboardShortcuts";
import { useServerEvents } from "@/hooks/useServerEvents";
import { useServerStore } from "@/store/serverStore";
import { getConfig } from "@/lib/tauri";

const views = {
  dialpad: DialpadView,
  chat: ChatView,
  people: PeopleView,
  files: FilesView,
  recent: RecentCallsList,
  admin: AdminView,
  settings: SettingsView,
} as const;

export function AppShell() {
  const activeTab = useUiStore((s) => s.activeTab);
  const activeCallId = useCallStore((s) => s.activeCallId);
  const hasActiveCall = activeCallId !== null;

  const [cmdPaletteOpen, setCmdPaletteOpen] = useState(false);
  const [showWizard, setShowWizard] = useState(false);
  const [wizardChecked, setWizardChecked] = useState(false);
  const mobile = isMobile();

  const openCommandPalette = useCallback(() => setCmdPaletteOpen(true), []);
  const closeCommandPalette = useCallback(() => setCmdPaletteOpen(false), []);

  useKeyboardShortcuts({ onOpenCommandPalette: openCommandPalette });

  // Connect to pale-server SSE for real-time presence & message events
  const serverBaseUrl = useServerStore((s) => s.baseUrl);
  const serverToken = useServerStore((s) => s.token);
  const setServerConnection = useServerStore((s) => s.setConnection);
  useServerEvents(serverBaseUrl, serverToken);

  // Check if this is first run (no account configured) + auto-connect to server
  useEffect(() => {
    getConfig()
      .then((config) => {
        if (!config.account && !config.matrix?.homeserver) {
          setShowWizard(true);
        }

        // Auto-reconnect to pale-server if configured and token is in session
        if (config.server?.url && config.server.auto_connect) {
          const token = sessionStorage.getItem("pale.admin.token");
          if (token) {
            setServerConnection(config.server.url, token);
          }
        }

        setWizardChecked(true);
      })
      .catch(() => {
        setShowWizard(true);
        setWizardChecked(true);
      });
  }, []);

  if (!wizardChecked) return null;

  if (showWizard) {
    return (
      <div className="flex flex-col h-screen w-screen overflow-hidden">
        {!mobile && <TitleBar />}
        <main className="flex-1 overflow-y-auto">
          <SetupWizard onComplete={() => setShowWizard(false)} />
        </main>
        <ToastContainer />
      </div>
    );
  }

  const View = views[activeTab];

  return (
    <div className="flex flex-col h-screen w-screen overflow-hidden">
      {!mobile && <TitleBar />}
      <StatusBar />

      <main className="flex-1 overflow-y-auto relative">
        {hasActiveCall ? <ActiveCallView /> : <View />}
      </main>

      {!hasActiveCall && <BottomNav />}

      {/* Overlays */}
      <IncomingCallOverlay />
      <CommandPalette open={cmdPaletteOpen} onClose={closeCommandPalette} />
      <ToastContainer />
    </div>
  );
}
