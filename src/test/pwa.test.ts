import { describe, expect, it } from "vitest";
import { shouldRegisterServiceWorker } from "@/lib/pwa";

describe("PWA service worker gate", () => {
  it("does not register during dev", () => {
    expect(
      shouldRegisterServiceWorker({
        dev: true,
        hasWindow: true,
        hasNavigator: true,
        hasServiceWorker: true,
        secureContext: true,
      }),
    ).toBe(false);
  });

  it("does not register inside the Tauri shell", () => {
    expect(
      shouldRegisterServiceWorker({
        dev: false,
        hasWindow: true,
        hasNavigator: true,
        hasServiceWorker: true,
        isTauri: true,
        secureContext: true,
      }),
    ).toBe(false);
  });

  it("registers for secure browser production contexts", () => {
    expect(
      shouldRegisterServiceWorker({
        dev: false,
        hasWindow: true,
        hasNavigator: true,
        hasServiceWorker: true,
        isTauri: false,
        secureContext: true,
        hostname: "teams.example.com",
      }),
    ).toBe(true);
  });

  it("allows localhost for preview deployments", () => {
    expect(
      shouldRegisterServiceWorker({
        dev: false,
        hasWindow: true,
        hasNavigator: true,
        hasServiceWorker: true,
        isTauri: false,
        secureContext: false,
        hostname: "localhost",
      }),
    ).toBe(true);
  });
});
