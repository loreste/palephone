import { describe, it, expect, beforeEach } from "vitest";
import { useCallStore } from "@/store/callStore";
import { useAccountStore } from "@/store/accountStore";
import { useUiStore } from "@/store/uiStore";
import { useChatStore } from "@/store/chatStore";
import type { CallSession } from "@/types";

describe("callStore", () => {
  beforeEach(() => {
    useCallStore.getState().clearAll();
  });

  const mockSession: CallSession = {
    id: 1,
    direction: "outbound",
    state: "dialing",
    remoteUri: "sip:alice@example.com",
    remoteName: "Alice",
    startTime: Date.now(),
    connectTime: null,
    isMuted: false,
    isHeld: false,
  };

  it("adds and removes sessions", () => {
    const store = useCallStore.getState();
    store.addSession(mockSession);
    expect(useCallStore.getState().sessions).toHaveLength(1);
    expect(useCallStore.getState().sessions[0].remoteName).toBe("Alice");

    store.removeSession(1);
    expect(useCallStore.getState().sessions).toHaveLength(0);
  });

  it("updates session state", () => {
    const store = useCallStore.getState();
    store.addSession(mockSession);
    store.updateSessionState(1, "connected");
    expect(useCallStore.getState().sessions[0].state).toBe("connected");
  });

  it("sets muted and held", () => {
    const store = useCallStore.getState();
    store.addSession(mockSession);

    store.setMuted(1, true);
    expect(useCallStore.getState().sessions[0].isMuted).toBe(true);

    store.setHeld(1, true);
    expect(useCallStore.getState().sessions[0].isHeld).toBe(true);
  });

  it("tracks active call ID", () => {
    const store = useCallStore.getState();
    store.addSession(mockSession);
    store.setActiveCallId(1);
    expect(useCallStore.getState().activeCallId).toBe(1);

    store.removeSession(1);
    expect(useCallStore.getState().activeCallId).toBeNull();
  });

  it("manages incoming call", () => {
    const store = useCallStore.getState();
    store.setIncomingCall(mockSession);
    expect(useCallStore.getState().incomingCall).not.toBeNull();
    expect(useCallStore.getState().incomingCall?.remoteName).toBe("Alice");

    store.setIncomingCall(null);
    expect(useCallStore.getState().incomingCall).toBeNull();
  });
});

describe("accountStore", () => {
  it("sets account and registration state", () => {
    const store = useAccountStore.getState();

    store.setAccount({
      displayName: "Test User",
      sipUri: "test@sip.example.com",
      registrarUri: "sip.example.com",
      authUsername: "test",
      transport: "tls",
    });
    expect(useAccountStore.getState().account?.sipUri).toBe("test@sip.example.com");

    store.setRegState("registered");
    expect(useAccountStore.getState().regState).toBe("registered");

    store.setRegState("unregistered", "Timeout");
    expect(useAccountStore.getState().regState).toBe("unregistered");
    expect(useAccountStore.getState().regError).toBe("Timeout");
  });
});

describe("uiStore", () => {
  it("switches tabs", () => {
    const store = useUiStore.getState();
    store.setActiveTab("chat");
    expect(useUiStore.getState().activeTab).toBe("chat");

    store.setActiveTab("settings");
    expect(useUiStore.getState().activeTab).toBe("settings");
  });

  it("toggles theme", () => {
    const store = useUiStore.getState();
    store.setTheme("dark");
    expect(useUiStore.getState().theme).toBe("dark");

    store.toggleTheme();
    expect(useUiStore.getState().theme).toBe("light");

    store.toggleTheme();
    expect(useUiStore.getState().theme).toBe("dark");
  });
});

describe("chatStore", () => {
  beforeEach(() => {
    useChatStore.getState().setRooms([]);
    useChatStore.getState().setActiveRoomId(null);
  });

  it("manages rooms", () => {
    const store = useChatStore.getState();
    store.setRooms([
      {
        room_id: "!room1:example.com",
        name: "General",
        is_direct: false,
        is_encrypted: true,
        last_message: null,
        last_message_sender: null,
        last_message_ts: null,
        unread_count: 3,
      },
    ]);
    expect(useChatStore.getState().rooms).toHaveLength(1);
    expect(useChatStore.getState().rooms[0].name).toBe("General");
  });

  it("adds messages and avoids duplicates", () => {
    const store = useChatStore.getState();
    store.setRooms([{
      room_id: "!room1:example.com",
      name: "Test",
      is_direct: true,
      is_encrypted: true,
      last_message: null,
      last_message_sender: null,
      last_message_ts: null,
      unread_count: 0,
    }]);

    const msg = {
      event_id: "$evt1",
      room_id: "!room1:example.com",
      sender: "@alice:example.com",
      sender_name: "Alice",
      body: "Hello!",
      msg_type: "text" as const,
      timestamp: 1000,
      is_encrypted: true,
      is_own: false,
    };

    store.addMessage(msg);
    expect(useChatStore.getState().messages["!room1:example.com"]).toHaveLength(1);

    // Adding same message again should not duplicate
    store.addMessage(msg);
    expect(useChatStore.getState().messages["!room1:example.com"]).toHaveLength(1);
  });
});
