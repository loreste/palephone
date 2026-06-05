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
import { FilesView } from "@/components/files/FilesView";
import { ActiveCallView } from "@/components/call/ActiveCallView";
import { IncomingCallOverlay } from "@/components/call/IncomingCallOverlay";
import { CommandPalette } from "@/components/shared/CommandPalette";
import { SetupWizard } from "@/components/auth/SetupWizard";
import { ToastContainer } from "@/components/ui/Toast";
import { useKeyboardShortcuts } from "@/hooks/useKeyboardShortcuts";
import { getConfig } from "@/lib/tauri";

const views = {
  dialpad: DialpadView,
  chat: ChatView,
  files: FilesView,
  recent: RecentCallsList,
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

  // Check if this is first run (no account configured)
  useEffect(() => {
    getConfig()
      .then((config) => {
        if (!config.account && !config.matrix?.homeserver) {
          setShowWizard(true);
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
