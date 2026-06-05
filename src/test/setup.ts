import "@testing-library/jest-dom/vitest";

// Mock Tauri APIs for testing
vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn().mockResolvedValue(undefined),
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn().mockResolvedValue(() => {}),
}));

vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: vi.fn(() => ({
    minimize: vi.fn(),
    toggleMaximize: vi.fn(),
    close: vi.fn(),
    show: vi.fn(),
    hide: vi.fn(),
    unminimize: vi.fn(),
    setFocus: vi.fn(),
  })),
}));
