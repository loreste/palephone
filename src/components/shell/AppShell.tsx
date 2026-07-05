import { lazy, Suspense, useState, useCallback, useEffect } from "react";
import { ExternalLink, WifiOff } from "lucide-react";
import { TitleBar } from "./TitleBar";
import { StatusBar } from "./StatusBar";
import { BottomNav } from "./BottomNav";
import { useUiStore } from "@/store/uiStore";
import { isMobile } from "@/hooks/usePlatform";
import { useCallStore } from "@/store/callStore";
import { useChatStore } from "@/store/chatStore";
import { t } from "@/lib/i18n";
import { DialpadView } from "@/components/dialpad/DialpadView";
import { SettingsView } from "@/components/settings/SettingsView";
import { RecentCallsList } from "@/components/recent/RecentCallsList";
import { ChatView } from "@/components/chat/ChatView";
import { PeopleView } from "@/components/people/PeopleView";
import { FilesView } from "@/components/files/FilesView";
import { CalendarView } from "@/components/calendar/CalendarView";
import { ActiveCallView } from "@/components/call/ActiveCallView";
import { IncomingCallOverlay } from "@/components/call/IncomingCallOverlay";
import { CommandPalette } from "@/components/shared/CommandPalette";
import { SearchOverlay } from "@/components/shared/SearchOverlay";
import { SetupWizard } from "@/components/auth/SetupWizard";
import { ToastContainer } from "@/components/ui/Toast";
import { useKeyboardShortcuts } from "@/hooks/useKeyboardShortcuts";
import { useServerEvents } from "@/hooks/useServerEvents";
import { useAutoAway } from "@/hooks/useAutoAway";
import { useMeetingReminders } from "@/hooks/useMeetingReminders";
import { useServerStore } from "@/store/serverStore";
import { useAccountStore } from "@/store/accountStore";
import { getConfig, getSipPassword, openPopoutWindow, paleLogin, registerAccount, saveSettings } from "@/lib/tauri";

/**
 * Persisted on the frontend (localStorage) rather than in config.ui: the Rust
 * UiPersist struct only round-trips known fields, so an extra TS-only field
 * would be silently dropped by save_settings.
 */
const SETUP_COMPLETE_KEY = "pale.setup_complete";
const AdminView = lazy(() => import("@/components/admin/AdminView").then((module) => ({ default: module.AdminView })));

const views = {
  dialpad: DialpadView,
  chat: ChatView,
  people: PeopleView,
  files: FilesView,
  recent: RecentCallsList,
  calendar: CalendarView,
  admin: AdminView,
  settings: SettingsView,
} as const;

