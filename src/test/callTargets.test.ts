import { describe, expect, it } from "vitest";
import { accountDomain, normalizeSipDestination, preflightSipCall } from "@/lib/callTargets";
import type { SipAccount } from "@/types";

const account: SipAccount = {
  displayName: "Fay Oreste",
  sipUri: "1005@drcpbx.com",
  registrarUri: "drcpbx.com:5061",
  authUsername: "1005",
  transport: "tls",
};

describe("call target preflight", () => {
  it("derives the domain from the registered account", () => {
    expect(accountDomain(account)).toBe("drcpbx.com");
  });

  it("normalizes full SIP and bare extension targets", () => {
    expect(normalizeSipDestination("sip:1004@drcpbx.com", account)).toBe("sip:1004@drcpbx.com");
    expect(normalizeSipDestination("1004@drcpbx.com", account)).toBe("sip:1004@drcpbx.com");
    expect(normalizeSipDestination("1004", account)).toBe("sip:1004@drcpbx.com");
  });

  it("blocks voice and video calls until the SIP account is registered", () => {
    expect(preflightSipCall("sip:1004@drcpbx.com", account, "registering")).toEqual({
      ok: false,
      reason: "SIP account is not registered yet",
    });
  });

  it("returns a callable URI only when registration is active", () => {
    expect(preflightSipCall("1004", account, "registered")).toEqual({
      ok: true,
      uri: "sip:1004@drcpbx.com",
      label: "1004",
    });
  });
});
