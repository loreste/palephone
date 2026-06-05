import { useState, useCallback, useRef } from "react";
import {
  FileText, Image, Film, Music, Package,
  Upload, Download, Search,
} from "lucide-react";
import { cn } from "@/lib/cn";
import { useFileStore, type SharedFile, type FileTransfer } from "@/store/fileStore";
import { useChatStore } from "@/store/chatStore";
import { useMatrixStore } from "@/store/matrixStore";
import { matrixSendFile } from "@/lib/tauri";
import { toast } from "@/components/ui/Toast";
import { MatrixLoginView } from "@/components/auth/MatrixLoginView";

export function FilesView() {
  const authState = useMatrixStore((s) => s.authState);
  const { sharedFiles, transfers } = useFileStore();
  const rooms = useChatStore((s) => s.rooms);
  const [searchQuery, setSearchQuery] = useState("");
  const [dragOver, setDragOver] = useState(false);
  const fileInputRef = useRef<HTMLInputElement>(null);

  if (authState !== "logged_in") {
    return <MatrixLoginView />;
  }

  const filteredFiles = searchQuery
    ? sharedFiles.filter(
        (f) =>
          f.filename.toLowerCase().includes(searchQuery.toLowerCase()) ||
          f.roomName.toLowerCase().includes(searchQuery.toLowerCase())
      )
    : sharedFiles;

  const activeTransfers = transfers.filter((t) => t.status === "in_progress" || t.status === "pending");

  const handleFileDrop = useCallback(
    async (e: React.DragEvent) => {
      e.preventDefault();
      setDragOver(false);

      const files = Array.from(e.dataTransfer.files);
      if (files.length === 0 || rooms.length === 0) return;

      // Upload to first room (in a real app, show room picker)
      const targetRoom = rooms[0];
      for (const file of files) {
        try {
          // Tauri needs the file path — for drag-drop we use the path property
          const path = (file as any).path ?? file.name;
          await matrixSendFile(targetRoom.room_id, path);
          toast({ type: "success", title: "File sent", description: file.name });
        } catch (err) {
          toast({ type: "error", title: "Upload failed", description: String(err) });
        }
      }
    },
    [rooms]
  );

  return (
    <div
      className="flex flex-col h-full"
      onDragOver={(e) => {
        e.preventDefault();
        setDragOver(true);
      }}
      onDragLeave={() => setDragOver(false)}
      onDrop={handleFileDrop}
    >
      {/* Header */}
      <div className="flex items-center justify-between px-4 pt-4 pb-2">
        <h1 className="text-lg font-semibold text-primary">Files</h1>
        <button
          onClick={() => fileInputRef.current?.click()}
          className="p-1.5 rounded-md text-tertiary hover:text-accent hover:bg-elevated transition-colors"
          aria-label="Upload file"
        >
          <Upload size={16} />
        </button>
        <input
          ref={fileInputRef}
          type="file"
          className="hidden"
          onChange={(e) => {
            // Handle file upload from picker
            const file = e.target.files?.[0];
            if (file && rooms.length > 0) {
              matrixSendFile(rooms[0].room_id, (file as any).path ?? file.name)
                .then(() => toast({ type: "success", title: "File sent" }))
                .catch((err) => toast({ type: "error", title: "Upload failed", description: String(err) }));
            }
          }}
        />
      </div>

      {/* Search */}
      <div className="px-4 pb-3">
        <div className="relative">
          <Search size={14} className="absolute left-3 top-1/2 -translate-y-1/2 text-tertiary" />
          <input
            type="text"
            value={searchQuery}
            onChange={(e) => setSearchQuery(e.target.value)}
            placeholder="Search files..."
            className={cn(
              "w-full bg-surface border border-border-subtle rounded-lg",
              "pl-8 pr-3 py-2 text-sm text-primary",
              "placeholder:text-tertiary",
              "focus:outline-none focus:border-border-focus"
            )}
          />
        </div>
      </div>

      {/* Active transfers */}
      {activeTransfers.length > 0 && (
        <div className="px-4 pb-3 space-y-2">
          <p className="text-[10px] font-semibold text-tertiary uppercase tracking-wider">
            Active Transfers
          </p>
          {activeTransfers.map((t) => (
            <TransferItem key={t.id} transfer={t} />
          ))}
        </div>
      )}

      {/* Drag-drop overlay */}
      {dragOver && (
        <div className="absolute inset-0 z-30 flex items-center justify-center bg-accent/10 border-2 border-dashed border-accent rounded-xl m-2">
          <div className="text-center">
            <Upload size={32} className="text-accent mx-auto mb-2" />
            <p className="text-sm font-semibold text-accent">Drop files to upload</p>
            <p className="text-xs text-tertiary">Files will be encrypted end-to-end</p>
          </div>
        </div>
      )}

      {/* File list */}
      <div className="flex-1 overflow-y-auto px-2">
        {filteredFiles.length === 0 ? (
          <div className="flex flex-col items-center justify-center h-32 gap-2">
            <Package size={32} className="text-tertiary" />
            <p className="text-sm text-tertiary">No files shared yet</p>
            <p className="text-xs text-tertiary">Drag and drop files or use the upload button</p>
          </div>
        ) : (
          filteredFiles.map((file) => (
            <FileItem key={file.eventId} file={file} />
          ))
        )}
      </div>
    </div>
  );
}

