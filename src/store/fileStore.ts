import { create } from "zustand";

export interface FileTransfer {
  id: string;
  filename: string;
  roomId: string;
  direction: "upload" | "download";
  totalBytes: number;
  transferredBytes: number;
  status: "pending" | "in_progress" | "complete" | "failed";
  error?: string;
  mimeType?: string;
  localPath?: string;
}

export interface SharedFile {
  eventId: string;
  roomId: string;
  roomName: string;
  filename: string;
  size: number | null;
  mimeType: string | null;
  sender: string;
  timestamp: number;
  url: string;
}

interface FileStoreState {
  transfers: FileTransfer[];
  sharedFiles: SharedFile[];

  addTransfer: (transfer: FileTransfer) => void;
  updateTransfer: (id: string, update: Partial<FileTransfer>) => void;
  removeTransfer: (id: string) => void;
  addSharedFile: (file: SharedFile) => void;
  setSharedFiles: (files: SharedFile[]) => void;
}

export const useFileStore = create<FileStoreState>((set) => ({
  transfers: [],
  sharedFiles: [],

  addTransfer: (transfer) =>
    set((state) => ({ transfers: [...state.transfers, transfer] })),

  updateTransfer: (id, update) =>
    set((state) => ({
      transfers: state.transfers.map((t) =>
        t.id === id ? { ...t, ...update } : t
      ),
    })),

  removeTransfer: (id) =>
    set((state) => ({
      transfers: state.transfers.filter((t) => t.id !== id),
    })),

  addSharedFile: (file) =>
    set((state) => ({
      sharedFiles: [file, ...state.sharedFiles],
    })),

  setSharedFiles: (files) => set({ sharedFiles: files }),
}));
