import type { AccountConfig } from "@/lib/tauri";
import type { SipAccount } from "@/types";

const DEFAULT_TLS_PORT = 5061;

type SipTransport = AccountConfig["transport"];

function normalizeTransport(value?: string | null): SipTransport {
  const normalized = value?.trim().toLowerCase();
  if (normalized === "udp" || normalized === "tcp" || normalized === "tls") {
    return normalized;
  }
  return "tls";
}

function stripSipScheme(value: string): string {
  return value.replace(/^sips?:/i, "");
}

function hasExplicitPort(authority: string): boolean {
  const hostPort = authority.split(";")[0] ?? authority;
  if (hostPort.startsWith("[") && hostPort.includes("]")) {
    return hostPort.slice(hostPort.indexOf("]") + 1).startsWith(":");
  }
  return /:\d+$/.test(hostPort);
}

function appendDefaultTlsPort(authority: string): string {
  if (hasExplicitPort(authority)) return authority;
  const [host, ...params] = authority.split(";");
  return [`${host}:${DEFAULT_TLS_PORT}`, ...params].join(";");
}

export function normalizeRegistrarUri(registrarUri: string, transport?: string | null): string {
  const trimmed = registrarUri.trim();
  if (!trimmed) return "";

  const schemeMatch = trimmed.match(/^(sips?):/i);
  const scheme = schemeMatch?.[1].toLowerCase();
  const authority = stripSipScheme(trimmed);
  const effectiveTransport = scheme === "sips" ? "tls" : normalizeTransport(transport);

  if (effectiveTransport === "tls") {
    return `sips:${appendDefaultTlsPort(authority)}`;
  }

  return scheme ? `${scheme}:${authority}` : `sip:${authority}`;
}

export function normalizeProvisionedSipAccount(input: {
  displayName: string;
  sipUri: string;
  registrarUri: string;
  authUsername: string;
  transport?: string | null;
}): SipAccount {
  const transport = normalizeTransport(input.transport);
  return {
    displayName: input.displayName,
    sipUri: input.sipUri,
    registrarUri: normalizeRegistrarUri(input.registrarUri, transport),
    authUsername: input.authUsername,
    transport,
  };
}