function FileItem({ file }: { file: SharedFile }) {
  const Icon = getFileIcon(file.mimeType);
  const size = file.size ? formatBytes(file.size) : "Unknown size";
  const time = new Date(file.timestamp * 1000).toLocaleDateString([], {
    month: "short",
    day: "numeric",
    hour: "numeric",
    minute: "2-digit",
  });

  return (
    <div
      className={cn(
        "group flex items-center gap-3 px-3 py-2.5 rounded-lg",
        "hover:bg-elevated transition-colors"
      )}
    >
      <div className="w-9 h-9 rounded-lg bg-accent/10 flex items-center justify-center shrink-0">
        <Icon size={18} className="text-accent" />
      </div>
      <div className="flex-1 min-w-0">
        <p className="text-sm font-medium text-primary truncate">{file.filename}</p>
        <p className="text-[10px] text-tertiary">
          {file.sender.split(":")[0]?.replace("@", "")} · {file.roomName} · {time}
        </p>
      </div>
      <div className="flex items-center gap-1 shrink-0">
        <span className="text-[10px] text-tertiary">{size}</span>
        <button
          className="p-1 rounded-md text-tertiary hover:text-accent opacity-0 group-hover:opacity-100 transition-opacity"
          aria-label="Download"
        >
          <Download size={14} />
        </button>
      </div>
    </div>
  );
}

function TransferItem({ transfer }: { transfer: FileTransfer }) {
  const progress =
    transfer.totalBytes > 0
      ? Math.round((transfer.transferredBytes / transfer.totalBytes) * 100)
      : 0;
  const Icon = transfer.direction === "upload" ? Upload : Download;

  return (
    <div className="flex items-center gap-3 px-3 py-2 rounded-lg bg-surface border border-border-subtle">
      <Icon size={14} className="text-accent shrink-0" />
      <div className="flex-1 min-w-0">
        <p className="text-xs font-medium text-primary truncate">{transfer.filename}</p>
        <div className="w-full h-1.5 bg-elevated rounded-full mt-1">
          <div
            className="h-full bg-accent rounded-full transition-all"
            style={{ width: `${progress}%` }}
          />
        </div>
      </div>
      <span className="text-[10px] text-tertiary shrink-0">{progress}%</span>
    </div>
  );
}

function getFileIcon(mimeType: string | null) {
  if (!mimeType) return FileText;
  if (mimeType.startsWith("image/")) return Image;
  if (mimeType.startsWith("video/")) return Film;
  if (mimeType.startsWith("audio/")) return Music;
  return FileText;
}

function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  if (bytes < 1024 * 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  return `${(bytes / (1024 * 1024 * 1024)).toFixed(1)} GB`;
}
