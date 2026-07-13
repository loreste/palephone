import { useEffect, useState } from "react";
import { AppShell } from "@/components/shell/AppShell";
import { useUiStore } from "@/store/uiStore";
import { useSipEvents } from "@/hooks/useSipEvents";
import { useRingtone } from "@/hooks/useRingtone";
import { useConfigLoader } from "@/hooks/useConfigLoader";
import { useMatrixEvents } from "@/hooks/useMatrixEvents";
import {
  completeSsoCallback,
  readSsoCallbackParams,
  takeRememberedSsoServerUrl,
} from "@/lib/ssoLogin";
import { storeSipPassword, getConfig, saveSettings } from "@/lib/tauri";
import { useServerStore } from "@/store/serverStore";
import { toast } from "@/components/ui/Toast";

export default function App() {
  const theme = useUiStore((s) => s.theme);
  const [ssoBusy, setSsoBusy] = useState(false);

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

  // Complete OIDC redirect: /auth/sso/callback?code=&state=
  useEffect(() => {
    const path = window.location.pathname || "";
    const isCallback =
      path.includes("/auth/sso/callback") || path.endsWith("/auth/sso/callback");
    const params = readSsoCallbackParams();
    if (!params) return;
    if (!isCallback && !window.location.search.includes("code=")) return;

    const serverUrl =
      takeRememberedSsoServerUrl() ||
      useServerStore.getState().baseUrl ||
      "https://drcpbx.com";

    setSsoBusy(true);
    void completeSsoCallback(serverUrl, params.code, params.state)
      .then(async (phase) => {
        if (phase.kind === "mfa_pending") {
          sessionStorage.setItem("pale.admin.token", phase.pendingToken);
          useServerStore.getState().setConnection(
            phase.serverUrl,
            phase.pendingToken,
            new Date(Date.now() + 10 * 60 * 1000).toISOString(),
            "mfa_pending",
            phase.user.display_name,
          );
          toast({
            type: "info",
            title: "SSO almost done",
            description: "Enter your authenticator code in Settings or reopen setup.",
          });
        } else {
          const session = phase.session;
          sessionStorage.setItem("pale.admin.token", session.token);
          useServerStore.getState().setConnection(
            serverUrl,
            session.token,
            session.expires_at,
            session.user.role,
            session.user.display_name,
          );
          await storeSipPassword("pale-server-login", "").catch(() => {});
          const config = await getConfig().catch(() => null);
          if (config) {
            config.server = {
              url: serverUrl,
              username: session.user.sip_uri,
              auto_connect: true,
              role: session.user.role,
              display_name: session.user.display_name,
            };
            await saveSettings(config).catch(() => {});
          }
          localStorage.setItem("pale.setup_complete", "1");
          toast({
            type: "success",
            title: `Welcome, ${session.user.display_name}!`,
            description: "SSO sign-in complete.",
          });
        }
        // Clean OAuth params from the address bar
        const clean = `${window.location.origin}${window.location.pathname.replace(/\/auth\/sso\/callback.*/, "/")}`;
        window.history.replaceState({}, "", clean || "/");
      })
      .catch((err) => {
        toast({ type: "error", title: "SSO failed", description: String(err) });
      })
      .finally(() => setSsoBusy(false));
  }, []);

  if (ssoBusy) {
    return (
      <div className="flex h-full items-center justify-center text-sm text-secondary">
        Completing single sign-on…
      </div>
    );
  }

  return <AppShell />;
}
