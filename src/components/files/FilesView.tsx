import { useState, useCallback, useRef, useEffect } from "react";
import {
  FileText, Image, Film, Music, Package,
  Upload, Download, Search, Server, Trash2,
} from "lucide-react";
import { cn } from "@/lib/cn";
import { useFileStore, type SharedFile, type FileTransfer, type ServerFile } from "@/store/fileStore";
import { useChatStore } from "@/store/chatStore";
import { useMatrixStore } from "@/store/matrixStore";
import { useServerStore } from "@/store/serverStore";
import {
  matrixSendFile,
  paleServerGetFiles,
  paleServerUploadFile,
  paleServerDeleteFile,
} from "@/lib/tauri";
import { toast } from "@/components/ui/Toast";
import { MatrixLoginView } from "@/components/auth/MatrixLoginView";

type FileSource = "chat" | "server";

export function FilesView() {
  const authState = useMatrixStore((s) => s.authState);
  const { sharedFiles, transfers, serverFiles, setServerFiles, removeServerFile } = useFileStore();
  const rooms = useChatStore((s) => s.rooms);
  const { baseUrl, token, connected: serverConnected } = useServerStore();
  const [searchQuery, setSearchQuery] = useState("");
  const [dragOver, setDragOver] = useState(false);
  const [source, setSource] = useState<FileSource>(serverConnected ? "server" : "chat");
  const fileInputRef = useRef<HTMLInputElement>(null);
  const serverFileInputRef = useRef<HTMLInputElement>(null);

  // Load server files
  useEffect(() => {
    if (serverConnected && baseUrl && token) {
      paleServerGetFiles(baseUrl, token)
        .then(setServerFiles)
        .catch(() => {});
    }
  }, [serverConnected, baseUrl, token, setServerFiles]);

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

  if (authState !== "logged_in" && !serverConnected) {
    return <MatrixLoginView />;
  }

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
          onClick={() => {
            if (source === "server") {
              serverFileInputRef.current?.click();
            } else {
              fileInputRef.current?.click();
            }
          }}
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
            const file = e.target.files?.[0];
            if (file && rooms.length > 0) {
              matrixSendFile(rooms[0].room_id, (file as any).path ?? file.name)
                .then(() => toast({ type: "success", title: "File sent" }))
                .catch((err) => toast({ type: "error", title: "Upload failed", description: String(err) }));
            }
          }}
        />
        <input
          ref={serverFileInputRef}
          type="file"
          className="hidden"
          onChange={async (e) => {
            const file = e.target.files?.[0];
            if (file && baseUrl && token) {
              try {
                const uploaded = await paleServerUploadFile(baseUrl, token, file);
                setServerFiles([...serverFiles, uploaded]);
                toast({ type: "success", title: "File uploaded" });
              } catch (err) {
                toast({ type: "error", title: "Upload failed", description: String(err) });
              }
            }
          }}
        />
      </div>

      {/* Source tabs */}
      {serverConnected && (
        <div className="flex gap-1 px-4 pb-2">
          <button
            onClick={() => setSource("chat")}
            className={cn(
              "flex items-center gap-1 px-2.5 py-1 rounded-md text-xs font-medium transition-colors",
              source === "chat" ? "bg-accent-muted text-accent" : "text-tertiary hover:text-secondary hover:bg-elevated"
            )}
          >
            Chat Files
          </button>
          <button
            onClick={() => setSource("server")}
            className={cn(
              "flex items-center gap-1 px-2.5 py-1 rounded-md text-xs font-medium transition-colors",
              source === "server" ? "bg-accent-muted text-accent" : "text-tertiary hover:text-secondary hover:bg-elevated"
            )}
          >
            <Server size={12} />
            Server Files
          </button>
        </div>
      )}

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
        {source === "chat" ? (
          filteredFiles.length === 0 ? (
            <div className="flex flex-col items-center justify-center h-32 gap-2">
              <Package size={32} className="text-tertiary" />
              <p className="text-sm text-tertiary">No files shared yet</p>
              <p className="text-xs text-tertiary">Drag and drop files or use the upload button</p>
            </div>
          ) : (
            filteredFiles.map((file) => (
              <FileItem key={file.eventId} file={file} />
            ))
          )
        ) : (
          <ServerFileList
            files={serverFiles}
            searchQuery={searchQuery}
            baseUrl={baseUrl}
            token={token}
            onDelete={async (id) => {
              if (!baseUrl || !token) return;
              try {
                await paleServerDeleteFile(baseUrl, token, id);
                removeServerFile(id);
                toast({ type: "success", title: "File deleted" });
              } catch (err) {
                toast({ type: "error", title: "Delete failed", description: String(err) });
              }
            }}
          />
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

function ServerFileList({
  files,
  searchQuery,
  baseUrl,
  token,
  onDelete,
}: {
  files: ServerFile[];
  searchQuery: string;
  baseUrl: string | null;
  token: string | null;
  onDelete: (id: string) => void;
}) {
  const filtered = searchQuery
    ? files.filter((f) => f.filename.toLowerCase().includes(searchQuery.toLowerCase()))
    : files;

  if (filtered.length === 0) {
    return (
      <div className="flex flex-col items-center justify-center h-32 gap-2">
        <Server size={32} className="text-tertiary" />
        <p className="text-sm text-tertiary">No server files</p>
        <p className="text-xs text-tertiary">Upload files to the Pale server</p>
      </div>
    );
  }

  return (
    <>
      {filtered.map((file) => {
        const Icon = getFileIcon(file.content_type);
        const time = new Date(file.created_at).toLocaleDateString([], {
          month: "short",
          day: "numeric",
          hour: "numeric",
          minute: "2-digit",
        });

        return (
          <div
            key={file.id}
            className={cn(
              "group flex items-center gap-3 px-3 py-2.5 rounded-lg",
              "hover:bg-elevated transition-colors"
            )}
          >
            <div className="w-9 h-9 rounded-lg bg-accent/10 flex items-center justify-center shrink-0">
              <Icon size={18} className="text-accent" />
            </div>
            <div className="flex-1 min-w-0">
              <div className="flex items-center gap-2 min-w-0">
                <p className="text-sm font-medium text-primary truncate">{file.filename}</p>
                {file.legal_hold && (
                  <span className="shrink-0 rounded bg-amber-500/10 px-1.5 py-0.5 text-[10px] text-amber-500">
                    Hold
                  </span>
                )}
                {file.dlp_violation_count ? (
                  <span className="shrink-0 rounded bg-red-500/10 px-1.5 py-0.5 text-[10px] text-red-500">
                    DLP
                  </span>
                ) : null}
              </div>
              <p className="text-[10px] text-tertiary">
                {file.owner} &middot; {time}
                {file.dlp_status ? ` · ${file.dlp_status}` : ""}
              </p>
            </div>
            <div className="flex items-center gap-1 shrink-0">
              <span className="text-[10px] text-tertiary">{formatBytes(file.size)}</span>
              {baseUrl && token && (
                <a
                  href={`${baseUrl.replace(/\/+$/, "")}/v1/files/${file.id}`}
                  target="_blank"
                  rel="noopener noreferrer"
                  className="p-1 rounded-md text-tertiary hover:text-accent opacity-0 group-hover:opacity-100 transition-opacity"
                  title="Download"
                >
                  <Download size={14} />
                </a>
              )}
              <button
                onClick={() => onDelete(file.id)}
                className="p-1 rounded-md text-tertiary hover:text-destructive opacity-0 group-hover:opacity-100 transition-opacity"
                title="Delete"
              >
                <Trash2 size={14} />
              </button>
            </div>
          </div>
        );
      })}
    </>
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
