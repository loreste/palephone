import { useEffect } from "react";
import { getConfig } from "@/lib/tauri";
import { useAccountStore } from "@/store/accountStore";
import { useUiStore } from "@/store/uiStore";

/**
 * Loads persisted config on app startup and restores state.
 * Attempts to auto-register if a saved account exists.
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

        // Restore account (without password)
        if (config.account) {
          setAccount({
            displayName: config.account.display_name,
            sipUri: config.account.sip_uri,
            registrarUri: config.account.registrar_uri,
            authUsername: config.account.auth_username,
            transport: config.account.transport,
          });
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