export function AppShell() {
  const activeTab = useUiStore((s) => s.activeTab);
  const activeCallId = useCallStore((s) => s.activeCallId);
  const activeRoomId = useChatStore((s) => s.activeRoomId);
  const hasActiveCall = activeCallId !== null;
  const isOffline = useChatStore((s) => s.isOffline);
  const queuedCount = useChatStore((s) => s.queuedMessages.length);

  const [cmdPaletteOpen, setCmdPaletteOpen] = useState(false);
  const [searchOpen, setSearchOpen] = useState(false);
  const [showWizard, setShowWizard] = useState(false);
  const [wizardChecked, setWizardChecked] = useState(false);
  const mobile = isMobile();

  const openCommandPalette = useCallback(() => setCmdPaletteOpen(true), []);
  const closeCommandPalette = useCallback(() => setCmdPaletteOpen(false), []);
  const openSearch = useCallback(() => setSearchOpen(true), []);
  const closeSearch = useCallback(() => setSearchOpen(false), []);

  const popoutCurrentView = useCallback(() => {
    const kind = hasActiveCall ? "call" : activeTab === "chat" ? "chat" : activeTab === "files" ? "files" : activeTab === "calendar" ? "calendar" : null;
    if (!kind) return;
    const targetId = hasActiveCall ? String(activeCallId) : activeTab === "chat" ? activeRoomId : null;
    openPopoutWindow(kind, targetId, `Pale ${kind}`).catch(() => {});
  }, [activeCallId, activeRoomId, activeTab, hasActiveCall]);

  useKeyboardShortcuts({ onOpenCommandPalette: openCommandPalette, onOpenSearch: openSearch });

  // Connect to pale-server SSE for real-time presence & message events
  const serverBaseUrl = useServerStore((s) => s.baseUrl);
  const serverToken = useServerStore((s) => s.token);
  const setServerConnection = useServerStore((s) => s.setConnection);
  const setServerIdentity = useServerStore((s) => s.setIdentity);
  useServerEvents(serverBaseUrl, serverToken);
  useAutoAway();
  useMeetingReminders();

  const setAccount = useAccountStore((s) => s.setAccount);

  // Check if this is first run or auto-login with saved credentials
  useEffect(() => {
    getConfig()
      .then(async (config) => {
        if (config.server?.role) {
          setServerIdentity(config.server.role, config.server.display_name);
        }

        // Auto-login: if server is configured with auto_connect and not already connected
        const alreadyConnected = useServerStore.getState().connected;
        if (!alreadyConnected && config.server?.url && config.server.username && config.server.auto_connect) {
          try {
            const savedPassword = await getSipPassword("pale-server-login");
            if (savedPassword) {
              const response = await paleLogin(config.server.url, config.server.username, savedPassword);
              sessionStorage.setItem("pale.admin.token", response.token);
              setServerConnection(config.server.url, response.token, response.expires_at, response.user.role, response.user.display_name);
              config.server = {
                ...config.server,
                role: response.user.role,
                display_name: response.user.display_name,
              };
              await saveSettings(config).catch(() => {});

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

              // Auto-subscribe to push notifications (non-blocking)
              import("@/lib/pushSubscription")
                .then(({ subscribeToPush }) =>
                  subscribeToPush(config.server!.url!, response.token)
                )
                .catch(() => {});

              setWizardChecked(true);
              return;
            }
          } catch {
            // Auto-login failed, fall through to show wizard
          }
        }

        // Show wizard if not connected to server and the user has never
        // completed (or skipped) setup. Skipped users run in local SIP-only
        // mode and can connect later via Settings > Server.
        if (
          !useServerStore.getState().connected &&
          localStorage.getItem(SETUP_COMPLETE_KEY) !== "1"
        ) {
          setShowWizard(true);
        }

        setWizardChecked(true);
      })
      .catch(() => {
        if (localStorage.getItem(SETUP_COMPLETE_KEY) !== "1") {
          setShowWizard(true);
        }
        setWizardChecked(true);
      });
  }, [setAccount, setServerConnection, setServerIdentity]);

  if (!wizardChecked) return null;

  if (showWizard) {
    return (
      <div className="flex flex-col h-screen w-screen overflow-hidden safe-area-top safe-area-bottom">
        {!mobile && <TitleBar />}
        <main className="flex-1 overflow-y-auto">
          <SetupWizard
            onComplete={() => {
              localStorage.setItem(SETUP_COMPLETE_KEY, "1");
              setShowWizard(false);
            }}
          />
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
      {!mobile && (hasActiveCall || activeTab === "chat" || activeTab === "files" || activeTab === "calendar") && (
        <button
          onClick={popoutCurrentView}
          className="fixed right-3 top-10 z-30 h-8 w-8 inline-flex items-center justify-center rounded-md border border-border-subtle bg-surface text-tertiary hover:text-primary hover:bg-elevated"
          aria-label="Open in separate window"
          title="Open in separate window"
        >
          <ExternalLink size={15} />
        </button>
      )}

      {/* Offline indicator banner */}
      {isOffline && (
        <div
          className="flex items-center gap-2 px-4 py-2 bg-warning/20 border-b border-warning/30 text-warning text-xs font-medium"
          role="alert"
          aria-live="assertive"
        >
          <WifiOff size={14} aria-hidden="true" />
          <span>{t("offline.banner")}</span>
          {queuedCount > 0 && (
            <span className="ml-auto text-warning/80">
              {queuedCount} {t("offline.queued")}
            </span>
          )}
        </div>
      )}

      <main className="flex-1 overflow-y-auto relative">
        {hasActiveCall ? (
          <ActiveCallView />
        ) : (
          <Suspense fallback={<div className="p-4 text-sm text-secondary">Loading...</div>}>
            <View />
          </Suspense>
        )}
      </main>

      {!hasActiveCall && <BottomNav />}

      {/* Overlays */}
      <IncomingCallOverlay />
      <CommandPalette open={cmdPaletteOpen} onClose={closeCommandPalette} />
      <SearchOverlay open={searchOpen} onClose={closeSearch} />
      {/* Notification toast region with aria-live for screen readers */}
      <div aria-live="polite" aria-atomic="true">
        <ToastContainer />
      </div>
    </div>
  );
}
