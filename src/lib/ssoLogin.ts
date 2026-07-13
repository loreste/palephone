/**
 * Client SSO (OIDC) helpers for SetupWizard and Settings.
 *
 * Flow:
 * 1. listPublicSsoProviders(serverUrl)
 * 2. startSsoLogin(serverUrl, providerId) → open redirect_url in browser
 * 3. IdP redirects to redirect_uri with ?code=&state=
 * 4. completeSsoCallback(serverUrl, code, state) → session (or mfa_pending)
 */

import { paleFetch, type UserLoginResponse } from "@/lib/tauri";

export interface PublicSsoProvider {
  id: string;
  name: string;
  provider_type: string;
  enabled: boolean;
}

export type SsoLoginPhase =
  | { kind: "complete"; session: UserLoginResponse }
  | {
      kind: "mfa_pending";
      serverUrl: string;
      pendingToken: string;
      user: UserLoginResponse["user"];
      needsEnrollment: boolean;
    };

function normalizeBase(serverUrl: string): string {
  return serverUrl.replace(/\/$/, "");
}

export async function listPublicSsoProviders(serverUrl: string): Promise<PublicSsoProvider[]> {
  const base = normalizeBase(serverUrl);
  const res = await paleFetch(`${base}/v1/auth/sso/providers`);
  if (!res.ok) {
    throw new Error(`Failed to list SSO providers (${res.status})`);
  }
  return res.json();
}

export async function startSsoLogin(
  serverUrl: string,
  providerId: string,
): Promise<{ redirect_url: string; state: string; nonce: string }> {
  const base = normalizeBase(serverUrl);
  const res = await paleFetch(`${base}/v1/auth/sso/${providerId}/login`);
  if (!res.ok) {
    throw new Error(`SSO start failed (${res.status})`);
  }
  return res.json();
}

export async function completeSsoCallback(
  serverUrl: string,
  code: string,
  state: string,
  providerId?: string,
): Promise<SsoLoginPhase> {
  const base = normalizeBase(serverUrl);
  const res = await paleFetch(`${base}/v1/auth/sso/callback`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({
      code,
      state,
      provider_id: providerId || undefined,
    }),
  });
  if (!res.ok) {
    const text = await res.text().catch(() => "");
    throw new Error(text || `SSO callback failed (${res.status})`);
  }
  const body = await res.json();
  const session: UserLoginResponse = {
    token: body.token,
    user: body.user,
    expires_at: body.expires_at,
    mfa_required: !!body.mfa_required,
    sip_credentials: body.sip_credentials ?? null,
  };
  if (session.mfa_required) {
    return {
      kind: "mfa_pending",
      serverUrl: base,
      pendingToken: session.token,
      user: session.user,
      needsEnrollment: true,
    };
  }
  return { kind: "complete", session };
}

/** Persist server URL so the callback page can finish login after IdP redirect. */
export function rememberSsoServerUrl(serverUrl: string): void {
  try {
    sessionStorage.setItem("pale.sso.serverUrl", normalizeBase(serverUrl));
  } catch {
    /* ignore */
  }
}

export function takeRememberedSsoServerUrl(): string | null {
  try {
    const v = sessionStorage.getItem("pale.sso.serverUrl");
    return v;
  } catch {
    return null;
  }
}

/** Detect SSO callback query params from the current location. */
export function readSsoCallbackParams(
  search = typeof window !== "undefined" ? window.location.search : "",
): { code: string; state: string } | null {
  const params = new URLSearchParams(search);
  const code = params.get("code");
  const state = params.get("state");
  if (!code || !state) return null;
  return { code, state };
}

/** Open IdP authorize URL (system browser when possible). */
export async function openSsoAuthorizeUrl(redirectUrl: string): Promise<void> {
  // Prefer Tauri opener so the system browser can complete MFA/device login.
  try {
    const tauri = (window as unknown as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__;
    if (tauri) {
      const { openUrl } = await import("@tauri-apps/plugin-opener");
      await openUrl(redirectUrl);
      return;
    }
  } catch {
    /* fall through */
  }
  window.open(redirectUrl, "_blank", "noopener,noreferrer");
}
