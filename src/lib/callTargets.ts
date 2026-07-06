import type { RegState, SipAccount } from "@/types";

export type SipCallPreflight =
  | { ok: true; uri: string; label: string }
  | { ok: false; reason: string };

export function accountDomain(account: SipAccount | null): string | null {
  if (!account) return null;
  const fromSipUri = account.sipUri?.split("@")[1]?.split(/[;>]/)[0]?.trim();
  if (fromSipUri) return fromSipUri;
  const fromRegistrar = account.registrarUri
    ?.replace(/^sips?:/, "")
    .split(/[;>]/)[0]
    ?.trim();
  return fromRegistrar || null;
}

export function normalizeSipDestination(input: string, account: SipAccount | null): string | null {
  const trimmed = input.trim();
  if (!trimmed) return null;
  if (trimmed.startsWith("sip:") || trimmed.startsWith("sips:")) return trimmed;
  if (trimmed.includes("@")) return `sip:${trimmed}`;
  const domain = accountDomain(account);
  return domain ? `sip:${trimmed}@${domain}` : null;
}

export function preflightSipCall(
  input: string,
  account: SipAccount | null,
  regState: RegState,
): SipCallPreflight {
  if (regState !== "registered") {
    return { ok: false, reason: "SIP account is not registered yet" };
  }
  const uri = normalizeSipDestination(input, account);
  if (!uri) {
    return { ok: false, reason: "Enter a SIP URI or register an account with a SIP domain" };
  }
  const label = uri.replace(/^sips?:/, "").split("@")[0] || uri;
  return { ok: true, uri, label };
}
