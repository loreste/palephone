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
import { SearchOverlay } from "@/components/shared/SearchOverlay";
import { SetupWizard } from "@/components/auth/SetupWizard";
import { ToastContainer } from "@/components/ui/Toast";
import { useKeyboardShortcuts } from "@/hooks/useKeyboardShortcuts";
import { useServerEvents } from "@/hooks/useServerEvents";
import { useServerStore } from "@/store/serverStore";
import { useAccountStore } from "@/store/accountStore";
import { getConfig, getSipPassword, paleLogin, registerAccount } from "@/lib/tauri";

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
  const [searchOpen, setSearchOpen] = useState(false);
  const [showWizard, setShowWizard] = useState(false);
  const [wizardChecked, setWizardChecked] = useState(false);
  const mobile = isMobile();

  const openCommandPalette = useCallback(() => setCmdPaletteOpen(true), []);
  const closeCommandPalette = useCallback(() => setCmdPaletteOpen(false), []);
  const openSearch = useCallback(() => setSearchOpen(true), []);
  const closeSearch = useCallback(() => setSearchOpen(false), []);

  useKeyboardShortcuts({ onOpenCommandPalette: openCommandPalette, onOpenSearch: openSearch });

  // Connect to pale-server SSE for real-time presence & message events
  const serverBaseUrl = useServerStore((s) => s.baseUrl);
  const serverToken = useServerStore((s) => s.token);
  const setServerConnection = useServerStore((s) => s.setConnection);
  useServerEvents(serverBaseUrl, serverToken);

  const setAccount = useAccountStore((s) => s.setAccount);

  // Check if this is first run or auto-login with saved credentials
  useEffect(() => {
    getConfig()
      .then(async (config) => {
        // Auto-login: if server is configured with auto_connect and not already connected
        const alreadyConnected = useServerStore.getState().connected;
        if (!alreadyConnected && config.server?.url && config.server.username && config.server.auto_connect) {
          try {
            const savedPassword = await getSipPassword("pale-server-login");
            if (savedPassword) {
              const response = await paleLogin(config.server.url, config.server.username, savedPassword);
              sessionStorage.setItem("pale.admin.token", response.token);
              setServerConnection(config.server.url, response.token, response.expires_at, response.user.role, response.user.display_name);

              // Auto-register SIP
              if (response.sip_credentials) {
                const creds = response.sip_credentials;
                setAccount({
                  displayName: response.user.display_name,
                  sipUri: creds.sip_uri,
                  registrarUri: creds.registrar_uri,
                  authUsername: creds.username,
                  transport: (creds.transport as "udp" | "tcp" | "tls") || "udp",
                });
                await registerAccount({
                  display_name: response.user.display_name,
                  sip_uri: creds.sip_uri,
                  registrar_uri: creds.registrar_uri,
                  auth_username: creds.username,
                  auth_password: creds.password,
                  transport: (creds.transport as "udp" | "tcp" | "tls") || "udp",
                }).catch(() => {});
              }

              setWizardChecked(true);
              return;
            }
          } catch {
            // Auto-login failed, fall through to show wizard
          }
        }

        // Show wizard if not connected to server — need credentials
        if (!useServerStore.getState().connected) {
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
      <div className="flex flex-col h-screen w-screen overflow-hidden safe-area-top safe-area-bottom">
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
    <div className="flex flex-col h-screen w-screen overflow-hidden safe-area-top safe-area-bottom">
      {!mobile && <TitleBar />}
      <StatusBar />

      <main className="flex-1 overflow-y-auto relative">
        {hasActiveCall ? <ActiveCallView /> : <View />}
      </main>

      {!hasActiveCall && <BottomNav />}

      {/* Overlays */}
      <IncomingCallOverlay />
      <CommandPalette open={cmdPaletteOpen} onClose={closeCommandPalette} />
      <SearchOverlay open={searchOpen} onClose={closeSearch} />
      <ToastContainer />
    </div>
  );
}
