import { describe, expect, it } from "vitest";
import { normalizeProvisionedSipAccount, normalizeRegistrarUri } from "@/lib/sipDefaults";

describe("SIP defaults", () => {
  it("uses SIP over TLS on 5061 when the registrar has no port", () => {
    expect(normalizeRegistrarUri("drcpbx.com", "tls")).toBe("sips:drcpbx.com:5061");
    expect(normalizeRegistrarUri("sip:drcpbx.com", "tls")).toBe("sips:drcpbx.com:5061");
  });

  it("keeps an explicit TLS registrar port", () => {
    expect(normalizeRegistrarUri("sip:drcpbx.com:5061", "tls")).toBe("sips:drcpbx.com:5061");
  });

  it("defaults unknown provisioned transport to TLS", () => {
    expect(
      normalizeProvisionedSipAccount({
        displayName: "Super Admin",
        sipUri: "sip:superadmin@drcpbx.com",
        registrarUri: "drcpbx.com",
        authUsername: "superadmin",
        transport: "",
      }),
    ).toMatchObject({
      registrarUri: "sips:drcpbx.com:5061",
      transport: "tls",
    });
  });
});
