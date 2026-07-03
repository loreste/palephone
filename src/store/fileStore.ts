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

export interface ServerFile {
  id: string;
  owner: string;
  filename: string;
  content_type: string;
  size: number;
  sha256: string;
  created_at: string;
  dlp_status?: string;
  dlp_violation_count?: number;
  legal_hold?: boolean;
  deleted_at?: string | null;
  deleted_by?: string | null;
  folder_id?: string | null;
  locked_by?: string | null;
  locked_at?: string | null;
}

export interface FileVersion {
  id: string;
  file_id: string;
  version_number: number;
  uploader: string;
  size: number;
  sha256: string;
  created_at: string;
  storage_path: string;
}

export interface Folder {
  id: string;
  room_id: string;
  parent_id: string | null;
  name: string;
  created_by: string;
  created_at: string;
}

export interface ApprovalRequest {
  id: string;
  title: string;
  description: string;
  requestor: string;
  approvers: string[];
  status: string;
  responses: Array<{ user: string; decision: string; comment?: string; responded_at: string }>;
  room_id?: string | null;
  created_at: string;
  resolved_at?: string | null;
}

interface FileStoreState {
  transfers: FileTransfer[];
  sharedFiles: SharedFile[];
  serverFiles: ServerFile[];

  addTransfer: (transfer: FileTransfer) => void;
  updateTransfer: (id: string, update: Partial<FileTransfer>) => void;
  removeTransfer: (id: string) => void;
  addSharedFile: (file: SharedFile) => void;
  setSharedFiles: (files: SharedFile[]) => void;
  setServerFiles: (files: ServerFile[]) => void;
  removeServerFile: (id: string) => void;
}

export const useFileStore = create<FileStoreState>((set) => ({
  transfers: [],
  sharedFiles: [],
  serverFiles: [],

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

  setServerFiles: (files) => set({ serverFiles: files }),

  removeServerFile: (id) =>
    set((state) => ({
      serverFiles: state.serverFiles.filter((f) => f.id !== id),
    })),
}));
