import { useEffect } from "react";
import { getConfig, getSipPassword, registerAccount } from "@/lib/tauri";
import { useAccountStore } from "@/store/accountStore";
import { useUiStore } from "@/store/uiStore";

/**
 * Loads persisted config on app startup and restores state.
 * Automatically registers the SIP account if saved credentials exist.
 */
export function useConfigLoader() {
  const setAccount = useAccountStore((s) => s.setAccount);
  const setTheme = useUiStore((s) => s.setTheme);

  useEffect(() => {
    let cancelled = false;

    async function load() {
      try {
        const config = await getConfig();

        if (cancelled) return;

        // Restore theme
        if (config.ui?.theme === "light" || config.ui?.theme === "dark") {
          setTheme(config.ui.theme as "dark" | "light");
        }

        // Restore account and auto-register with PJSIP
        if (config.account?.sip_uri && config.account?.registrar_uri) {
          const acct = config.account;
          setAccount({
            displayName: acct.display_name,
            sipUri: acct.sip_uri,
            registrarUri: acct.registrar_uri,
            authUsername: acct.auth_username,
            transport: acct.transport,
          });

          // Retrieve password from OS keychain and register with PJSIP
          const password = await getSipPassword(acct.sip_uri).catch(() => null);
          if (password && !cancelled) {
            await registerAccount({
              display_name: acct.display_name,
              sip_uri: acct.sip_uri,
              registrar_uri: acct.registrar_uri,
              auth_username: acct.auth_username,
              auth_password: password,
              transport: (acct.transport as "udp" | "tcp" | "tls") || "tls",
            }).catch((e) => {
              console.warn("Auto SIP registration failed:", e);
            });
          }
        }
      } catch {
        // Config not available yet — first run
      }
    }

    load();
    return () => {
      cancelled = true;
    };
  }, [setAccount, setTheme]);
}
