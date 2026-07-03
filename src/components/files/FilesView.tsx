import { useState, useCallback, useRef, useEffect } from "react";
import {
  FileText, Image, Film, Music, Package,
  Upload, Download, Search, Server, Trash2,
  Lock, Unlock, History, FolderPlus, Folder as FolderIcon, ChevronRight, Home,
} from "lucide-react";
import { cn } from "@/lib/cn";
import { useFileStore, type SharedFile, type FileTransfer, type ServerFile, type FileVersion, type Folder } from "@/store/fileStore";
import { useChatStore } from "@/store/chatStore";
import { useMatrixStore } from "@/store/matrixStore";
import { useServerStore } from "@/store/serverStore";
import {
  matrixSendFile,
  paleServerGetFiles,
  paleServerUploadFile,
  paleServerDeleteFile,
  paleServerApi,
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

  // Folder navigation state
  const [currentFolderId, setCurrentFolderId] = useState<string | null>(null);
  const [breadcrumb, setBreadcrumb] = useState<Folder[]>([]);
  const [folders, setFolders] = useState<Folder[]>([]);
  const [showNewFolder, setShowNewFolder] = useState(false);
  const [newFolderName, setNewFolderName] = useState("");

  // Version history state
  const [versionFileId, setVersionFileId] = useState<string | null>(null);
  const [versions, setVersions] = useState<FileVersion[]>([]);

  // Load server files
  useEffect(() => {
    if (serverConnected && baseUrl && token) {
      paleServerGetFiles(baseUrl, token)
        .then(setServerFiles)
        .catch(() => {});
    }
  }, [serverConnected, baseUrl, token, setServerFiles]);

  // Load folders when navigating (using first room as context)
  useEffect(() => {
    if (!serverConnected || !baseUrl || !token) return;
    const roomId = rooms[0]?.room_id;
    if (!roomId) return;
    const params = currentFolderId ? `?parent_id=${currentFolderId}` : "";
    paleServerApi<Folder[]>(baseUrl, token, `/v1/rooms/${roomId}/folders${params}`)
      .then(setFolders)
      .catch(() => setFolders([]));
  }, [serverConnected, baseUrl, token, rooms, currentFolderId]);

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

      const targetRoom = rooms[0];
      for (const file of files) {
        try {
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

  const navigateToFolder = (folder: Folder) => {
    setCurrentFolderId(folder.id);
    setBreadcrumb((prev) => [...prev, folder]);
  };

  const navigateToRoot = () => {
    setCurrentFolderId(null);
    setBreadcrumb([]);
  };

  const navigateToBreadcrumb = (index: number) => {
    if (index < 0) {
      navigateToRoot();
    } else {
      const folder = breadcrumb[index];
      setCurrentFolderId(folder.id);
      setBreadcrumb(breadcrumb.slice(0, index + 1));
    }
  };

  const createFolder = async () => {
    if (!baseUrl || !token || !newFolderName.trim()) return;
    const roomId = rooms[0]?.room_id;
    if (!roomId) return;
    try {
      const folder = await paleServerApi<Folder>(baseUrl, token, `/v1/rooms/${roomId}/folders`, {
        method: "POST",
        body: { name: newFolderName.trim(), parent_id: currentFolderId },
      });
      setFolders((prev) => [...prev, folder]);
      setNewFolderName("");
      setShowNewFolder(false);
      toast({ type: "success", title: "Folder created" });
    } catch (err) {
      toast({ type: "error", title: "Failed to create folder", description: String(err) });
    }
  };

  const loadVersionHistory = async (fileId: string) => {
    if (!baseUrl || !token) return;
    try {
      const vers = await paleServerApi<FileVersion[]>(baseUrl, token, `/v1/files/${fileId}/versions`);
      setVersions(vers);
      setVersionFileId(fileId);
    } catch (err) {
      toast({ type: "error", title: "Failed to load versions", description: String(err) });
    }
  };

  const toggleLock = async (file: ServerFile) => {
    if (!baseUrl || !token) return;
    const action = file.locked_by ? "unlock" : "lock";
    try {
      const updated = await paleServerApi<ServerFile>(baseUrl, token, `/v1/files/${file.id}/${action}`, { method: "POST" });
      setServerFiles(serverFiles.map((f) => (f.id === updated.id ? updated : f)));
      toast({ type: "success", title: file.locked_by ? "File unlocked" : "File locked" });
    } catch (err) {
      toast({ type: "error", title: `Failed to ${action}`, description: String(err) });
    }
  };

  if (authState !== "logged_in" && !serverConnected) {
    return <MatrixLoginView />;
  }

  // Version history modal
  if (versionFileId) {
    const file = serverFiles.find((f) => f.id === versionFileId);
    return (
      <div className="flex flex-col h-full">
        <div className="flex items-center justify-between px-4 pt-4 pb-2">
          <div className="flex items-center gap-2">
            <button onClick={() => setVersionFileId(null)} className="text-sm text-accent hover:underline">
              Back
            </button>
            <h1 className="text-lg font-semibold text-primary">Version History</h1>
          </div>
        </div>
        {file && <p className="px-4 pb-2 text-sm text-secondary">{file.filename}</p>}
        <div className="flex-1 overflow-y-auto px-4 space-y-2">
          {versions.length === 0 ? (
            <p className="text-sm text-tertiary py-8 text-center">No version history</p>
          ) : (
            versions.map((v) => (
              <div key={v.id} className="flex items-center justify-between p-3 rounded-lg bg-surface border border-border-subtle">
                <div>
                  <p className="text-sm font-medium text-primary">Version {v.version_number}</p>
                  <p className="text-[10px] text-tertiary">
                    {v.uploader} &middot; {new Date(v.created_at).toLocaleString()} &middot; {formatBytes(v.size)}
                  </p>
                </div>
                {baseUrl && token && (
                  <a
                    href={`${baseUrl.replace(/\/+$/, "")}/v1/files/${versionFileId}/versions/${v.version_number}`}
                    target="_blank"
                    rel="noopener noreferrer"
                    className="p-1.5 rounded-md text-tertiary hover:text-accent"
                    title="Download this version"
                  >
                    <Download size={14} />
                  </a>
                )}
              </div>
            ))
          )}
        </div>
      </div>
    );
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
        <div className="flex items-center gap-1">
          {source === "server" && (
            <button
              onClick={() => setShowNewFolder(!showNewFolder)}
              className="p-1.5 rounded-md text-tertiary hover:text-accent hover:bg-elevated transition-colors"
              aria-label="New folder"
            >
              <FolderPlus size={16} />
            </button>
          )}
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
        </div>
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

      {/* Breadcrumb navigation for folders */}
      {source === "server" && breadcrumb.length > 0 && (
        <div className="flex items-center gap-1 px-4 pb-2 text-xs text-tertiary overflow-x-auto">
          <button onClick={navigateToRoot} className="hover:text-accent flex items-center gap-0.5 shrink-0">
            <Home size={12} /> Root
          </button>
          {breadcrumb.map((folder, idx) => (
            <span key={folder.id} className="flex items-center gap-0.5 shrink-0">
              <ChevronRight size={10} />
              <button onClick={() => navigateToBreadcrumb(idx)} className="hover:text-accent">
                {folder.name}
              </button>
            </span>
          ))}
        </div>
      )}

      {/* New folder input */}
      {showNewFolder && source === "server" && (
        <div className="flex items-center gap-2 px-4 pb-2">
          <input
            type="text"
            value={newFolderName}
            onChange={(e) => setNewFolderName(e.target.value)}
            placeholder="Folder name"
            className="flex-1 bg-surface border border-border-subtle rounded-md px-2 py-1 text-sm text-primary"
            onKeyDown={(e) => e.key === "Enter" && createFolder()}
          />
          <button onClick={createFolder} className="px-2 py-1 rounded-md bg-accent text-inverse text-xs">Create</button>
          <button onClick={() => { setShowNewFolder(false); setNewFolderName(""); }} className="px-2 py-1 rounded-md text-xs text-tertiary hover:text-secondary">Cancel</button>
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
          <>
            {/* Folders */}
            {folders.map((folder) => (
              <div
                key={folder.id}
                onClick={() => navigateToFolder(folder)}
                className={cn(
                  "group flex items-center gap-3 px-3 py-2.5 rounded-lg cursor-pointer",
                  "hover:bg-elevated transition-colors"
                )}
              >
                <div className="w-9 h-9 rounded-lg bg-amber-500/10 flex items-center justify-center shrink-0">
                  <FolderIcon size={18} className="text-amber-500" />
                </div>
                <div className="flex-1 min-w-0">
                  <p className="text-sm font-medium text-primary truncate">{folder.name}</p>
                  <p className="text-[10px] text-tertiary">
                    {folder.created_by} &middot; {new Date(folder.created_at).toLocaleDateString()}
                  </p>
                </div>
                <ChevronRight size={14} className="text-tertiary shrink-0" />
              </div>
            ))}

            <ServerFileList
              files={serverFiles}
              searchQuery={searchQuery}
              baseUrl={baseUrl}
              token={token}
              folderId={currentFolderId}
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
              onVersionHistory={loadVersionHistory}
              onToggleLock={toggleLock}
            />
          </>
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
  folderId,
  onDelete,
  onVersionHistory,
  onToggleLock,
}: {
  files: ServerFile[];
  searchQuery: string;
  baseUrl: string | null;
  token: string | null;
  folderId: string | null;
  onDelete: (id: string) => void;
  onVersionHistory: (id: string) => void;
  onToggleLock: (file: ServerFile) => void;
}) {
  let filtered = searchQuery
    ? files.filter((f) => f.filename.toLowerCase().includes(searchQuery.toLowerCase()))
    : files.filter((f) => (f.folder_id ?? null) === folderId);

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
                {file.locked_by && (
                  <span className="shrink-0 rounded bg-blue-500/10 px-1.5 py-0.5 text-[10px] text-blue-500 flex items-center gap-0.5">
                    <Lock size={8} /> {file.locked_by}
                  </span>
                )}
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
              <button
                onClick={() => onVersionHistory(file.id)}
                className="p-1 rounded-md text-tertiary hover:text-accent opacity-0 group-hover:opacity-100 transition-opacity"
                title="Version History"
              >
                <History size={14} />
              </button>
              <button
                onClick={() => onToggleLock(file)}
                className="p-1 rounded-md text-tertiary hover:text-accent opacity-0 group-hover:opacity-100 transition-opacity"
                title={file.locked_by ? "Unlock" : "Lock"}
              >
                {file.locked_by ? <Unlock size={14} /> : <Lock size={14} />}
              </button>
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
