import { describe, it, expect } from "vitest";
import en from "@/i18n/locales/en.json";

describe("i18n translations", () => {
  it("has all navigation keys", () => {
    expect(en.nav.calls).toBe("Calls");
    expect(en.nav.chat).toBe("Chat");
    expect(en.nav.people).toBe("People");
    expect(en.nav.files).toBe("Files");
    expect(en.nav.recent).toBe("Recent");
    expect(en.nav.admin).toBe("Admin");
    expect(en.nav.settings).toBe("Settings");
  });

  it("has all presence status keys", () => {
    expect(en.presence.online).toBe("Online");
    expect(en.presence.offline).toBe("Offline");
    expect(en.presence.busy).toBe("Busy");
    expect(en.presence.away).toBe("Away");
    expect(en.presence.dnd).toBe("Do Not Disturb");
  });

  it("has all settings keys", () => {
    expect(en.settings.title).toBe("Settings");
    expect(en.settings.account).toBe("Account");
    expect(en.settings.server).toBe("Server");
    expect(en.settings.notifications).toBe("Notifications");
  });

  it("has wizard keys", () => {
    expect(en.wizard.welcome).toBe("Welcome to Pale");
    expect(en.wizard.getStarted).toBe("Get Started");
    expect(en.wizard.allSet).toBe("You're all set!");
  });

  it("has chat keys", () => {
    expect(en.chat.title).toBe("Chat");
    expect(en.chat.typeMessage).toBe("Type a message...");
    expect(en.chat.directMessage).toBe("Direct Message");
    expect(en.chat.groupRoom).toBe("Group Room");
  });

  it("has no empty string values", () => {
    const checkObject = (obj: Record<string, unknown>, path: string) => {
      for (const [key, value] of Object.entries(obj)) {
        if (typeof value === "string") {
          expect(value.length, `${path}.${key} should not be empty`).toBeGreaterThan(0);
        } else if (typeof value === "object" && value !== null) {
          checkObject(value as Record<string, unknown>, `${path}.${key}`);
        }
      }
    };
    checkObject(en as Record<string, unknown>, "en");
  });
});
