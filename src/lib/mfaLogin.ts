/**
 * Helpers for password login when the server requires MFA enrollment or TOTP.
 */

import { paleLogin, type UserLoginResponse } from "@/lib/tauri";
import { setupMfa, validateMfa, verifyMfa, type MfaSetupResponse } from "@/lib/adminApi";

export type MfaLoginPhase =
  | { kind: "complete"; session: UserLoginResponse }
  | {
      kind: "mfa_pending";
      serverUrl: string;
      pendingToken: string;
      user: UserLoginResponse["user"];
      /** Present when enrollment is still needed (CA require_mfa, no TOTP yet). */
      needsEnrollment: boolean;
    };

/** Password login; returns either a full session or an MFA-pending challenge. */
export async function loginWithPossibleMfa(
  serverUrl: string,
  sipUri: string,
  password: string,
): Promise<MfaLoginPhase> {
  const response = await paleLogin(serverUrl, sipUri, password);
  if (response.mfa_required) {
    // Probe whether enrollment is needed: setup is allowed with mfa_pending token.
    // If MFA is already enabled, setup still returns a secret but verify/enable may no-op;
    // we treat status via a soft heuristic: try validate path first after UI enroll check.
    return {
      kind: "mfa_pending",
      serverUrl,
      pendingToken: response.token,
      user: response.user,
      needsEnrollment: true,
    };
  }
  return { kind: "complete", session: response };
}

/** Start TOTP enrollment using an mfa_pending (or full) token. */
export async function beginMfaEnrollment(
  serverUrl: string,
  pendingToken: string,
): Promise<MfaSetupResponse> {
  return setupMfa(serverUrl, pendingToken);
}

/** Confirm enrollment code, then complete login with the same code. */
export async function enrollAndValidateMfa(
  serverUrl: string,
  pendingToken: string,
  code: string,
): Promise<UserLoginResponse> {
  await verifyMfa(serverUrl, pendingToken, code);
  return validateMfa(serverUrl, pendingToken, code);
}

/** Validate TOTP when the user is already enrolled. */
export async function completeMfaLogin(
  serverUrl: string,
  pendingToken: string,
  code: string,
): Promise<UserLoginResponse> {
  return validateMfa(serverUrl, pendingToken, code);
}
