import { FormEvent, type ReactNode, useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  Activity,
  BarChart3,
  Archive,
  CheckCircle2,
  ClipboardList,
  Download,
  FileText,
  GitBranch,
  Lock,
  Mic,
  Plus,
  RadioTower,
  RefreshCw,
  Router,
  Save,
  Search,
  Server,
  Shield,
  Trash2,
  UserPlus,
  Upload,
  Users,
  type LucideIcon,
} from "lucide-react";
import { cn } from "@/lib/cn";
import {
  adminBaseUrl,
  adminLogin,
  adminLogout,
  createAdminSipAccount,
  createAdminUser,
  createConference,
  createRoutingRule,
  deleteAdminSipAccount,
  deleteAdminUser,
  deleteFile,
  deleteRoutingRule,
  loadAdminSnapshot,
  setAdminUserActive,
  setAdminSipAccountEnabled,
  updateRoutingRule,
  type AdminSnapshot,
} from "@/lib/adminApi";
import { toast } from "@/components/ui/Toast";
import { useServerStore } from "@/store/serverStore";
import { disconnectServer } from "@/lib/session";
import { paleServerApi, paleServerUploadFile } from "@/lib/tauri";
import type { ServerCollaborationPolicy } from "@/lib/tauri";

// Helper: all server calls go through Tauri invoke (not webview fetch)
async function api<T = any>(baseUrl: string, token: string, path: string, opts?: { method?: string; body?: unknown }): Promise<T> {
  return paleServerApi<T>(baseUrl, token, path, opts);
}

type AdminTab = "overview" | "users" | "sip" | "routing" | "ring_groups" | "ivr" | "queues" | "extensions" | "dids" | "hours" | "holidays" | "paging" | "media" | "calls" | "cdrs" | "agents" | "wallboard" | "qa" | "vip" | "conferences" | "files" | "directory" | "audit" | "cqd" | "policy" | "retention" | "dlp" | "barriers" | "labels" | "roles" | "packages" | "analytics" | "meeting_templates" | "recording_policies" | "hold_music" | "api_clients" | "bots" | "connectors";

const adminTabs: { id: AdminTab; label: string; icon: LucideIcon }[] = [
  { id: "overview", label: "Overview", icon: Activity },
  { id: "users", label: "Users", icon: Users },
  { id: "sip", label: "SIP", icon: Server },
  { id: "extensions", label: "Extensions", icon: Server },
  { id: "dids", label: "DIDs", icon: Router },
  { id: "routing", label: "Routing", icon: GitBranch },
  { id: "ring_groups", label: "Ring Groups", icon: Users },
  { id: "queues", label: "Queues", icon: Users },
  { id: "ivr", label: "IVR", icon: Router },
  { id: "hours", label: "Hours", icon: Activity },
  { id: "holidays", label: "Holidays", icon: Activity },
  { id: "paging", label: "Paging", icon: RadioTower },
  { id: "media", label: "Media", icon: RadioTower },
  { id: "calls", label: "Calls", icon: Router },
  { id: "cdrs", label: "CDR", icon: ClipboardList },
  { id: "agents", label: "Agents", icon: Users },
  { id: "wallboard", label: "Wallboard", icon: Activity },
  { id: "qa", label: "QA", icon: ClipboardList },
  { id: "vip", label: "VIP", icon: Users },
  { id: "conferences", label: "Conferences", icon: Mic },
  { id: "files", label: "Files", icon: FileText },
  { id: "directory", label: "Directory", icon: Users },
  { id: "audit", label: "Audit", icon: ClipboardList },
  { id: "cqd", label: "Call Quality", icon: BarChart3 },
  { id: "policy", label: "Policy", icon: Shield },
  { id: "retention", label: "Retention", icon: Archive },
  { id: "dlp", label: "DLP", icon: Shield },
  { id: "barriers", label: "Barriers", icon: Shield },
  { id: "labels", label: "Labels", icon: FileText },
  { id: "roles", label: "Roles", icon: Shield },
  { id: "packages", label: "Packages", icon: ClipboardList },
  { id: "analytics", label: "Analytics", icon: BarChart3 },
  { id: "meeting_templates", label: "Meeting Templates", icon: ClipboardList },
  { id: "recording_policies", label: "Rec. Policies", icon: Mic },
  { id: "hold_music", label: "Hold Music", icon: RadioTower },
  { id: "api_clients", label: "API Clients", icon: Shield },
  { id: "bots", label: "Bots", icon: Router },
  { id: "connectors", label: "Connectors", icon: GitBranch },
];

export function AdminView() {
  const serverBaseUrl = useServerStore((s) => s.baseUrl);
  const serverToken = useServerStore((s) => s.token);
  const [baseUrl] = useState(serverBaseUrl || adminBaseUrl());
  const [token, setToken] = useState(() => serverToken || sessionStorage.getItem("pale.admin.token") || "");
  const [username, setUsername] = useState("admin");
  const [password, setPassword] = useState("");
  const [activeTab, setActiveTab] = useState<AdminTab>("overview");
  const [snapshot, setSnapshot] = useState<AdminSnapshot | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const setServerConnection = useServerStore((s) => s.setConnection);

  // Sync token from serverStore if it changes (e.g. after wizard login)
  useEffect(() => {
    if (serverToken && serverToken !== token) {
      setToken(serverToken);
      sessionStorage.setItem("pale.admin.token", serverToken);
    }
  }, [serverToken, token]);

  const authenticated = token.length > 0;

  const refresh = useCallback(async () => {
    if (!token) return;
    setLoading(true);
    setError(null);
    try {
      setSnapshot(await loadAdminSnapshot(baseUrl, token));
    } catch (err) {
      const message = err instanceof Error ? err.message : "Unable to load admin data";
      setError(message);
      toast({ type: "error", title: message });
    } finally {
      setLoading(false);
    }
  }, [baseUrl, token]);

  useEffect(() => {
    if (authenticated) {
      refresh();
      setServerConnection(baseUrl, token);
    }
  }, [authenticated, baseUrl, refresh, setServerConnection, token]);

  // Auto-refresh via SSE events + polling fallback
  useEffect(() => {
    if (!authenticated || !token) return;
    // Instant refresh on SSE-triggered events
    const handler = () => refresh();
    window.addEventListener("pale:admin-refresh", handler);
    // Polling fallback every 30s
    const interval = window.setInterval(handler, 30000);
    return () => {
      window.removeEventListener("pale:admin-refresh", handler);
      window.clearInterval(interval);
    };
  }, [authenticated, refresh, token]);

  const onLogin = async (event: FormEvent) => {
    event.preventDefault();
    setLoading(true);
    setError(null);
    try {
      const session = await adminLogin(baseUrl, username, password);
      sessionStorage.setItem("pale.admin.token", session.token);
      setToken(session.token);
      setPassword("");
      setServerConnection(baseUrl, session.token, session.expires_at);
      toast({ type: "success", title: "Admin session started" });
    } catch (err) {
      const message = err instanceof Error ? err.message : "Login failed";
      setError(message);
      toast({ type: "error", title: message });
    } finally {
      setLoading(false);
    }
  };

  const totals = useMemo(
    () => ({
      users: snapshot?.users.length ?? 0,
      accounts: snapshot?.sipAccounts.length ?? 0,
      registrations: snapshot?.registrations.length ?? 0,
      online: snapshot?.presence.filter((p) => p.status !== "offline").length ?? 0,
      routes: snapshot?.routingRules.length ?? 0,
      calls: snapshot?.calls.length ?? 0,
      conferences: snapshot?.conferences.length ?? 0,
      subscriptions: snapshot?.subscriptions.length ?? 0,
    }),
    [snapshot]
  );

  if (!authenticated) {
    return (
      <div className="h-full bg-base text-primary p-4 md:p-6">
        <div className="max-w-[420px] mx-auto mt-8 border border-border-subtle bg-surface rounded-md p-4">
          <div className="flex items-center gap-3 mb-4">
            <div className="w-9 h-9 rounded-md bg-accent-muted text-accent flex items-center justify-center">
              <Lock size={18} />
            </div>
            <div>
              <h1 className="text-lg font-semibold">Admin</h1>
              <p className="text-sm text-secondary">Sign in to manage backend resources.</p>
            </div>
          </div>
          <form onSubmit={onLogin} className="space-y-3">
            <ReadOnlyField label="Server URL" value={baseUrl} />
            <Field label="Username" value={username} onChange={setUsername} />
            <Field label="Password" value={password} onChange={setPassword} type="password" />
            {error && <p className="text-sm text-destructive">{error}</p>}
            <button
              disabled={loading}
              className="w-full h-10 rounded-md bg-accent hover:bg-accent-hover text-white text-sm font-medium disabled:opacity-60"
            >
              {loading ? "Signing in..." : "Sign in"}
            </button>
          </form>
        </div>
      </div>
    );
  }

  return (
    <div className="h-full bg-base text-primary overflow-y-auto">
      <div className="p-4 md:p-6 space-y-4">
        <header className="flex flex-col md:flex-row md:items-center justify-between gap-3">
          <div>
            <h1 className="text-xl font-semibold">Admin</h1>
            <p className="text-sm text-secondary">{baseUrl}</p>
          </div>
          <div className="flex items-center gap-2">
            <button
              onClick={refresh}
              disabled={loading}
              className="h-9 px-3 rounded-md border border-border-default hover:bg-elevated text-sm flex items-center gap-2 disabled:opacity-60"
            >
              <RefreshCw size={16} className={loading ? "animate-spin" : ""} />
              Refresh
            </button>
            <button
              onClick={async () => {
                adminLogout(baseUrl, token).catch(() => {});
                setToken("");
                setSnapshot(null);
                // Clears the token, server connection, and stale presence/rooms/files
                disconnectServer();
              }}
              className="h-9 px-3 rounded-md border border-border-default hover:bg-elevated text-sm"
            >
              Sign out
            </button>
          </div>
        </header>

        {error && <div className="rounded-md border border-destructive/40 bg-destructive/10 p-3 text-sm text-destructive">{error}</div>}

        <div className="grid grid-cols-2 md:grid-cols-4 lg:grid-cols-8 gap-2">
          <Metric label="Users" value={totals.users} />
          <Metric label="SIP accounts" value={totals.accounts} />
          <Metric label="Registered" value={totals.registrations} />
          <Metric label="Online" value={totals.online} />
          <Metric label="Routes" value={totals.routes} />
          <Metric label="Calls" value={totals.calls} />
          <Metric label="Conferences" value={totals.conferences} />
          <Metric label="Subscriptions" value={totals.subscriptions} />
        </div>

        <div className="flex gap-1 overflow-x-auto border-b border-border-subtle">
          {adminTabs.map(({ id, label, icon: Icon }) => (
            <button
              key={id}
              onClick={() => setActiveTab(id)}
              className={cn(
                "h-10 px-3 text-sm flex items-center gap-2 border-b-2 shrink-0",
                activeTab === id
                  ? "border-accent text-accent"
                  : "border-transparent text-secondary hover:text-primary"
              )}
            >
              <Icon size={16} />
              {label}
            </button>
          ))}
        </div>

        {activeTab === "overview" && <Overview snapshot={snapshot} />}
        {activeTab === "users" && <UsersPanel baseUrl={baseUrl} token={token} snapshot={snapshot} onChange={refresh} />}
        {activeTab === "sip" && <SipPanel baseUrl={baseUrl} token={token} snapshot={snapshot} onChange={refresh} />}
        {activeTab === "routing" && <RoutingPanel baseUrl={baseUrl} token={token} snapshot={snapshot} onChange={refresh} />}
        {activeTab === "extensions" && <ExtensionsPanel baseUrl={baseUrl} token={token} />}
        {activeTab === "dids" && <DidsPanel baseUrl={baseUrl} token={token} />}
        {activeTab === "ring_groups" && <RingGroupsPanel baseUrl={baseUrl} token={token} />}
        {activeTab === "queues" && <QueuesPanel baseUrl={baseUrl} token={token} />}
        {activeTab === "ivr" && <IvrPanel baseUrl={baseUrl} token={token} />}
        {activeTab === "hours" && <CrudPanel baseUrl={baseUrl} token={token} endpoint="business-hours" title="Business Hours" icon={Activity} columns={["Name", "Timezone", "After Hours"]} rowFn={(h: any) => [h.name, h.timezone, h.after_hours_destination || "voicemail"]} fields={[["Name", "name"], ["Timezone", "timezone", "America/New_York"], ["After Hours Destination", "after_hours_destination"]]} extraJson={{ schedule: { mon: { open: "09:00", close: "17:00" }, tue: { open: "09:00", close: "17:00" }, wed: { open: "09:00", close: "17:00" }, thu: { open: "09:00", close: "17:00" }, fri: { open: "09:00", close: "17:00" } } }} />}
        {activeTab === "holidays" && <CrudPanel baseUrl={baseUrl} token={token} endpoint="holidays" title="Holidays" icon={Activity} columns={["Name", "Date", "Recurring", "Destination"]} rowFn={(h: any) => [h.name, h.date, h.recurring ? "Yes" : "No", h.destination || "-"]} fields={[["Name", "name"], ["Date (YYYY-MM-DD)", "date"], ["Destination", "destination"]]} extraJson={{ recurring: false }} />}
        {activeTab === "paging" && <CrudPanel baseUrl={baseUrl} token={token} endpoint="paging-groups" title="Paging Groups" icon={RadioTower} columns={["Name", "Extension", "Members"]} rowFn={(p: any) => [p.name, p.extension, (p.members || []).join(", ")]} fields={[["Name", "name"], ["Extension", "extension"], ["Members (comma-separated)", "members_csv"]]} transformSubmit={(d: any) => ({ ...d, members: (d.members_csv || "").split(",").map((m: string) => m.trim()).filter(Boolean), members_csv: undefined })} />}
        {activeTab === "media" && <MediaPanel snapshot={snapshot} />}
        {activeTab === "cdrs" && <CdrsPanel baseUrl={baseUrl} token={token} />}
        {activeTab === "agents" && <AgentsPanel baseUrl={baseUrl} token={token} />}
        {activeTab === "wallboard" && <WallboardPanel baseUrl={baseUrl} token={token} />}
        {activeTab === "qa" && <QaPanel baseUrl={baseUrl} token={token} />}
        {activeTab === "vip" && <VipCallersPanel baseUrl={baseUrl} token={token} />}
        {activeTab === "calls" && <CallsPanel snapshot={snapshot} />}
        {activeTab === "conferences" && <ConferencesPanel baseUrl={baseUrl} token={token} snapshot={snapshot} onChange={refresh} />}
        {activeTab === "files" && <FilesPanel baseUrl={baseUrl} token={token} snapshot={snapshot} onChange={refresh} />}
        {activeTab === "directory" && <DirectoryPanel baseUrl={baseUrl} token={token} />}
        {activeTab === "audit" && <AuditPanel baseUrl={baseUrl} token={token} snapshot={snapshot} />}
        {activeTab === "cqd" && <CqdPanel baseUrl={baseUrl} token={token} />}
        {activeTab === "policy" && <CollaborationPolicyPanel baseUrl={baseUrl} token={token} />}
        {activeTab === "retention" && <RetentionPanel baseUrl={baseUrl} token={token} />}
        {activeTab === "dlp" && <DlpPanel baseUrl={baseUrl} token={token} />}
        {activeTab === "barriers" && <BarriersPanel baseUrl={baseUrl} token={token} />}
        {activeTab === "labels" && <LabelsPanel baseUrl={baseUrl} token={token} />}
        {activeTab === "roles" && <RolesPanel baseUrl={baseUrl} token={token} />}
        {activeTab === "packages" && <PackagesPanel baseUrl={baseUrl} token={token} />}
        {activeTab === "analytics" && <AnalyticsPanel baseUrl={baseUrl} token={token} />}
        {activeTab === "meeting_templates" && <MeetingTemplatesPanel baseUrl={baseUrl} token={token} />}
        {activeTab === "recording_policies" && <RecordingPoliciesPanel baseUrl={baseUrl} token={token} />}
        {activeTab === "hold_music" && <HoldMusicPanel baseUrl={baseUrl} token={token} />}
        {activeTab === "api_clients" && <ApiClientsPanel baseUrl={baseUrl} token={token} />}
        {activeTab === "bots" && <BotsPanel baseUrl={baseUrl} token={token} />}
        {activeTab === "connectors" && <ConnectorsPanel baseUrl={baseUrl} token={token} />}
      </div>
    </div>
  );
}

function Overview({ snapshot }: { snapshot: AdminSnapshot | null }) {
  return (
    <div className="grid md:grid-cols-2 gap-3">
      <Table
        title="User presence"
        columns={["User", "Status", "Note"]}
        rows={(snapshot?.presence ?? [])
          .sort((a, b) => (a.status === "offline" ? 1 : -1) - (b.status === "offline" ? 1 : -1))
          .slice(0, 10)
          .map((p) => [p.sip_uri, p.status, p.note ?? "-"])}
      />
      <Table
        title="Recent registrations"
        columns={["AOR", "Contact", "Source"]}
        rows={(snapshot?.registrations ?? []).slice(0, 6).map((item) => [item.aor, item.contact, item.source])}
      />
      <Table
        title="Active routing"
        columns={["Priority", "Rule", "Target"]}
        rows={(snapshot?.routingRules ?? []).slice(0, 6).map((item) => [
          String(item.priority),
          item.name,
          item.enabled ? item.target : "disabled",
        ])}
      />
      <Table
        title="Active subscriptions"
        columns={["Subscriber", "Target", "Event", "Expires"]}
        rows={(snapshot?.subscriptions ?? []).slice(0, 6).map((item) => [
          item.subscriber,
          item.target,
          item.event,
          shortDate(item.expires_at),
        ])}
      />
    </div>
  );
}

function UsersPanel({
  baseUrl,
  token,
  snapshot,
  onChange,
}: {
  baseUrl: string;
  token: string;
  snapshot: AdminSnapshot | null;
  onChange: () => void;
}) {
  const [displayName, setDisplayName] = useState("");
  const [sipUri, setSipUri] = useState("");
  const [userPassword, setUserPassword] = useState("");
  const [role, setRole] = useState("user");
  const [mode, setMode] = useState<"provision" | "manual">("provision");
  const [extensionNumber, setExtensionNumber] = useState("");
  const [sipDomain, setSipDomain] = useState("pale.local");
  const [extensions, setExtensions] = useState<any[]>([]);

  useEffect(() => {
    api<any[]>(baseUrl, token, "/v1/extensions").then(setExtensions).catch(() => {});
  }, [baseUrl, token]);

  const nextExtension = () => {
    const used = new Set(extensions.map(e => parseInt(e.extension)).filter(n => !isNaN(n)));
    let next = 1001;
    while (used.has(next)) next++;
    return next.toString();
  };

  const submitProvision = async (event: FormEvent) => {
    event.preventDefault();
    try {
      await api(baseUrl, token, "/v1/users/provision", {
        method: "POST",
        body: {
          display_name: displayName,
          password: userPassword || undefined,
          role,
          extension_number: extensionNumber || undefined,
          sip_domain: sipDomain,
        },
      });
      setDisplayName("");
      setUserPassword("");
      setRole("user");
      setExtensionNumber("");
      toast({ type: "success", title: "User provisioned" });
      // Reload extensions for next-available calculation
      api<any[]>(baseUrl, token, "/v1/extensions").then(setExtensions).catch(() => {});
      onChange();
    } catch (err) {
      toast({ type: "error", title: err instanceof Error ? err.message : "Failed to provision user" });
    }
  };

  const submitManual = async (event: FormEvent) => {
    event.preventDefault();
    try {
      await createAdminUser(baseUrl, token, {
        display_name: displayName,
        sip_uri: sipUri,
        matrix_user_id: null,
        password: userPassword || undefined,
        role,
      });
      setDisplayName("");
      setSipUri("");
      setUserPassword("");
      setRole("user");
      toast({ type: "success", title: "User created" });
      onChange();
    } catch (err) {
      toast({ type: "error", title: err instanceof Error ? err.message : "Failed to create user" });
    }
  };

  const remove = async (id: string) => {
    try {
      await deleteAdminUser(baseUrl, token, id);
      toast({ type: "success", title: "User deactivated" });
      onChange();
    } catch (err) {
      toast({ type: "error", title: err instanceof Error ? err.message : "Failed to deactivate user" });
    }
  };

  const activate = async (id: string) => {
    try {
      await setAdminUserActive(baseUrl, token, id, true);
      toast({ type: "success", title: "User activated" });
      onChange();
    } catch (err) {
      toast({ type: "error", title: err instanceof Error ? err.message : "Failed to activate user" });
    }
  };

  const toggleRole = async (user: { id: string; sip_uri: string; display_name: string; matrix_user_id?: string | null }, currentRole: string) => {
    const newRole = currentRole === "admin" ? "user" : "admin";
    try {
      await api(baseUrl, token, `/v1/users/${user.id}/role`, {
        method: "PUT",
        body: { role: newRole },
      });
      toast({ type: "success", title: `${user.display_name} is now ${newRole}` });
      onChange();
    } catch (err) {
      toast({ type: "error", title: err instanceof Error ? err.message : "Failed to update role" });
    }
  };

  // Build user_id -> extension numbers map
  const extMap = new Map<string, string[]>();
  for (const ext of extensions) {
    if (ext.user_id) {
      const list = extMap.get(ext.user_id) || [];
      list.push(ext.extension);
      extMap.set(ext.user_id, list);
    }
  }

  return (
    <section className="border border-border-subtle bg-surface rounded-md overflow-hidden">
      <div className="p-3 border-b border-border-subtle flex items-center gap-2">
        <UserPlus size={17} className="text-accent" />
        <h2 className="font-medium">Users</h2>
      </div>

      {/* Mode toggle */}
      <div className="flex gap-1 border-b border-border-subtle mb-0 px-3 pt-1">
        <button type="button" onClick={() => setMode("provision")} className={cn("px-3 py-2 text-sm border-b-2", mode === "provision" ? "border-accent text-accent" : "border-transparent text-secondary")}>
          Quick Provision
        </button>
        <button type="button" onClick={() => setMode("manual")} className={cn("px-3 py-2 text-sm border-b-2", mode === "manual" ? "border-accent text-accent" : "border-transparent text-secondary")}>
          Manual Create
        </button>
      </div>

      {mode === "provision" ? (
        <form onSubmit={submitProvision} className="p-3 space-y-2 border-b border-border-subtle">
          <div className="grid md:grid-cols-3 gap-2">
            <Field label="Display name" value={displayName} onChange={setDisplayName} />
            <Field label="Password" value={userPassword} onChange={setUserPassword} type="password" />
            <label className="block">
              <span className="block text-xs text-tertiary mb-1">Role</span>
              <select
                value={role}
                onChange={(e) => setRole(e.target.value)}
                className="w-full h-10 rounded-md bg-base border border-border-default px-3 text-sm outline-none focus:border-border-focus"
              >
                <option value="user">User</option>
                <option value="admin">Admin</option>
              </select>
            </label>
          </div>
          <div className="grid md:grid-cols-3 gap-2">
            <div className="flex gap-1">
              <Field label="Extension number" value={extensionNumber} onChange={setExtensionNumber} />
              <button type="button" onClick={() => setExtensionNumber(nextExtension())} className="self-end h-10 px-2 rounded-md border border-border-default text-xs hover:bg-elevated">Suggest</button>
            </div>
            <Field label="SIP domain" value={sipDomain} onChange={setSipDomain} />
            <button className="h-10 self-end rounded-md bg-accent hover:bg-accent-hover text-white text-sm font-medium flex items-center justify-center gap-2">
              <Plus size={16} />
              Provision
            </button>
          </div>
        </form>
      ) : (
        <form onSubmit={submitManual} className="p-3 grid md:grid-cols-5 gap-2 border-b border-border-subtle">
          <Field label="Display name" value={displayName} onChange={setDisplayName} />
          <Field label="SIP URI" value={sipUri} onChange={setSipUri} />
          <Field label="Password" value={userPassword} onChange={setUserPassword} type="password" />
          <label className="block">
            <span className="block text-xs text-tertiary mb-1">Role</span>
            <select
              value={role}
              onChange={(e) => setRole(e.target.value)}
              className="w-full h-10 rounded-md bg-base border border-border-default px-3 text-sm outline-none focus:border-border-focus"
            >
              <option value="user">User</option>
              <option value="admin">Admin</option>
            </select>
          </label>
          <button className="h-10 self-end rounded-md bg-accent hover:bg-accent-hover text-white text-sm font-medium flex items-center justify-center gap-2">
            <Plus size={16} />
            Create user
          </button>
        </form>
      )}

      <div className="p-3 overflow-x-auto">
        <table className="w-full text-sm">
          <thead className="text-tertiary">
            <tr className="border-b border-border-subtle">
              {["Name", "Ext", "SIP URI", "Role", "Status", ""].map((header) => (
                <th key={header} className="text-left py-2 px-2 font-medium">{header}</th>
              ))}
            </tr>
          </thead>
          <tbody>
            {(snapshot?.users ?? []).map((user) => (
              <tr key={user.id} className="border-b border-border-subtle">
                <td className="py-2 px-2">{user.display_name}</td>
                <td className="py-2 px-2 font-mono text-xs text-secondary">{extMap.get(user.id)?.join(", ") || "-"}</td>
                <td className="py-2 px-2 text-secondary">{user.sip_uri}</td>
                <td className="py-2 px-2">
                  <span className={cn(
                    "px-2 py-0.5 rounded-full text-xs font-medium",
                    (user as any).role === "admin" ? "bg-accent/20 text-accent" : "bg-elevated text-secondary"
                  )}>
                    {(user as any).role || "user"}
                  </span>
                </td>
                <td className="py-2 px-2">
                  <span className={cn(
                    "px-2 py-0.5 rounded-full text-xs font-medium",
                    user.active === false ? "bg-destructive/10 text-destructive" : "bg-emerald-500/10 text-emerald-500"
                  )}>
                    {user.active === false ? "Inactive" : "Active"}
                  </span>
                </td>
                <td className="py-2 px-2 text-right">
                  <div className="inline-flex items-center gap-1">
                    <button
                      onClick={() => toggleRole(user, (user as any).role || "user")}
                      disabled={user.active === false}
                      className="h-8 px-2 rounded-md hover:bg-elevated text-xs text-secondary hover:text-primary"
                    >
                      {(user as any).role === "admin" ? "Demote" : "Promote"}
                    </button>
                    {user.active === false ? (
                      <button
                        onClick={() => activate(user.id)}
                        className="h-8 px-2 rounded-md hover:bg-elevated text-xs text-secondary hover:text-primary"
                      >
                        Activate
                      </button>
                    ) : (
                      <IconButton label="Deactivate user" tone="danger" onClick={() => remove(user.id)}>
                        <Trash2 size={16} />
                      </IconButton>
                    )}
                  </div>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </section>
  );
}

function SipPanel({
  baseUrl,
  token,
  snapshot,
  onChange,
}: {
  baseUrl: string;
  token: string;
  snapshot: AdminSnapshot | null;
  onChange: () => void;
}) {
  const [username, setUsername] = useState("");
  const [domain, setDomain] = useState("");
  const [password, setPassword] = useState("");
  const [displayName, setDisplayName] = useState("");

  const submit = async (event: FormEvent) => {
    event.preventDefault();
    try {
      await createAdminSipAccount(baseUrl, token, {
        username,
        domain,
        password,
        display_name: displayName || null,
      });
      setUsername("");
      setDomain("");
      setPassword("");
      setDisplayName("");
      toast({ type: "success", title: "SIP account created" });
      onChange();
    } catch (err) {
      toast({ type: "error", title: err instanceof Error ? err.message : "Failed to create SIP account" });
    }
  };

  const setEnabled = async (username: string, domain: string, enabled: boolean) => {
    try {
      await setAdminSipAccountEnabled(baseUrl, token, username, domain, enabled);
      toast({ type: "success", title: enabled ? "SIP account enabled" : "SIP account disabled" });
      onChange();
    } catch (err) {
      toast({ type: "error", title: err instanceof Error ? err.message : "Failed to update SIP account" });
    }
  };

  const remove = async (username: string, domain: string) => {
    try {
      await deleteAdminSipAccount(baseUrl, token, username, domain);
      toast({ type: "success", title: "SIP account deleted" });
      onChange();
    } catch (err) {
      toast({ type: "error", title: err instanceof Error ? err.message : "Failed to delete SIP account" });
    }
  };

  return (
    <div className="space-y-3">
      <PanelWithForm
        title="SIP accounts"
        icon={Server}
        onSubmit={submit}
        fields={[
          ["Username", username, setUsername],
          ["Domain", domain, setDomain],
          ["Password", password, setPassword, "password"],
          ["Display name", displayName, setDisplayName],
        ]}
        action="Create account"
      >
        <div className="overflow-x-auto">
          <table className="w-full text-sm">
            <thead className="text-tertiary">
              <tr className="border-b border-border-subtle">
                {["Username", "Domain", "Display", "Status", ""].map((header) => (
                  <th key={header} className="text-left py-2 px-2 font-medium">{header}</th>
                ))}
              </tr>
            </thead>
            <tbody>
              {(snapshot?.sipAccounts ?? []).map((account) => (
                <tr key={`${account.username}@${account.domain}`} className="border-b border-border-subtle">
                  <td className="py-2 px-2">{account.username}</td>
                  <td className="py-2 px-2 text-secondary">{account.domain}</td>
                  <td className="py-2 px-2">{account.display_name || "-"}</td>
                  <td className="py-2 px-2">{account.enabled ? "enabled" : "disabled"}</td>
                  <td className="py-2 px-2 text-right">
                    <div className="inline-flex items-center gap-1">
                      <button
                        onClick={() => setEnabled(account.username, account.domain, !account.enabled)}
                        className="h-8 px-2 rounded-md hover:bg-elevated text-xs text-secondary hover:text-primary"
                      >
                        {account.enabled ? "Disable" : "Enable"}
                      </button>
                      <IconButton
                        label="Delete SIP account"
                        tone="danger"
                        onClick={() => remove(account.username, account.domain)}
                      >
                        <Trash2 size={16} />
                      </IconButton>
                    </div>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      </PanelWithForm>
      <Table
        title="Registrations"
        columns={["AOR", "Contact", "Source", "Expires"]}
        rows={(snapshot?.registrations ?? []).map((item) => [
          item.aor,
          item.contact,
          item.source,
          shortDate(item.expires_at),
        ])}
      />
    </div>
  );
}

function RoutingPanel({
  baseUrl,
  token,
  snapshot,
  onChange,
}: {
  baseUrl: string;
  token: string;
  snapshot: AdminSnapshot | null;
  onChange: () => void;
}) {
  const [name, setName] = useState("");
  const [sourcePattern, setSourcePattern] = useState("*");
  const [destinationPattern, setDestinationPattern] = useState("sip:*");
  const [target, setTarget] = useState("");
  const [destinationType, setDestinationType] = useState("user");
  const [methodPattern, setMethodPattern] = useState("INVITE");
  const [headerConditions, setHeaderConditions] = useState("[]");
  const [headerActions, setHeaderActions] = useState("[]");
  const [stopProcessing, setStopProcessing] = useState(true);
  const [priority, setPriority] = useState("100");
  const [previewDirection, setPreviewDirection] = useState("inbound");
  const [previewSource, setPreviewSource] = useState("*");
  const [previewDestination, setPreviewDestination] = useState("");
  const [previewMethod, setPreviewMethod] = useState("INVITE");
  const [previewHeaders, setPreviewHeaders] = useState("[]");
  const [preview, setPreview] = useState<any | null>(null);

  const parseArray = (value: string, label: string) => {
    const parsed = JSON.parse(value || "[]");
    if (!Array.isArray(parsed)) throw new Error(`${label} must be a JSON array`);
    return parsed;
  };

  const submit = async (event: FormEvent) => {
    event.preventDefault();
    try {
      await createRoutingRule(baseUrl, token, {
        name,
        source_pattern: sourcePattern,
        destination_pattern: destinationPattern,
        target,
        destination_type: destinationType,
        method_pattern: methodPattern,
        header_conditions: parseArray(headerConditions, "Header conditions"),
        header_actions: parseArray(headerActions, "Header actions"),
        stop_processing: stopProcessing,
        priority: Number(priority),
        enabled: true,
      });
      setName("");
      setTarget("");
      setHeaderConditions("[]");
      setHeaderActions("[]");
      toast({ type: "success", title: "Routing rule created" });
      onChange();
    } catch (err) {
      toast({ type: "error", title: err instanceof Error ? err.message : "Failed to create routing rule" });
    }
  };

  const remove = async (id: string) => {
    try {
      await deleteRoutingRule(baseUrl, token, id);
      toast({ type: "success", title: "Routing rule removed" });
      onChange();
    } catch (err) {
      toast({ type: "error", title: err instanceof Error ? err.message : "Failed to delete routing rule" });
    }
  };

  const toggle = async (rule: NonNullable<AdminSnapshot["routingRules"]>[number]) => {
    try {
      await updateRoutingRule(baseUrl, token, rule.id, {
        name: rule.name,
        source_pattern: rule.source_pattern,
        destination_pattern: rule.destination_pattern,
        target: rule.target,
        destination_type: rule.destination_type,
        method_pattern: rule.method_pattern,
        header_conditions: rule.header_conditions,
        header_actions: rule.header_actions,
        stop_processing: rule.stop_processing,
        priority: rule.priority,
        enabled: !rule.enabled,
      });
      toast({ type: "success", title: !rule.enabled ? "Routing rule enabled" : "Routing rule disabled" });
      onChange();
    } catch (err) {
      toast({ type: "error", title: err instanceof Error ? err.message : "Failed to update routing rule" });
    }
  };

  const runPreview = async (event: FormEvent) => {
    event.preventDefault();
    try {
      const params = new URLSearchParams({
        direction: previewDirection,
        source: previewSource,
        destination: previewDestination,
        method: previewMethod,
        headers: previewHeaders,
      });
      setPreview(await api(baseUrl, token, `/v1/routes/preview?${params}`));
    } catch (err) {
      toast({ type: "error", title: err instanceof Error ? err.message : "Route preview failed" });
    }
  };

  return (
    <section className="border border-border-subtle bg-surface rounded-md overflow-hidden">
      <div className="p-3 border-b border-border-subtle flex items-center gap-2">
        <GitBranch size={17} className="text-accent" />
        <h2 className="font-medium">Routing rules</h2>
      </div>

      <form onSubmit={submit} className="p-3 border-b border-border-subtle space-y-3">
        <div className="grid md:grid-cols-5 gap-2">
          <Field label="Name" value={name} onChange={setName} />
          <Field label="Source pattern" value={sourcePattern} onChange={setSourcePattern} />
          <Field label="Destination pattern" value={destinationPattern} onChange={setDestinationPattern} />
          <Field label="Method pattern" value={methodPattern} onChange={setMethodPattern} />
          <Field label="Priority" value={priority} onChange={setPriority} type="number" />
        </div>
        <div className="grid md:grid-cols-5 gap-2">
          <label className="block">
            <span className="block text-xs text-tertiary mb-1">Destination type</span>
            <select value={destinationType} onChange={(event) => setDestinationType(event.target.value)}
              className="w-full h-10 rounded-md bg-base border border-border-default px-3 text-sm outline-none focus:border-border-focus">
              {["user", "ring_group", "queue", "ivr", "voicemail", "conference", "external"].map((value) => (
                <option key={value} value={value}>{value}</option>
              ))}
            </select>
          </label>
          <div className="md:col-span-3">
            <Field label="Target" value={target} onChange={setTarget} />
          </div>
          <label className="flex items-end gap-2 h-10 mt-5 text-sm text-secondary">
            <input type="checkbox" checked={stopProcessing} onChange={(event) => setStopProcessing(event.target.checked)} className="accent-accent" />
            Stop
          </label>
        </div>
        <div className="grid md:grid-cols-2 gap-2">
          <JsonField label="Header conditions" value={headerConditions} onChange={setHeaderConditions} />
          <JsonField label="Header actions" value={headerActions} onChange={setHeaderActions} />
        </div>
        <button className="h-10 px-3 rounded-md bg-accent hover:bg-accent-hover text-white text-sm font-medium inline-flex items-center justify-center gap-2">
          <Plus size={16} /> Add route
        </button>
      </form>

      <form onSubmit={runPreview} className="p-3 border-b border-border-subtle grid md:grid-cols-6 gap-2">
        <label className="block">
          <span className="block text-xs text-tertiary mb-1">Direction</span>
          <select value={previewDirection} onChange={(event) => setPreviewDirection(event.target.value)}
            className="w-full h-10 rounded-md bg-base border border-border-default px-3 text-sm outline-none focus:border-border-focus">
            <option value="inbound">inbound</option>
            <option value="outbound">outbound</option>
          </select>
        </label>
        <Field label="Source" value={previewSource} onChange={setPreviewSource} />
        <Field label="Destination" value={previewDestination} onChange={setPreviewDestination} />
        <Field label="Method" value={previewMethod} onChange={setPreviewMethod} />
        <div className="md:col-span-2">
          <JsonField label="Preview headers" value={previewHeaders} onChange={setPreviewHeaders} />
        </div>
        <button className="h-10 self-end rounded-md border border-border-default hover:bg-elevated text-sm font-medium">
          Preview
        </button>
        {preview && (
          <div className="md:col-span-6 rounded-md bg-base border border-border-subtle p-2 text-xs text-secondary">
            <span className="text-primary font-medium">{preview.resolved?.destination_type}</span>
            <span> to </span>
            <span className="font-mono">{preview.resolved?.destination}</span>
            {preview.matched_rule && <span> via {preview.matched_rule.name}</span>}
          </div>
        )}
      </form>

      <div className="p-3 overflow-x-auto">
        <table className="w-full text-sm">
          <thead className="text-tertiary">
            <tr className="border-b border-border-subtle">
              {["Priority", "Name", "Method", "Source", "Destination", "Target", "Headers", "Status", ""].map((header) => (
                <th key={header} className="text-left py-2 px-2 font-medium">{header}</th>
              ))}
            </tr>
          </thead>
          <tbody>
            {(snapshot?.routingRules ?? []).map((rule) => (
              <tr key={rule.id} className="border-b border-border-subtle">
                <td className="py-2 px-2">{rule.priority}</td>
                <td className="py-2 px-2">{rule.name}</td>
                <td className="py-2 px-2 text-secondary">{rule.method_pattern ?? "*"}</td>
                <td className="py-2 px-2 text-secondary">{rule.source_pattern}</td>
                <td className="py-2 px-2 text-secondary">{rule.destination_pattern}</td>
                <td className="py-2 px-2">{rule.destination_type ?? "user"}:{rule.target}</td>
                <td className="py-2 px-2 text-secondary">
                  {(rule.header_conditions?.length ?? 0) + (rule.header_actions?.length ?? 0)}
                </td>
                <td className="py-2 px-2">{rule.enabled ? "enabled" : "disabled"}</td>
                <td className="py-2 px-2 text-right">
                  <div className="inline-flex items-center gap-1">
                    <button
                      onClick={() => toggle(rule)}
                      className="h-8 px-2 rounded-md hover:bg-elevated text-xs text-secondary hover:text-primary"
                    >
                      {rule.enabled ? "Disable" : "Enable"}
                    </button>
                    <IconButton label="Delete route" tone="danger" onClick={() => remove(rule.id)}>
                      <Trash2 size={16} />
                    </IconButton>
                  </div>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </section>
  );
}

function CallsPanel({ snapshot }: { snapshot: AdminSnapshot | null }) {
  const activeCalls = (snapshot?.calls ?? []).filter((c) => c.status === "ringing" || c.status === "active" || c.status === "held");
  const activeDialogs = (snapshot?.dialogs ?? []).filter((d) => d.status === "ringing" || d.status === "routing" || d.status === "held");
  const endedCalls = (snapshot?.calls ?? []).filter((c) => c.status === "ended" || c.status === "failed");

  return (
    <div className="space-y-3">
      <section className="border border-border-subtle bg-surface rounded-md overflow-hidden">
        <div className="p-3 border-b border-border-subtle flex items-center gap-2">
          <span className="relative flex items-center justify-center w-2 h-2">
            <span className={cn("w-2 h-2 rounded-full", activeCalls.length > 0 ? "bg-success animate-pulse" : "bg-tertiary")} />
          </span>
          <h2 className="font-medium">Live Calls</h2>
          <span className="text-xs text-tertiary">({activeCalls.length} active)</span>
        </div>
        <div className="overflow-x-auto">
          <table className="w-full text-sm">
            <thead className="text-tertiary">
              <tr className="border-b border-border-subtle">
                {["Caller", "Callee(s)", "Media", "Status", "Duration"].map((h) => (
                  <th key={h} className="text-left py-2 px-3 font-medium">{h}</th>
                ))}
              </tr>
            </thead>
            <tbody>
              {activeCalls.length === 0 ? (
                <tr><td colSpan={5} className="py-6 px-3 text-center text-secondary">No active calls</td></tr>
              ) : activeCalls.map((call) => (
                <tr key={call.id} className="border-b border-border-subtle">
                  <td className="py-2 px-3">{call.caller}</td>
                  <td className="py-2 px-3 text-secondary">{call.callees.join(", ")}</td>
                  <td className="py-2 px-3">
                    {call.media.map((m: string) => (
                      <span key={m} className="inline-block mr-1 px-1.5 py-0.5 rounded bg-accent/20 text-accent text-xs">{m}</span>
                    ))}
                  </td>
                  <td className="py-2 px-3">
                    <span className={cn(
                      "px-2 py-0.5 rounded-full text-xs font-medium",
                      call.status === "active" ? "bg-success/20 text-success" :
                      call.status === "ringing" ? "bg-warning/20 text-warning" :
                      "bg-accent/20 text-accent"
                    )}>
                      {call.status === "active" ? "In Progress" : call.status === "ringing" ? "Ringing" : "On Hold"}
                    </span>
                  </td>
                  <td className="py-2 px-3 text-secondary tabular-nums">{shortDate(call.updated_at)}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      </section>

      <section className="border border-border-subtle bg-surface rounded-md overflow-hidden">
        <div className="p-3 border-b border-border-subtle flex items-center gap-2">
          <h2 className="font-medium">Active SIP Dialogs</h2>
          <span className="text-xs text-tertiary">({activeDialogs.length})</span>
        </div>
        <div className="overflow-x-auto">
          <table className="w-full text-sm">
            <thead className="text-tertiary">
              <tr className="border-b border-border-subtle">
                {["Call-ID", "From", "To", "Status", "Media"].map((h) => (
                  <th key={h} className="text-left py-2 px-3 font-medium">{h}</th>
                ))}
              </tr>
            </thead>
            <tbody>
              {activeDialogs.length === 0 ? (
                <tr><td colSpan={5} className="py-4 px-3 text-secondary">No active dialogs</td></tr>
              ) : activeDialogs.map((d) => (
                <tr key={d.call_id} className="border-b border-border-subtle">
                  <td className="py-2 px-3 font-mono text-xs max-w-[120px] truncate">{d.call_id}</td>
                  <td className="py-2 px-3">{d.from_uri}</td>
                  <td className="py-2 px-3">{d.to_uri}</td>
                  <td className="py-2 px-3">
                    <span className={cn(
                      "px-2 py-0.5 rounded-full text-xs font-medium",
                      d.status === "ringing" ? "bg-warning/20 text-warning" :
                      d.status === "held" ? "bg-accent/20 text-accent" :
                      "bg-elevated text-secondary"
                    )}>
                      {d.status}
                    </span>
                  </td>
                  <td className="py-2 px-3 text-secondary">{(d as any).media_types?.join(", ") || "-"}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      </section>

      {endedCalls.length > 0 && (
        <Table
          title={`Recent ended calls (${endedCalls.length})`}
          columns={["Caller", "Callees", "Status", "Ended"]}
          rows={endedCalls.slice(0, 20).map((call) => [
            call.caller,
            call.callees.join(", "),
            call.status,
            shortDate(call.updated_at),
          ])}
        />
      )}
    </div>
  );
}

function MediaPanel({ snapshot }: { snapshot: AdminSnapshot | null }) {
  const media = snapshot?.mediaConfig;
  return (
    <div className="grid md:grid-cols-2 gap-3">
      <Table
        title="NAT traversal"
        columns={["Setting", "Value"]}
        rows={[
          ["ICE", media?.ice_enabled ? "enabled" : "disabled"],
          ["STUN failure policy", media?.stun_ignore_failure ? "ignore failures" : "fail startup"],
          ["STUN servers", media?.stun_servers.length ? media.stun_servers.join(", ") : "not configured"],
        ]}
      />
      <Table
        title="TURN relay"
        columns={["Setting", "Value"]}
        rows={[
          ["Server", media?.turn?.server ?? "not configured"],
          ["Transport", media?.turn?.transport ?? "-"],
          ["Username", media?.turn?.username ?? "-"],
          ["Realm", media?.turn?.realm ?? "-"],
        ]}
      />
    </div>
  );
}

function AuditPanel({ baseUrl, token, snapshot }: { baseUrl: string; token: string; snapshot: AdminSnapshot | null }) {
  const [events, setEvents] = useState<AdminSnapshot["auditEvents"]>([]);
  const [principal, setPrincipal] = useState("");
  const [action, setAction] = useState("");
  const [target, setTarget] = useState("");
  const [limit, setLimit] = useState("250");
  const [loading, setLoading] = useState(false);

  useEffect(() => {
    setEvents(snapshot?.auditEvents ?? []);
  }, [snapshot?.auditEvents]);

  const queryString = useCallback(() => {
    const params = new URLSearchParams();
    if (principal.trim()) params.set("principal", principal.trim());
    if (action.trim()) params.set("action", action.trim());
    if (target.trim()) params.set("target", target.trim());
    if (limit.trim()) params.set("limit", limit.trim());
    const query = params.toString();
    return query ? `?${query}` : "";
  }, [action, limit, principal, target]);

  const load = useCallback(async () => {
    if (!token) return;
    setLoading(true);
    try {
      setEvents(await api<AdminSnapshot["auditEvents"]>(baseUrl, token, `/v1/admin/audit${queryString()}`));
    } catch (err) {
      toast({ type: "error", title: err instanceof Error ? err.message : "Unable to load audit events" });
    } finally {
      setLoading(false);
    }
  }, [baseUrl, queryString, token]);

  const downloadCsv = async () => {
    try {
      const response = await fetch(`${baseUrl.replace(/\/$/, "")}/v1/admin/audit/export.csv${queryString()}`, {
        headers: { Authorization: `Bearer ${token}` },
      });
      if (!response.ok) throw new Error(`Export failed (${response.status})`);
      const blob = await response.blob();
      const url = URL.createObjectURL(blob);
      const a = document.createElement("a");
      a.href = url;
      a.download = `audit-log-${new Date().toISOString().slice(0, 10)}.csv`;
      a.click();
      URL.revokeObjectURL(url);
    } catch (err) {
      toast({ type: "error", title: err instanceof Error ? err.message : "Unable to export audit log" });
    }
  };

  return (
    <div className="space-y-3">
      <div className="rounded-md border border-border-subtle bg-surface p-3">
        <div className="grid grid-cols-1 md:grid-cols-[1fr_1fr_1fr_100px_auto_auto] gap-2">
          <input
            value={principal}
            onChange={(event) => setPrincipal(event.target.value)}
            placeholder="Principal"
            className="h-10 rounded-md bg-base border border-border-default px-3 text-sm outline-none focus:border-border-focus"
          />
          <input
            value={action}
            onChange={(event) => setAction(event.target.value)}
            placeholder="Action"
            className="h-10 rounded-md bg-base border border-border-default px-3 text-sm outline-none focus:border-border-focus"
          />
          <input
            value={target}
            onChange={(event) => setTarget(event.target.value)}
            placeholder="Target"
            className="h-10 rounded-md bg-base border border-border-default px-3 text-sm outline-none focus:border-border-focus"
          />
          <input
            value={limit}
            onChange={(event) => setLimit(event.target.value)}
            inputMode="numeric"
            placeholder="Limit"
            className="h-10 rounded-md bg-base border border-border-default px-3 text-sm outline-none focus:border-border-focus"
          />
          <button
            onClick={() => load()}
            disabled={loading}
            className="h-10 px-3 rounded-md border border-border-default hover:bg-elevated text-sm disabled:opacity-60"
          >
            {loading ? "Loading" : "Refresh"}
          </button>
          <button
            onClick={downloadCsv}
            className="h-10 px-3 rounded-md bg-accent hover:bg-accent-hover text-white text-sm inline-flex items-center justify-center gap-2"
          >
            <Download size={15} />
            CSV
          </button>
        </div>
      </div>
      <Table
        title="Audit log"
        columns={["Time", "Principal", "Action", "Target"]}
        rows={events.map((event) => [
          shortDate(event.created_at),
          event.principal,
          event.action,
          event.target ?? "-",
        ])}
      />
    </div>
  );
}

function RingGroupsPanel({ baseUrl, token }: { baseUrl: string; token: string }) {
  const [groups, setGroups] = useState<any[]>([]);
  const [name, setName] = useState("");
  const [extension, setExtension] = useState("");
  const [strategy, setStrategy] = useState("simultaneous");
  const [members, setMembers] = useState("");
  const [fallback, setFallback] = useState("");

  const load = useCallback(async () => {
    try {
      const data = await api<any[]>(baseUrl, token, "/v1/ring-groups");
      setGroups(data);
    } catch { /* ignore */ }
  }, [baseUrl, token]);

  useEffect(() => { load(); }, [load]);

  const submit = async (event: FormEvent) => {
    event.preventDefault();
    try {
      await api(baseUrl, token, "/v1/ring-groups", {
        method: "POST",
        body: {
          name,
          extension: extension.startsWith("sip:") ? extension : `sip:${extension}`,
          strategy,
          members: members.split(",").map((m) => m.trim()).filter(Boolean).map((m) => m.startsWith("sip:") ? m : `sip:${m}`),
          fallback_uri: fallback || null,
        },
      });
      setName(""); setExtension(""); setMembers(""); setFallback("");
      toast({ type: "success", title: "Ring group created" });
      load();
    } catch (err) {
      toast({ type: "error", title: err instanceof Error ? err.message : "Failed" });
    }
  };

  const remove = async (id: string) => {
    try {
      await api(baseUrl, token, `/v1/ring-groups/${id}`, { method: "DELETE" });
      toast({ type: "success", title: "Ring group deleted" });
      load();
    } catch { toast({ type: "error", title: "Failed to delete" }); }
  };

  return (
    <section className="border border-border-subtle bg-surface rounded-md overflow-hidden">
      <div className="p-3 border-b border-border-subtle flex items-center gap-2">
        <Users size={17} className="text-accent" />
        <h2 className="font-medium">Ring Groups</h2>
      </div>
      <form onSubmit={submit} className="p-3 grid md:grid-cols-6 gap-2 border-b border-border-subtle">
        <Field label="Name" value={name} onChange={setName} />
        <Field label="Extension" value={extension} onChange={setExtension} />
        <label className="block">
          <span className="block text-xs text-tertiary mb-1">Strategy</span>
          <select value={strategy} onChange={(e) => setStrategy(e.target.value)}
            className="w-full h-10 rounded-md bg-base border border-border-default px-3 text-sm outline-none focus:border-border-focus">
            <option value="simultaneous">Ring All</option>
            <option value="sequential">Sequential</option>
            <option value="random">Random</option>
          </select>
        </label>
        <Field label="Members (SIP URIs)" value={members} onChange={setMembers} />
        <Field label="Fallback URI" value={fallback} onChange={setFallback} />
        <button className="h-10 self-end rounded-md bg-accent hover:bg-accent-hover text-white text-sm font-medium flex items-center justify-center gap-2">
          <Plus size={16} /> Create
        </button>
      </form>
      <div className="p-3 overflow-x-auto">
        <table className="w-full text-sm">
          <thead className="text-tertiary">
            <tr className="border-b border-border-subtle">
              {["Name", "Extension", "Strategy", "Members", "Fallback", ""].map((h) => (
                <th key={h} className="text-left py-2 px-2 font-medium">{h}</th>
              ))}
            </tr>
          </thead>
          <tbody>
            {groups.length === 0 ? (
              <tr><td colSpan={6} className="py-4 px-2 text-secondary">No ring groups</td></tr>
            ) : groups.map((g) => (
              <tr key={g.id} className="border-b border-border-subtle">
                <td className="py-2 px-2">{g.name}</td>
                <td className="py-2 px-2 text-secondary">{g.extension}</td>
                <td className="py-2 px-2">{g.strategy}</td>
                <td className="py-2 px-2 text-secondary max-w-[200px] truncate">{(g.members || []).join(", ")}</td>
                <td className="py-2 px-2 text-secondary">{g.fallback_uri || "-"}</td>
                <td className="py-2 px-2 text-right">
                  <IconButton label="Delete" tone="danger" onClick={() => remove(g.id)}><Trash2 size={16} /></IconButton>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </section>
  );
}

function IvrPanel({ baseUrl, token }: { baseUrl: string; token: string }) {
  const [ivrs, setIvrs] = useState<any[]>([]);
  const [name, setName] = useState("");
  const [extension, setExtension] = useState("");
  const [greeting, setGreeting] = useState("");
  const [options, setOptions] = useState<{ digit: string; label: string; destination: string; destination_type: string }[]>([
    { digit: "1", label: "Sales", destination: "", destination_type: "ring_group" },
    { digit: "2", label: "Support", destination: "", destination_type: "ring_group" },
    { digit: "0", label: "Operator", destination: "", destination_type: "user" },
  ]);
  const [timeoutDest, setTimeoutDest] = useState("");
  const [invalidDest, setInvalidDest] = useState("");
  const [greetingMode, setGreetingMode] = useState<"text" | "upload">("text");
  const [greetingFileId, setGreetingFileId] = useState<string | null>(null);
  const [uploading, setUploading] = useState(false);
  const uploadGreeting = async (file: File) => {
    setUploading(true);
    try {
      const record = await paleServerUploadFile(baseUrl, token, file);
      setGreetingFileId(record.id);
      toast({ type: "success", title: `Uploaded: ${file.name}` });
    } catch (err) {
      toast({ type: "error", title: err instanceof Error ? err.message : "Upload failed" });
    }
    setUploading(false);
  };

  const load = useCallback(async () => {
    try {
      const data = await api<any[]>(baseUrl, token, "/v1/ivrs");
      setIvrs(data);
    } catch { /* ignore */ }
  }, [baseUrl, token]);

  useEffect(() => { load(); }, [load]);

  const addOption = () => {
    setOptions([...options, { digit: String(options.length + 1), label: "", destination: "", destination_type: "user" }]);
  };

  const updateOption = (idx: number, field: string, value: string) => {
    setOptions(options.map((o, i) => i === idx ? { ...o, [field]: value } : o));
  };

  const removeOption = (idx: number) => {
    setOptions(options.filter((_, i) => i !== idx));
  };

  const submit = async (event: FormEvent) => {
    event.preventDefault();
    try {
      await api(baseUrl, token, "/v1/ivrs", {
        method: "POST",
        body: {
          name,
          extension: extension.startsWith("sip:") ? extension : `sip:${extension}`,
          greeting_text: greetingMode === "text"
            ? (greeting || "Welcome. " + options.map((o) => `Press ${o.digit} for ${o.label}`).join(". ") + ".")
            : (greetingFileId ? `[audio:${greetingFileId}]` : "Welcome."),
          greeting_file_id: greetingMode === "upload" ? greetingFileId : null,
          timeout_destination: timeoutDest || null,
          invalid_destination: invalidDest || null,
          options: options.filter((o) => o.destination).map((o) => ({
            ...o,
            destination: o.destination.startsWith("sip:") ? o.destination : `sip:${o.destination}`,
          })),
        },
      });
      setName(""); setExtension(""); setGreeting("");
      toast({ type: "success", title: "IVR created" });
      load();
    } catch (err) {
      toast({ type: "error", title: err instanceof Error ? err.message : "Failed" });
    }
  };

  const remove = async (id: string) => {
    try {
      await api(baseUrl, token, `/v1/ivrs/${id}`, { method: "DELETE" });
      toast({ type: "success", title: "IVR deleted" });
      load();
    } catch { toast({ type: "error", title: "Failed to delete" }); }
  };

  return (
    <section className="border border-border-subtle bg-surface rounded-md overflow-hidden">
      <div className="p-3 border-b border-border-subtle flex items-center gap-2">
        <Router size={17} className="text-accent" />
        <h2 className="font-medium">IVR / Auto-Attendant</h2>
      </div>
      <form onSubmit={submit} className="p-3 space-y-3 border-b border-border-subtle">
        <div className="grid md:grid-cols-2 gap-2">
          <Field label="Name" value={name} onChange={setName} />
          <Field label="Extension (e.g. main@pale.local)" value={extension} onChange={setExtension} />
        </div>

        <div>
          <div className="flex items-center gap-2 mb-2">
            <span className="text-xs font-semibold text-tertiary uppercase tracking-wider">Greeting</span>
            <div className="flex gap-1">
              <button type="button" onClick={() => setGreetingMode("text")}
                className={cn("px-2 py-0.5 text-xs rounded-md", greetingMode === "text" ? "bg-accent-muted text-accent" : "text-tertiary hover:text-secondary")}>
                Text-to-Speech
              </button>
              <button type="button" onClick={() => setGreetingMode("upload")}
                className={cn("px-2 py-0.5 text-xs rounded-md", greetingMode === "upload" ? "bg-accent-muted text-accent" : "text-tertiary hover:text-secondary")}>
                Upload Audio
              </button>
            </div>
          </div>
          {greetingMode === "text" ? (
            <textarea
              value={greeting}
              onChange={(e) => setGreeting(e.target.value)}
              placeholder="Welcome to our company. Press 1 for sales, press 2 for support..."
              rows={2}
              className="w-full rounded-md bg-base border border-border-default px-3 py-2 text-sm outline-none focus:border-border-focus resize-none"
            />
          ) : (
            <div className="flex items-center gap-3">
              <input
                type="file"
                accept="audio/*,.wav,.mp3,.ogg"
                onChange={(e) => {
                  const file = e.target.files?.[0];
                  if (file) uploadGreeting(file);
                }}
                className="text-sm text-secondary file:mr-3 file:py-1.5 file:px-3 file:rounded-md file:border-0 file:text-sm file:font-medium file:bg-accent file:text-white file:cursor-pointer hover:file:bg-accent-hover"
              />
              {uploading && <span className="text-xs text-tertiary">Uploading...</span>}
              {greetingFileId && !uploading && (
                <div className="flex items-center gap-2">
                  <span className="text-xs text-success">Uploaded</span>
                  <audio controls className="h-8" src={`${baseUrl}/v1/files/${greetingFileId}`} />
                </div>
              )}
            </div>
          )}
        </div>

        <div>
          <div className="flex items-center justify-between mb-2">
            <span className="text-xs font-semibold text-tertiary uppercase tracking-wider">Menu Options</span>
            <button type="button" onClick={addOption} className="text-xs text-accent hover:underline">+ Add option</button>
          </div>
          <div className="space-y-2">
            {options.map((opt, idx) => (
              <div key={idx} className="grid grid-cols-5 gap-2 items-end">
                <label className="block">
                  <span className="block text-xs text-tertiary mb-1">Digit</span>
                  <input value={opt.digit} onChange={(e) => updateOption(idx, "digit", e.target.value)}
                    className="w-full h-10 rounded-md bg-base border border-border-default px-3 text-sm outline-none focus:border-border-focus" />
                </label>
                <label className="block">
                  <span className="block text-xs text-tertiary mb-1">Label</span>
                  <input value={opt.label} onChange={(e) => updateOption(idx, "label", e.target.value)}
                    className="w-full h-10 rounded-md bg-base border border-border-default px-3 text-sm outline-none focus:border-border-focus" />
                </label>
                <label className="block">
                  <span className="block text-xs text-tertiary mb-1">Destination</span>
                  <input value={opt.destination} onChange={(e) => updateOption(idx, "destination", e.target.value)}
                    placeholder="user@pale.local or group extension"
                    className="w-full h-10 rounded-md bg-base border border-border-default px-3 text-sm outline-none focus:border-border-focus" />
                </label>
                <label className="block">
                  <span className="block text-xs text-tertiary mb-1">Type</span>
                  <select value={opt.destination_type} onChange={(e) => updateOption(idx, "destination_type", e.target.value)}
                    className="w-full h-10 rounded-md bg-base border border-border-default px-3 text-sm outline-none focus:border-border-focus">
                    <option value="user">User</option>
                    <option value="ring_group">Ring Group</option>
                    <option value="ivr">Sub-IVR</option>
                    <option value="voicemail">Voicemail</option>
                    <option value="external">External</option>
                  </select>
                </label>
                <button type="button" onClick={() => removeOption(idx)}
                  className="h-10 px-2 rounded-md hover:bg-elevated text-tertiary hover:text-destructive text-xs">Remove</button>
              </div>
            ))}
          </div>
        </div>

        <div className="grid md:grid-cols-2 gap-2">
          <Field label="Timeout destination (no input)" value={timeoutDest} onChange={setTimeoutDest} />
          <Field label="Invalid input destination" value={invalidDest} onChange={setInvalidDest} />
        </div>

        <button className="h-10 rounded-md bg-accent hover:bg-accent-hover text-white text-sm font-medium flex items-center justify-center gap-2 px-4">
          <Plus size={16} /> Create IVR
        </button>
      </form>
      <div className="p-3 overflow-x-auto">
        <table className="w-full text-sm">
          <thead className="text-tertiary">
            <tr className="border-b border-border-subtle">
              {["Name", "Extension", "Greeting", "Options", ""].map((h) => (
                <th key={h} className="text-left py-2 px-2 font-medium">{h}</th>
              ))}
            </tr>
          </thead>
          <tbody>
            {ivrs.length === 0 ? (
              <tr><td colSpan={5} className="py-4 px-2 text-secondary">No IVRs configured</td></tr>
            ) : ivrs.map((ivr) => (
              <tr key={ivr.id} className="border-b border-border-subtle">
                <td className="py-2 px-2">{ivr.name}</td>
                <td className="py-2 px-2 text-secondary">{ivr.extension}</td>
                <td className="py-2 px-2 text-secondary max-w-[200px]">
                  {ivr.greeting_file_id ? (
                    <div className="flex items-center gap-2">
                      <span className="text-xs bg-accent/20 text-accent px-1.5 py-0.5 rounded">Audio</span>
                      <audio controls className="h-7" src={`${baseUrl}/v1/files/${ivr.greeting_file_id}`} />
                    </div>
                  ) : (
                    <span className="truncate block">{ivr.greeting_text}</span>
                  )}
                </td>
                <td className="py-2 px-2">
                  {(ivr.options || []).map((o: any) => (
                    <span key={o.digit} className="inline-block mr-1 px-1.5 py-0.5 rounded bg-elevated text-xs">
                      {o.digit}: {o.label || o.destination}
                    </span>
                  ))}
                </td>
                <td className="py-2 px-2 text-right">
                  <IconButton label="Delete IVR" tone="danger" onClick={() => remove(ivr.id)}><Trash2 size={16} /></IconButton>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </section>
  );
}

function CrudPanel({ baseUrl, token, endpoint, title, icon: Icon, columns, rowFn, fields, extraJson, transformSubmit }: {
  baseUrl: string; token: string; endpoint: string; title: string; icon: LucideIcon;
  columns: string[]; rowFn: (item: any) => string[];
  fields: [string, string, string?][]; // [label, key, default?]
  extraJson?: Record<string, any>;
  transformSubmit?: (data: any) => any;
}) {
  const [items, setItems] = useState<any[]>([]);
  const [form, setForm] = useState<Record<string, string>>({});

  const load = useCallback(async () => {
    try {
      const data = await api<any[]>(baseUrl, token, `/v1/${endpoint}`);
      setItems(data);
    } catch { /* ignore */ }
  }, [baseUrl, endpoint, token]);

  useEffect(() => { load(); }, [load]);

  const submit = async (event: FormEvent) => {
    event.preventDefault();
    try {
      let body: any = { ...form, ...(extraJson || {}) };
      if (transformSubmit) body = transformSubmit(body);
      await api(baseUrl, token, `/v1/${endpoint}`, {
        method: "POST",
        body,
      });
      setForm({});
      toast({ type: "success", title: `${title} created` });
      load();
    } catch (err) {
      toast({ type: "error", title: err instanceof Error ? err.message : "Failed" });
    }
  };

  const remove = async (item: any) => {
    const key = item.id || item.extension;
    try {
      await api(baseUrl, token, `/v1/${endpoint}/${key}`, { method: "DELETE" });
      toast({ type: "success", title: "Deleted" }); load();
    } catch { toast({ type: "error", title: "Failed to delete" }); }
  };

  return (
    <section className="border border-border-subtle bg-surface rounded-md overflow-hidden">
      <div className="p-3 border-b border-border-subtle flex items-center gap-2">
        <Icon size={17} className="text-accent" /><h2 className="font-medium">{title}</h2>
      </div>
      <form onSubmit={submit} className="p-3 grid md:grid-cols-5 gap-2 border-b border-border-subtle">
        {fields.map(([label, key, def]) => (
          <Field key={key} label={label} value={form[key] || def || ""} onChange={(v) => setForm({ ...form, [key]: v })} />
        ))}
        <button className="h-10 self-end rounded-md bg-accent hover:bg-accent-hover text-white text-sm font-medium flex items-center justify-center gap-2">
          <Plus size={16} /> Create
        </button>
      </form>
      <div className="p-3 overflow-x-auto">
        <table className="w-full text-sm">
          <thead className="text-tertiary"><tr className="border-b border-border-subtle">
            {[...columns, ""].map((h) => <th key={h} className="text-left py-2 px-2 font-medium">{h}</th>)}
          </tr></thead>
          <tbody>
            {items.length === 0 ? <tr><td colSpan={columns.length + 1} className="py-4 px-2 text-secondary">No records</td></tr> :
              items.map((item, idx) => (
                <tr key={item.id || idx} className="border-b border-border-subtle">
                  {rowFn(item).map((cell, ci) => <td key={ci} className="py-2 px-2 max-w-[200px] truncate">{cell}</td>)}
                  <td className="py-2 px-2 text-right">
                    <IconButton label="Delete" tone="danger" onClick={() => remove(item)}><Trash2 size={16} /></IconButton>
                  </td>
                </tr>
              ))}
          </tbody>
        </table>
      </div>
    </section>
  );
}

function ExtensionsPanel({ baseUrl, token }: { baseUrl: string; token: string }) {
  const [extensions, setExtensions] = useState<any[]>([]);
  const [users, setUsers] = useState<any[]>([]);
  const [showUnassignedOnly, setShowUnassignedOnly] = useState(false);
  const [assigningExt, setAssigningExt] = useState<string | null>(null);
  const [selectedUserId, setSelectedUserId] = useState("");

  // Create form state
  const [newExt, setNewExt] = useState("");
  const [newType, setNewType] = useState("user");
  const [newLabel, setNewLabel] = useState("");
  const [newUserId, setNewUserId] = useState("");
  const [newDest, setNewDest] = useState("");

  const load = useCallback(async () => {
    const qs = showUnassignedOnly ? "?unassigned=true" : "";
    const [exts, userList] = await Promise.all([
      api<any[]>(baseUrl, token, `/v1/extensions${qs}`),
      api<any[]>(baseUrl, token, "/v1/users"),
    ]);
    setExtensions(exts);
    setUsers(userList);
  }, [baseUrl, showUnassignedOnly, token]);
  useEffect(() => { load(); }, [load]);

  const suggestNext = () => {
    const used = new Set(extensions.map(e => parseInt(e.extension)).filter(n => !isNaN(n)));
    let next = 1001;
    while (used.has(next)) next++;
    setNewExt(next.toString());
  };

  const submit = async (event: FormEvent) => {
    event.preventDefault();
    try {
      const body: any = {
        extension: newExt,
        destination: newType === "user" ? (users.find(u => u.id === newUserId)?.sip_uri || newDest) : newDest,
        destination_type: newType,
        label: newLabel,
      };
      if (newType === "user" && newUserId) body.user_id = newUserId;
      await api(baseUrl, token, "/v1/extensions", { method: "POST", body });
      setNewExt(""); setNewLabel(""); setNewUserId(""); setNewDest("");
      toast({ type: "success", title: "Extension created" });
      load();
    } catch (err) { toast({ type: "error", title: err instanceof Error ? err.message : "Failed" }); }
  };

  const assignUser = async (ext: string) => {
    if (!selectedUserId) return;
    try {
      await api(baseUrl, token, `/v1/extensions/${encodeURIComponent(ext)}/assign`, { method: "PUT", body: { user_id: selectedUserId } });
      setAssigningExt(null); setSelectedUserId("");
      toast({ type: "success", title: "Extension assigned" });
      load();
    } catch (_err) { toast({ type: "error", title: "Failed to assign" }); }
  };

  const unassign = async (ext: string) => {
    try {
      await api(baseUrl, token, `/v1/extensions/${encodeURIComponent(ext)}/unassign`, { method: "PUT" });
      toast({ type: "success", title: "Extension unassigned" });
      load();
    } catch (_err) { toast({ type: "error", title: "Failed" }); }
  };

  const remove = async (ext: string) => {
    try {
      await api(baseUrl, token, `/v1/extensions/${encodeURIComponent(ext)}`, { method: "DELETE" });
      toast({ type: "success", title: "Extension deleted" });
      load();
    } catch (_err) { toast({ type: "error", title: "Failed" }); }
  };

  return (
    <section className="border border-border-subtle bg-surface rounded-md overflow-hidden">
      <div className="p-3 border-b border-border-subtle flex items-center justify-between">
        <div className="flex items-center gap-2">
          <Server size={17} className="text-accent" />
          <h2 className="font-medium">Extensions</h2>
        </div>
        <label className="flex items-center gap-2 text-xs text-secondary">
          <input type="checkbox" checked={showUnassignedOnly} onChange={e => setShowUnassignedOnly(e.target.checked)} className="accent-accent" />
          Unassigned only
        </label>
      </div>

      {/* Create form */}
      <form onSubmit={submit} className="p-3 grid md:grid-cols-5 gap-2 border-b border-border-subtle">
        <div className="flex gap-1">
          <Field label="Extension" value={newExt} onChange={setNewExt} />
          <button type="button" onClick={suggestNext} className="self-end h-10 px-2 rounded-md border border-border-default text-xs hover:bg-elevated">Auto</button>
        </div>
        <label className="block">
          <span className="block text-xs text-tertiary mb-1">Type</span>
          <select value={newType} onChange={e => setNewType(e.target.value)}
            className="w-full h-10 rounded-md bg-base border border-border-default px-3 text-sm outline-none focus:border-border-focus">
            <option value="user">User</option>
            <option value="ring_group">Ring Group</option>
            <option value="ivr">IVR</option>
            <option value="queue">Queue</option>
            <option value="park">Park</option>
            <option value="voicemail">Voicemail</option>
            <option value="conference">Conference</option>
            <option value="external">External</option>
          </select>
        </label>
        {newType === "user" ? (
          <label className="block">
            <span className="block text-xs text-tertiary mb-1">User</span>
            <select value={newUserId} onChange={e => setNewUserId(e.target.value)}
              className="w-full h-10 rounded-md bg-base border border-border-default px-3 text-sm outline-none focus:border-border-focus">
              <option value="">Select user...</option>
              {users.map(u => <option key={u.id} value={u.id}>{u.display_name} ({u.sip_uri})</option>)}
            </select>
          </label>
        ) : (
          <Field label="Destination (SIP URI)" value={newDest} onChange={setNewDest} />
        )}
        <Field label="Label" value={newLabel} onChange={setNewLabel} />
        <button className="h-10 self-end rounded-md bg-accent hover:bg-accent-hover text-white text-sm font-medium flex items-center justify-center gap-2">
          <Plus size={16} /> Create
        </button>
      </form>

      {/* Table */}
      <div className="p-3 overflow-x-auto">
        <table className="w-full text-sm">
          <thead className="text-tertiary">
            <tr className="border-b border-border-subtle">
              {["Extension", "Assigned To", "Type", "Label", ""].map(h => (
                <th key={h} className="text-left py-2 px-2 font-medium">{h}</th>
              ))}
            </tr>
          </thead>
          <tbody>
            {extensions.length === 0 ? (
              <tr><td colSpan={5} className="py-4 px-2 text-secondary">No extensions</td></tr>
            ) : extensions.map(ext => (
              <tr key={ext.extension} className="border-b border-border-subtle">
                <td className="py-2 px-2 font-mono">{ext.extension}</td>
                <td className="py-2 px-2">
                  {assigningExt === ext.extension ? (
                    <div className="flex items-center gap-1">
                      <select value={selectedUserId} onChange={e => setSelectedUserId(e.target.value)}
                        className="h-8 rounded-md bg-base border border-border-default px-2 text-xs">
                        <option value="">Select user...</option>
                        {users.map(u => <option key={u.id} value={u.id}>{u.display_name}</option>)}
                      </select>
                      <button type="button" onClick={() => assignUser(ext.extension)}
                        className="h-8 px-2 rounded-md bg-accent text-white text-xs">Assign</button>
                      <button type="button" onClick={() => setAssigningExt(null)}
                        className="h-8 px-2 rounded-md border border-border-default text-xs">Cancel</button>
                    </div>
                  ) : ext.user_display_name ? (
                    <span className="text-primary">{ext.user_display_name}</span>
                  ) : ext.destination_type === "user" ? (
                    <span className="px-2 py-0.5 rounded-full text-xs font-medium bg-warning/20 text-warning">Unassigned</span>
                  ) : (
                    <span className="text-secondary">{ext.destination}</span>
                  )}
                </td>
                <td className="py-2 px-2">
                  <span className="px-2 py-0.5 rounded-full text-xs font-medium bg-elevated text-secondary">{ext.destination_type}</span>
                </td>
                <td className="py-2 px-2 text-secondary">{ext.label || "-"}</td>
                <td className="py-2 px-2 text-right">
                  <div className="inline-flex items-center gap-1">
                    {ext.user_id ? (
                      <button type="button" onClick={() => unassign(ext.extension)}
                        className="h-8 px-2 rounded-md hover:bg-elevated text-xs text-secondary">Unassign</button>
                    ) : ext.destination_type === "user" ? (
                      <button type="button" onClick={() => { setAssigningExt(ext.extension); setSelectedUserId(""); }}
                        className="h-8 px-2 rounded-md hover:bg-elevated text-xs text-accent">Assign</button>
                    ) : null}
                    <IconButton label="Delete" tone="danger" onClick={() => remove(ext.extension)}>
                      <Trash2 size={16} />
                    </IconButton>
                  </div>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </section>
  );
}

function DidsPanel({ baseUrl, token }: { baseUrl: string; token: string }) {
  const [dids, setDids] = useState<any[]>([]);
  const [users, setUsers] = useState<any[]>([]);
  const [did, setDid] = useState("");
  const [destinationType, setDestinationType] = useState("user");
  const [destination, setDestination] = useState("");
  const [userId, setUserId] = useState("");
  const [label, setLabel] = useState("");

  const load = useCallback(async () => {
    const [didList, userList] = await Promise.all([
      api<any[]>(baseUrl, token, "/v1/dids"),
      api<any[]>(baseUrl, token, "/v1/users"),
    ]);
    setDids(didList);
    setUsers(userList);
  }, [baseUrl, token]);
  useEffect(() => { load(); }, [load]);

  const submit = async (event: FormEvent) => {
    event.preventDefault();
    const selectedUser = users.find((user) => user.id === userId);
    try {
      await api(baseUrl, token, "/v1/dids", {
        method: "POST",
        body: {
          did,
          destination_type: destinationType,
          destination: destinationType === "user" ? selectedUser?.sip_uri ?? destination : destination,
          user_id: destinationType === "user" && userId ? userId : undefined,
          label,
        },
      });
      setDid("");
      setDestination("");
      setUserId("");
      setLabel("");
      toast({ type: "success", title: "DID created" });
      load();
    } catch (err) {
      toast({ type: "error", title: err instanceof Error ? err.message : "Failed to create DID" });
    }
  };

  const remove = async (number: string) => {
    try {
      await api(baseUrl, token, `/v1/dids/${encodeURIComponent(number)}`, { method: "DELETE" });
      toast({ type: "success", title: "DID deleted" });
      load();
    } catch (_err) {
      toast({ type: "error", title: "Failed to delete DID" });
    }
  };

  return (
    <section className="border border-border-subtle bg-surface rounded-md overflow-hidden">
      <div className="p-3 border-b border-border-subtle flex items-center gap-2">
        <Router size={17} className="text-accent" />
        <h2 className="font-medium">DIDs</h2>
      </div>
      <form onSubmit={submit} className="p-3 grid md:grid-cols-5 gap-2 border-b border-border-subtle">
        <Field label="DID" value={did} onChange={setDid} />
        <label className="block">
          <span className="block text-xs text-tertiary mb-1">Route to</span>
          <select value={destinationType} onChange={(event) => setDestinationType(event.target.value)}
            className="w-full h-10 rounded-md bg-base border border-border-default px-3 text-sm outline-none focus:border-border-focus">
            {["user", "ring_group", "queue", "ivr", "voicemail", "conference", "external"].map((value) => (
              <option key={value} value={value}>{value}</option>
            ))}
          </select>
        </label>
        {destinationType === "user" ? (
          <label className="block">
            <span className="block text-xs text-tertiary mb-1">User</span>
            <select value={userId} onChange={(event) => setUserId(event.target.value)}
              className="w-full h-10 rounded-md bg-base border border-border-default px-3 text-sm outline-none focus:border-border-focus">
              <option value="">Select user...</option>
              {users.map((user) => (
                <option key={user.id} value={user.id}>{user.display_name} ({user.sip_uri})</option>
              ))}
            </select>
          </label>
        ) : (
          <Field label="Destination" value={destination} onChange={setDestination} />
        )}
        <Field label="Label" value={label} onChange={setLabel} />
        <button className="h-10 self-end rounded-md bg-accent hover:bg-accent-hover text-white text-sm font-medium flex items-center justify-center gap-2">
          <Plus size={16} /> Add DID
        </button>
      </form>
      <div className="p-3 overflow-x-auto">
        <table className="w-full text-sm">
          <thead className="text-tertiary">
            <tr className="border-b border-border-subtle">
              {["DID", "Destination", "Type", "Label", ""].map((header) => (
                <th key={header} className="text-left py-2 px-2 font-medium">{header}</th>
              ))}
            </tr>
          </thead>
          <tbody>
            {dids.length === 0 ? (
              <tr><td colSpan={5} className="py-4 px-2 text-secondary">No DIDs</td></tr>
            ) : dids.map((entry) => (
              <tr key={entry.extension} className="border-b border-border-subtle">
                <td className="py-2 px-2 font-mono">{entry.extension}</td>
                <td className="py-2 px-2">{entry.user_display_name ?? entry.destination}</td>
                <td className="py-2 px-2 text-secondary">{entry.destination_type}</td>
                <td className="py-2 px-2 text-secondary">{entry.label || "-"}</td>
                <td className="py-2 px-2 text-right">
                  <IconButton label="Delete DID" tone="danger" onClick={() => remove(entry.extension)}>
                    <Trash2 size={16} />
                  </IconButton>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </section>
  );
}

function QueuesPanel({ baseUrl, token }: { baseUrl: string; token: string }) {
  const [queues, setQueues] = useState<any[]>([]);
  const [name, setName] = useState("");
  const [extension, setExtension] = useState("");
  const [strategy, setStrategy] = useState("round_robin");
  const [agents, setAgents] = useState("");
  const [overflow, setOverflow] = useState("");
  const [callbackEnabled, setCallbackEnabled] = useState(false);
  const [callbackThreshold, setCallbackThreshold] = useState("120");
  const [slaTarget, setSlaTarget] = useState("20");

  const load = useCallback(async () => {
    try {
      const data = await api<any[]>(baseUrl, token, "/v1/queues");
      setQueues(data);
    } catch { /* ignore */ }
  }, [baseUrl, token]);

  useEffect(() => { load(); }, [load]);

  const submit = async (event: FormEvent) => {
    event.preventDefault();
    try {
      const agentList = agents.split(",").map((a) => a.trim()).filter(Boolean).map((a) => ({
        agent_uri: a.startsWith("sip:") ? a : `sip:${a}`,
      }));
      await api(baseUrl, token, "/v1/queues", {
        method: "POST",
        body: {
          name, extension: extension.startsWith("sip:") ? extension : `sip:${extension}`,
          strategy, agents: agentList, overflow_destination: overflow || null,
          callback_enabled: callbackEnabled === true,
          callback_threshold_secs: parseInt(callbackThreshold) || 120,
          sla_target_secs: parseInt(slaTarget) || 20,
        },
      });
      setName(""); setExtension(""); setAgents(""); setOverflow("");
      setCallbackEnabled(false); setCallbackThreshold("120"); setSlaTarget("20");
      toast({ type: "success", title: "Queue created" }); load();
    } catch (err) {
      toast({ type: "error", title: err instanceof Error ? err.message : "Failed" });
    }
  };

  const remove = async (id: string) => {
    try {
      await api(baseUrl, token, `/v1/queues/${id}`, { method: "DELETE" });
      toast({ type: "success", title: "Queue deleted" }); load();
    } catch { toast({ type: "error", title: "Failed" }); }
  };

  return (
    <section className="border border-border-subtle bg-surface rounded-md overflow-hidden">
      <div className="p-3 border-b border-border-subtle flex items-center gap-2">
        <Users size={17} className="text-accent" /><h2 className="font-medium">Call Queues (ACD)</h2>
      </div>
      <form onSubmit={submit} className="p-3 grid md:grid-cols-3 lg:grid-cols-5 gap-2 border-b border-border-subtle">
        <Field label="Name" value={name} onChange={setName} />
        <Field label="Extension" value={extension} onChange={setExtension} />
        <label className="block">
          <span className="block text-xs text-tertiary mb-1">Strategy</span>
          <select value={strategy} onChange={(e) => setStrategy(e.target.value)}
            className="w-full h-10 rounded-md bg-base border border-border-default px-3 text-sm outline-none focus:border-border-focus">
            <option value="round_robin">Round Robin</option>
            <option value="longest_idle">Longest Idle</option>
            <option value="ring_all">Ring All</option>
            <option value="random">Random</option>
            <option value="skills_based">Skills Based</option>
          </select>
        </label>
        <Field label="Agents (SIP URIs)" value={agents} onChange={setAgents} />
        <Field label="Overflow" value={overflow} onChange={setOverflow} />
        <label className="flex items-center gap-2 self-end h-10">
          <input type="checkbox" checked={callbackEnabled} onChange={(e) => setCallbackEnabled(e.target.checked)}
            className="w-4 h-4 rounded border-border-default accent-accent" />
          <span className="text-xs text-tertiary">Callback enabled</span>
        </label>
        <Field label="Callback threshold (s)" value={callbackThreshold} onChange={setCallbackThreshold} />
        <Field label="SLA target (s)" value={slaTarget} onChange={setSlaTarget} />
        <button className="h-10 self-end rounded-md bg-accent hover:bg-accent-hover text-white text-sm font-medium flex items-center justify-center gap-2">
          <Plus size={16} /> Create
        </button>
      </form>
      <div className="p-3 overflow-x-auto">
        <table className="w-full text-sm">
          <thead className="text-tertiary"><tr className="border-b border-border-subtle">
            {["Name", "Extension", "Strategy", "Agents", "Overflow", ""].map((h) => <th key={h} className="text-left py-2 px-2 font-medium">{h}</th>)}
          </tr></thead>
          <tbody>
            {queues.length === 0 ? <tr><td colSpan={6} className="py-4 px-2 text-secondary">No queues</td></tr> :
              queues.map((q) => (
                <tr key={q.id} className="border-b border-border-subtle">
                  <td className="py-2 px-2">{q.name}</td>
                  <td className="py-2 px-2 text-secondary">{q.extension}</td>
                  <td className="py-2 px-2">{q.strategy}</td>
                  <td className="py-2 px-2 text-secondary max-w-[200px] truncate">{(q.agents || []).map((a: any) => a.agent_uri).join(", ")}</td>
                  <td className="py-2 px-2 text-secondary">{q.overflow_destination || "-"}</td>
                  <td className="py-2 px-2 text-right">
                    <IconButton label="Delete" tone="danger" onClick={() => remove(q.id)}><Trash2 size={16} /></IconButton>
                  </td>
                </tr>
              ))}
          </tbody>
        </table>
      </div>
    </section>
  );
}

function CdrsPanel({ baseUrl, token }: { baseUrl: string; token: string }) {
  const [cdrs, setCdrs] = useState<any[]>([]);

  useEffect(() => {
    api<any[]>(baseUrl, token, '/v1/cdrs?limit=200')
      .then(setCdrs)
      .catch(() => {});
  }, [baseUrl, token]);

  const answered = cdrs.filter((c) => c.disposition === "answered").length;
  const missed = cdrs.filter((c) => c.disposition === "no_answer" || c.disposition === "abandoned").length;
  const avgDuration = cdrs.length > 0 ? Math.round(cdrs.reduce((s, c) => s + c.duration_secs, 0) / cdrs.length) : 0;

  return (
    <div className="space-y-3">
      <div className="grid grid-cols-2 md:grid-cols-4 gap-2">
        <Metric label="Total Calls" value={cdrs.length} />
        <Metric label="Answered" value={answered} />
        <Metric label="Missed" value={missed} />
        <Metric label="Avg Duration (s)" value={avgDuration} />
      </div>
      <section className="border border-border-subtle bg-surface rounded-md overflow-hidden">
        <div className="p-3 border-b border-border-subtle"><h2 className="font-medium">Call Detail Records</h2></div>
        <div className="overflow-x-auto">
          <table className="w-full text-sm">
            <thead className="text-tertiary"><tr className="border-b border-border-subtle">
              {["Time", "Caller", "Callee", "Direction", "Duration", "Disposition", "Queue"].map((h) => (
                <th key={h} className="text-left py-2 px-2 font-medium">{h}</th>
              ))}
            </tr></thead>
            <tbody>
              {cdrs.length === 0 ? <tr><td colSpan={7} className="py-4 px-2 text-secondary">No records</td></tr> :
                cdrs.map((c) => (
                  <tr key={c.id} className="border-b border-border-subtle">
                    <td className="py-2 px-2 text-secondary">{shortDate(c.start_time)}</td>
                    <td className="py-2 px-2">{c.caller_uri}</td>
                    <td className="py-2 px-2">{c.callee_uri}</td>
                    <td className="py-2 px-2 text-secondary">{c.direction}</td>
                    <td className="py-2 px-2 tabular-nums">{c.duration_secs}s</td>
                    <td className="py-2 px-2">
                      <span className={cn("px-2 py-0.5 rounded-full text-xs font-medium",
                        c.disposition === "answered" ? "bg-success/20 text-success" :
                        c.disposition === "voicemail" ? "bg-accent/20 text-accent" :
                        "bg-destructive/20 text-destructive"
                      )}>{c.disposition}</span>
                    </td>
                    <td className="py-2 px-2 text-secondary">{c.queue_name || "-"}</td>
                  </tr>
                ))}
            </tbody>
          </table>
        </div>
      </section>
    </div>
  );
}

function AgentsPanel({ baseUrl, token }: { baseUrl: string; token: string }) {
  const [agents, setAgents] = useState<any[]>([]);
  const [sipUri, setSipUri] = useState("");
  const [role, setRole] = useState("agent");
  const [displayName, setDisplayName] = useState("");
  const [skills, setSkills] = useState("");
  const [selectedAgent, setSelectedAgent] = useState<string | null>(null);
  const [agentHistory, setAgentHistory] = useState<any[]>([]);

  const load = useCallback(async () => {
    try {
      const data = await api<any[]>(baseUrl, token, "/v1/agents");
      setAgents(data);
    } catch { /* ignore */ }
  }, [baseUrl, token]);
  useEffect(() => { load(); }, [load]);

  const submit = async (event: FormEvent) => {
    event.preventDefault();
    try {
      await api(baseUrl, token, "/v1/agents", {
        method: "POST",
        body: {
          user_sip_uri: sipUri.startsWith("sip:") ? sipUri : `sip:${sipUri}`,
          role, display_name: displayName,
          skills: skills ? skills.split(",").map((s) => s.trim()) : [],
        },
      });
      setSipUri(""); setDisplayName(""); setSkills("");
      toast({ type: "success", title: "Agent profile created" }); load();
    } catch (err) { toast({ type: "error", title: err instanceof Error ? err.message : "Failed" }); }
  };

  const changeState = async (uri: string, state: string) => {
    try {
      await api(baseUrl, token, `/v1/agents/${encodeURIComponent(uri)}/transition`, {
        method: "POST",
        body: { state, reason: null },
      });
      load();
    } catch (err) {
      toast({ type: "error", title: err instanceof Error ? err.message : "Invalid state transition" });
    }
  };

  const loadHistory = async (uri: string) => {
    setSelectedAgent(uri);
    try {
      const data = await api<any[]>(baseUrl, token, `/v1/agents/${encodeURIComponent(uri)}/history`);
      setAgentHistory((data || []).slice(0, 10));
    } catch {
      setAgentHistory([]);
    }
  };

  const stateColors: Record<string, string> = {
    available: "bg-success/20 text-success", on_call: "bg-red-500/20 text-red-500",
    wrap_up: "bg-yellow-500/20 text-yellow-500", break: "bg-accent/20 text-accent",
    training: "bg-accent/20 text-accent", meeting: "bg-accent/20 text-accent",
    offline: "bg-elevated text-secondary",
  };

  return (
    <section className="border border-border-subtle bg-surface rounded-md overflow-hidden">
      <div className="p-3 border-b border-border-subtle flex items-center gap-2">
        <Users size={17} className="text-accent" /><h2 className="font-medium">Agent Profiles</h2>
      </div>
      <form onSubmit={submit} className="p-3 grid md:grid-cols-5 gap-2 border-b border-border-subtle">
        <Field label="SIP URI" value={sipUri} onChange={setSipUri} />
        <Field label="Display Name" value={displayName} onChange={setDisplayName} />
        <label className="block">
          <span className="block text-xs text-tertiary mb-1">Role</span>
          <select value={role} onChange={(e) => setRole(e.target.value)}
            className="w-full h-10 rounded-md bg-base border border-border-default px-3 text-sm outline-none focus:border-border-focus">
            <option value="agent">Agent</option>
            <option value="supervisor">Supervisor</option>
            <option value="qa">QA</option>
            <option value="admin">Admin</option>
          </select>
        </label>
        <Field label="Skills (comma-separated)" value={skills} onChange={setSkills} />
        <button className="h-10 self-end rounded-md bg-accent hover:bg-accent-hover text-white text-sm font-medium flex items-center justify-center gap-2">
          <Plus size={16} /> Create
        </button>
      </form>
      <div className="p-3 overflow-x-auto">
        <table className="w-full text-sm">
          <thead className="text-tertiary"><tr className="border-b border-border-subtle">
            {["Agent", "Role", "State", "Calls", "Skills", "Actions"].map((h) => <th key={h} className="text-left py-2 px-2 font-medium">{h}</th>)}
          </tr></thead>
          <tbody>
            {agents.length === 0 ? <tr><td colSpan={6} className="py-4 px-2 text-secondary">No agents</td></tr> :
              agents.map((a) => (
                <tr key={a.user_sip_uri} className={cn("border-b border-border-subtle cursor-pointer hover:bg-elevated/50", selectedAgent === a.user_sip_uri && "bg-elevated/30")} onClick={() => loadHistory(a.user_sip_uri)}>
                  <td className="py-2 px-2">
                    <div>{a.display_name || a.user_sip_uri}</div>
                    <div className="text-xs text-tertiary">{a.user_sip_uri}</div>
                  </td>
                  <td className="py-2 px-2">
                    <span className={cn("px-2 py-0.5 rounded-full text-xs font-medium",
                      a.role === "supervisor" ? "bg-accent/20 text-accent" :
                      a.role === "qa" ? "bg-warning/20 text-warning" : "bg-elevated text-secondary"
                    )}>{a.role}</span>
                  </td>
                  <td className="py-2 px-2">
                    <span className={cn("px-2 py-0.5 rounded-full text-xs font-medium", stateColors[a.state] || "bg-elevated text-secondary")}>
                      {a.state}
                    </span>
                  </td>
                  <td className="py-2 px-2 tabular-nums">{a.total_calls}</td>
                  <td className="py-2 px-2 text-secondary">{(a.skills || []).join(", ") || "-"}</td>
                  <td className="py-2 px-2">
                    <select value={a.state} onChange={(e) => changeState(a.user_sip_uri, e.target.value)}
                      className="h-8 rounded-md bg-base border border-border-default px-2 text-xs outline-none">
                      <option value="available">Available</option>
                      <option value="on_call">On Call</option>
                      <option value="wrap_up">Wrap Up</option>
                      <option value="break">Break</option>
                      <option value="training">Training</option>
                      <option value="offline">Offline</option>
                    </select>
                  </td>
                </tr>
              ))}
          </tbody>
        </table>
      </div>
      {selectedAgent && (
        <div className="p-3 border-t border-border-subtle">
          <h3 className="font-medium text-sm mb-2">Agent History &mdash; {selectedAgent}</h3>
          <div className="overflow-x-auto">
            <table className="w-full text-sm">
              <thead className="text-tertiary"><tr className="border-b border-border-subtle">
                {["Time", "From", "To", "Reason", "Duration"].map((h) => <th key={h} className="text-left py-2 px-2 font-medium">{h}</th>)}
              </tr></thead>
              <tbody>
                {agentHistory.length === 0 ? <tr><td colSpan={5} className="py-4 px-2 text-secondary">No history</td></tr> :
                  agentHistory.map((h, i) => (
                    <tr key={i} className="border-b border-border-subtle">
                      <td className="py-2 px-2 text-secondary">{h.timestamp ? shortDate(h.timestamp) : "-"}</td>
                      <td className="py-2 px-2">{h.from_state || "-"}</td>
                      <td className="py-2 px-2">{h.to_state || "-"}</td>
                      <td className="py-2 px-2 text-secondary">{h.reason || "-"}</td>
                      <td className="py-2 px-2 tabular-nums">{h.duration_secs != null ? `${h.duration_secs}s` : "-"}</td>
                    </tr>
                  ))}
              </tbody>
            </table>
          </div>
        </div>
      )}
    </section>
  );
}

function WallboardPanel({ baseUrl, token }: { baseUrl: string; token: string }) {
  const [data, setData] = useState<any>(null);
  const [queueCallers, setQueueCallers] = useState<Record<string, any[]>>({});
  const [queueCallbacks, setQueueCallbacks] = useState<Record<string, any[]>>({});

  useEffect(() => {
    const load = () => {
      api(baseUrl, token, '/v1/wallboard')
        .then((d) => { if (d) setData(d); })
        .catch(() => {});
    };
    load();
    const interval = setInterval(load, 5000);
    return () => clearInterval(interval);
  }, [baseUrl, token]);

  // Fetch callers and callbacks for each queue
  useEffect(() => {
    if (!data?.queues) return;
    for (const q of data.queues) {
      if (q.calls_waiting > 0) {
        api<any[]>(baseUrl, token, `/v1/queues/${q.queue_id}/callers`)
          .then((callers) => setQueueCallers((prev) => ({ ...prev, [q.queue_id]: callers || [] })))
          .catch(() => {});
      }
      api<any[]>(baseUrl, token, `/v1/queues/${q.queue_id}/callbacks`)
        .then((cbs) => setQueueCallbacks((prev) => ({ ...prev, [q.queue_id]: cbs || [] })))
        .catch(() => {});
    }
  }, [data, baseUrl, token]);

  if (!data) return <p className="text-sm text-tertiary py-8 text-center">Loading wallboard...</p>;

  const waitTimeSince = (enteredAt: string) => {
    const secs = Math.round((Date.now() - new Date(enteredAt).getTime()) / 1000);
    return secs > 0 ? `${secs}s` : "0s";
  };

  return (
    <div className="space-y-4">
      <div className="grid grid-cols-2 md:grid-cols-5 gap-2">
        <Metric label="Agents Available" value={data.agents?.available ?? 0} />
        <Metric label="On Call" value={data.agents?.on_call ?? 0} />
        <Metric label="Wrap Up" value={data.agents?.wrap_up ?? 0} />
        <Metric label="On Break" value={data.agents?.on_break ?? 0} />
        <Metric label="Offline" value={data.agents?.offline ?? 0} />
      </div>

      {(data.queues || []).map((q: any) => {
        const slaAboveTarget = q.sla_target != null ? q.sla_percentage >= q.sla_target : true;
        const callers = queueCallers[q.queue_id] || [];
        const callbacks = queueCallbacks[q.queue_id] || [];
        return (
        <section key={q.queue_id} className="border border-border-subtle bg-surface rounded-md overflow-hidden">
          <div className="p-3 border-b border-border-subtle flex items-center justify-between">
            <div className="flex items-center gap-2">
              <span className={cn("w-2 h-2 rounded-full", q.calls_waiting > 0 ? "bg-warning animate-pulse" : "bg-success")} />
              <h3 className="font-medium">{q.queue_name}</h3>
            </div>
            <span className={cn("text-xs font-medium", slaAboveTarget ? "text-success" : "text-destructive")}>
              SLA: {q.sla_percentage.toFixed(0)}%{q.sla_target != null ? ` / ${q.sla_target}% target` : ""}
            </span>
          </div>
          <div className="grid grid-cols-3 md:grid-cols-6 gap-2 p-3">
            <div className="text-center"><div className="text-xl font-semibold">{q.calls_waiting}</div><div className="text-[10px] text-tertiary">Waiting</div></div>
            <div className="text-center"><div className="text-xl font-semibold">{q.calls_active}</div><div className="text-[10px] text-tertiary">Active</div></div>
            <div className="text-center"><div className="text-xl font-semibold">{q.agents_available}</div><div className="text-[10px] text-tertiary">Available</div></div>
            <div className="text-center"><div className="text-xl font-semibold">{q.longest_wait_secs}s</div><div className="text-[10px] text-tertiary">Longest Wait</div></div>
            <div className="text-center"><div className="text-xl font-semibold">{q.calls_answered}</div><div className="text-[10px] text-tertiary">Answered</div></div>
            <div className="text-center"><div className="text-xl font-semibold">{q.calls_abandoned}</div><div className="text-[10px] text-tertiary">Abandoned</div></div>
          </div>
          {q.calls_waiting > 0 && callers.length > 0 && (
            <div className="px-3 pb-3">
              <h4 className="text-xs font-medium text-tertiary mb-1">Callers in Queue</h4>
              <table className="w-full text-sm">
                <thead className="text-tertiary"><tr className="border-b border-border-subtle">
                  {["Position", "Caller", "Wait Time"].map((h) => <th key={h} className="text-left py-1 px-2 font-medium text-xs">{h}</th>)}
                </tr></thead>
                <tbody>
                  {callers.map((c: any, i: number) => (
                    <tr key={i} className="border-b border-border-subtle last:border-b-0">
                      <td className="py-1 px-2 tabular-nums">{c.position ?? i + 1}</td>
                      <td className="py-1 px-2">{c.caller_uri || c.caller || "-"}</td>
                      <td className="py-1 px-2 tabular-nums">{c.entered_at ? waitTimeSince(c.entered_at) : "-"}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )}
          {callbacks.length > 0 && (
            <div className="px-3 pb-3">
              <h4 className="text-xs font-medium text-tertiary mb-1">Pending Callbacks</h4>
              <table className="w-full text-sm">
                <thead className="text-tertiary"><tr className="border-b border-border-subtle">
                  {["Caller", "Callback #", "Position", "Status", "Requested At"].map((h) => <th key={h} className="text-left py-1 px-2 font-medium text-xs">{h}</th>)}
                </tr></thead>
                <tbody>
                  {callbacks.map((cb: any, i: number) => (
                    <tr key={i} className="border-b border-border-subtle last:border-b-0">
                      <td className="py-1 px-2">{cb.caller_uri || cb.caller || "-"}</td>
                      <td className="py-1 px-2">{cb.callback_number || "-"}</td>
                      <td className="py-1 px-2 tabular-nums">{cb.position ?? "-"}</td>
                      <td className="py-1 px-2">{cb.status || "-"}</td>
                      <td className="py-1 px-2 text-secondary">{cb.requested_at ? shortDate(cb.requested_at) : "-"}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )}
        </section>
        );
      })}

      <section className="border border-border-subtle bg-surface rounded-md overflow-hidden">
        <div className="p-3 border-b border-border-subtle"><h3 className="font-medium">Agent Status</h3></div>
        <div className="p-3 overflow-x-auto">
          <table className="w-full text-sm">
            <thead className="text-tertiary"><tr className="border-b border-border-subtle">
              {["Agent", "Role", "State", "Since", "Calls"].map((h) => <th key={h} className="text-left py-2 px-2 font-medium">{h}</th>)}
            </tr></thead>
            <tbody>
              {(data.agent_list || []).map((a: any) => (
                <tr key={a.user_sip_uri} className="border-b border-border-subtle">
                  <td className="py-2 px-2">{a.display_name || a.user_sip_uri}</td>
                  <td className="py-2 px-2 text-secondary">{a.role}</td>
                  <td className="py-2 px-2">
                    <span className={cn("px-2 py-0.5 rounded-full text-xs font-medium",
                      a.state === "available" ? "bg-success/20 text-success" :
                      a.state === "on_call" ? "bg-red-500/20 text-red-500" :
                      a.state === "wrap_up" ? "bg-yellow-500/20 text-yellow-500" :
                      "bg-elevated text-secondary"
                    )}>{a.state}</span>
                  </td>
                  <td className="py-2 px-2 text-secondary">{a.state_since ? shortDate(a.state_since) : "-"}</td>
                  <td className="py-2 px-2 tabular-nums">{a.total_calls}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      </section>
    </div>
  );
}

function VipCallersPanel({ baseUrl, token }: { baseUrl: string; token: string }) {
  const [vipCallers, setVipCallers] = useState<any[]>([]);
  const [pattern, setPattern] = useState("");
  const [priority, setPriority] = useState("10");
  const [label, setLabel] = useState("");
  const [queueOverride, setQueueOverride] = useState("");
  const [agentOverride, setAgentOverride] = useState("");

  const load = useCallback(async () => {
    try {
      const data = await api<any[]>(baseUrl, token, "/v1/vip-callers");
      setVipCallers(data);
    } catch { /* ignore */ }
  }, [baseUrl, token]);
  useEffect(() => { load(); }, [load]);

  const submit = async (event: FormEvent) => {
    event.preventDefault();
    try {
      await api(baseUrl, token, "/v1/vip-callers", {
        method: "POST",
        body: {
          pattern,
          priority: parseInt(priority) || 10,
          label,
          queue_override: queueOverride || null,
          agent_override: agentOverride || null,
        },
      });
      setPattern(""); setPriority("10"); setLabel(""); setQueueOverride(""); setAgentOverride("");
      toast({ type: "success", title: "VIP caller added" }); load();
    } catch (err) {
      toast({ type: "error", title: err instanceof Error ? err.message : "Failed" });
    }
  };

  const remove = async (id: string) => {
    try {
      await api(baseUrl, token, `/v1/vip-callers/${id}`, { method: "DELETE" });
      toast({ type: "success", title: "VIP caller removed" }); load();
    } catch { toast({ type: "error", title: "Failed" }); }
  };

  return (
    <section className="border border-border-subtle bg-surface rounded-md overflow-hidden">
      <div className="p-3 border-b border-border-subtle flex items-center gap-2">
        <Users size={17} className="text-accent" /><h2 className="font-medium">VIP Callers</h2>
      </div>
      <form onSubmit={submit} className="p-3 grid md:grid-cols-6 gap-2 border-b border-border-subtle">
        <Field label="Pattern" value={pattern} onChange={setPattern} />
        <label className="block">
          <span className="block text-xs text-tertiary mb-1">Priority</span>
          <input type="number" value={priority} onChange={(e) => setPriority(e.target.value)}
            className="w-full h-10 rounded-md bg-base border border-border-default px-3 text-sm outline-none focus:border-border-focus" />
        </label>
        <Field label="Label" value={label} onChange={setLabel} />
        <Field label="Queue Override" value={queueOverride} onChange={setQueueOverride} />
        <Field label="Agent Override" value={agentOverride} onChange={setAgentOverride} />
        <button className="h-10 self-end rounded-md bg-accent hover:bg-accent-hover text-white text-sm font-medium flex items-center justify-center gap-2">
          <Plus size={16} /> Create
        </button>
      </form>
      <div className="p-3 overflow-x-auto">
        <table className="w-full text-sm">
          <thead className="text-tertiary"><tr className="border-b border-border-subtle">
            {["Pattern", "Priority", "Label", "Queue Override", "Agent Override", ""].map((h) => <th key={h} className="text-left py-2 px-2 font-medium">{h}</th>)}
          </tr></thead>
          <tbody>
            {vipCallers.length === 0 ? <tr><td colSpan={6} className="py-4 px-2 text-secondary">No VIP callers</td></tr> :
              vipCallers.map((v) => (
                <tr key={v.id} className="border-b border-border-subtle">
                  <td className="py-2 px-2">{v.pattern}</td>
                  <td className="py-2 px-2 tabular-nums">{v.priority}</td>
                  <td className="py-2 px-2">{v.label || "-"}</td>
                  <td className="py-2 px-2 text-secondary">{v.queue_override || "-"}</td>
                  <td className="py-2 px-2 text-secondary">{v.agent_override || "-"}</td>
                  <td className="py-2 px-2 text-right">
                    <IconButton label="Delete" tone="danger" onClick={() => remove(v.id)}><Trash2 size={16} /></IconButton>
                  </td>
                </tr>
              ))}
          </tbody>
        </table>
      </div>
    </section>
  );
}

function QaPanel({ baseUrl, token }: { baseUrl: string; token: string }) {
  const [scorecards, setScorecards] = useState<any[]>([]);
  const [callId, setCallId] = useState("");
  const [agentUri, setAgentUri] = useState("");
  const [totalScore, setTotalScore] = useState("");
  const [maxScore, setMaxScore] = useState("100");
  const [comments, setComments] = useState("");

  useEffect(() => {
    api<any[]>(baseUrl, token, '/v1/qa/scorecards')
      .then(setScorecards).catch(() => {});
  }, [baseUrl, token]);

  const submit = async (event: FormEvent) => {
    event.preventDefault();
    try {
      await api(baseUrl, token, "/v1/qa/scorecards", {
        method: "POST",
        body: {
          call_id: callId, agent_uri: agentUri.startsWith("sip:") ? agentUri : `sip:${agentUri}`,
          scores: {}, total_score: parseFloat(totalScore) || 0, max_score: parseFloat(maxScore) || 100,
          comments,
        },
      });
      setCallId(""); setAgentUri(""); setTotalScore(""); setComments("");
      toast({ type: "success", title: "Scorecard saved" });
      const updated = await api<any[]>(baseUrl, token, "/v1/qa/scorecards");
      setScorecards(updated);
    } catch { toast({ type: "error", title: "Failed" }); }
  };

  const avgScore = scorecards.length > 0
    ? (scorecards.reduce((s, c) => s + (c.max_score > 0 ? (c.total_score / c.max_score) * 100 : 0), 0) / scorecards.length).toFixed(1)
    : "0";

  return (
    <div className="space-y-3">
      <div className="grid grid-cols-2 md:grid-cols-3 gap-2">
        <Metric label="Total Reviews" value={scorecards.length} />
        <Metric label="Avg Score %" value={parseFloat(avgScore)} />
        <Metric label="Agents Reviewed" value={new Set(scorecards.map((s) => s.agent_uri)).size} />
      </div>

      <section className="border border-border-subtle bg-surface rounded-md overflow-hidden">
        <div className="p-3 border-b border-border-subtle"><h2 className="font-medium">New Scorecard</h2></div>
        <form onSubmit={submit} className="p-3 grid md:grid-cols-5 gap-2">
          <Field label="Call ID" value={callId} onChange={setCallId} />
          <Field label="Agent SIP URI" value={agentUri} onChange={setAgentUri} />
          <Field label="Score" value={totalScore} onChange={setTotalScore} />
          <Field label="Max Score" value={maxScore} onChange={setMaxScore} />
          <button className="h-10 self-end rounded-md bg-accent hover:bg-accent-hover text-white text-sm font-medium">Score</button>
        </form>
      </section>

      <section className="border border-border-subtle bg-surface rounded-md overflow-hidden">
        <div className="p-3 border-b border-border-subtle"><h2 className="font-medium">Recent Scorecards</h2></div>
        <div className="overflow-x-auto">
          <table className="w-full text-sm">
            <thead className="text-tertiary"><tr className="border-b border-border-subtle">
              {["Date", "Agent", "Reviewer", "Score", "Comments"].map((h) => <th key={h} className="text-left py-2 px-2 font-medium">{h}</th>)}
            </tr></thead>
            <tbody>
              {scorecards.length === 0 ? <tr><td colSpan={5} className="py-4 px-2 text-secondary">No scorecards</td></tr> :
                scorecards.map((sc) => (
                  <tr key={sc.id} className="border-b border-border-subtle">
                    <td className="py-2 px-2 text-secondary">{shortDate(sc.created_at)}</td>
                    <td className="py-2 px-2">{sc.agent_uri}</td>
                    <td className="py-2 px-2 text-secondary">{sc.reviewer_uri}</td>
                    <td className="py-2 px-2">
                      <span className={cn("px-2 py-0.5 rounded-full text-xs font-medium",
                        (sc.total_score / sc.max_score) >= 0.8 ? "bg-success/20 text-success" :
                        (sc.total_score / sc.max_score) >= 0.6 ? "bg-warning/20 text-warning" :
                        "bg-destructive/20 text-destructive"
                      )}>{sc.total_score}/{sc.max_score}</span>
                    </td>
                    <td className="py-2 px-2 text-secondary max-w-[200px] truncate">{sc.comments || "-"}</td>
                  </tr>
                ))}
            </tbody>
          </table>
        </div>
      </section>
    </div>
  );
}

function DirectoryPanel({ baseUrl, token }: { baseUrl: string; token: string }) {
  const [config, setConfig] = useState({
    enabled: false,
    server_url: "ldap://dc.company.com:389",
    bind_dn: "",
    bind_password: "",
    base_dn: "",
    user_search_filter: "(&(objectClass=user)(sAMAccountName={username}))",
    user_dn_attribute: "sAMAccountName",
    display_name_attribute: "displayName",
    email_attribute: "mail",
    group_attribute: "memberOf",
    admin_group: "",
    sip_domain: "company.com",
  });
  const [testing, setTesting] = useState(false);
  const [testResult, setTestResult] = useState<string | null>(null);

  useEffect(() => {
    api(baseUrl, token, '/v1/ldap/config')
      .then((data) => { if (data) setConfig(data); })
      .catch(() => {});
  }, [baseUrl, token]);

  const save = async () => {
    try {
      await api(baseUrl, token, "/v1/ldap/config", {
        method: "PUT",
        body: config,
      });
      toast({ type: "success", title: "Directory configuration saved" });
    } catch {
      toast({ type: "error", title: "Failed to save" });
    }
  };

  const testConnection = async () => {
    setTesting(true);
    setTestResult(null);
    try {
      const data = await api<{ ok: boolean; message?: string }>(baseUrl, token, "/v1/ldap/test", {
        method: "POST",
      });
      setTestResult(data.ok ? "Connection successful" : (data.message || "Failed"));
    } catch {
      setTestResult("Connection failed");
    }
    setTesting(false);
  };

  return (
    <section className="border border-border-subtle bg-surface rounded-md overflow-hidden">
      <div className="p-3 border-b border-border-subtle flex items-center gap-2">
        <Users size={17} className="text-accent" />
        <h2 className="font-medium">Active Directory / LDAP</h2>
      </div>
      <div className="p-4 space-y-4">
        <div className="flex items-center justify-between">
          <div>
            <h3 className="text-sm font-medium text-primary">Enable Directory Integration</h3>
            <p className="text-xs text-tertiary">Users will authenticate against Active Directory. New users are auto-provisioned on first login.</p>
          </div>
          <input type="checkbox" checked={config.enabled} onChange={(e) => setConfig({ ...config, enabled: e.target.checked })}
            className="w-5 h-5 accent-accent" />
        </div>

        {config.enabled && (
          <>
            <div className="border-t border-border-subtle pt-4">
              <h4 className="text-xs font-semibold text-tertiary uppercase tracking-wider mb-3">Connection</h4>
              <div className="grid md:grid-cols-2 gap-3">
                <Field label="LDAP Server URL" value={config.server_url} onChange={(v) => setConfig({ ...config, server_url: v })} />
                <Field label="SIP Domain" value={config.sip_domain} onChange={(v) => setConfig({ ...config, sip_domain: v })} />
                <Field label="Bind DN (Service Account)" value={config.bind_dn} onChange={(v) => setConfig({ ...config, bind_dn: v })} />
                <Field label="Bind Password" value={config.bind_password} onChange={(v) => setConfig({ ...config, bind_password: v })} type="password" />
                <Field label="Base DN" value={config.base_dn} onChange={(v) => setConfig({ ...config, base_dn: v })} />
              </div>
            </div>

            <div className="border-t border-border-subtle pt-4">
              <h4 className="text-xs font-semibold text-tertiary uppercase tracking-wider mb-3">User Mapping</h4>
              <div className="grid md:grid-cols-2 gap-3">
                <Field label="User Search Filter" value={config.user_search_filter} onChange={(v) => setConfig({ ...config, user_search_filter: v })} />
                <Field label="Username Attribute" value={config.user_dn_attribute} onChange={(v) => setConfig({ ...config, user_dn_attribute: v })} />
                <Field label="Display Name Attribute" value={config.display_name_attribute} onChange={(v) => setConfig({ ...config, display_name_attribute: v })} />
                <Field label="Email Attribute" value={config.email_attribute} onChange={(v) => setConfig({ ...config, email_attribute: v })} />
              </div>
            </div>

            <div className="border-t border-border-subtle pt-4">
              <h4 className="text-xs font-semibold text-tertiary uppercase tracking-wider mb-3">Role Mapping</h4>
              <div className="grid md:grid-cols-2 gap-3">
                <Field label="Group Membership Attribute" value={config.group_attribute} onChange={(v) => setConfig({ ...config, group_attribute: v })} />
                <Field label="Admin Group DN" value={config.admin_group} onChange={(v) => setConfig({ ...config, admin_group: v })} />
              </div>
              <p className="text-xs text-tertiary mt-1">Users in the admin group will be assigned the admin role. Leave empty to make all AD users regular users.</p>
            </div>
          </>
        )}

        <div className="flex items-center gap-3 pt-2 border-t border-border-subtle">
          {config.enabled && (
            <button onClick={testConnection} disabled={testing}
              className="h-10 px-4 rounded-md border border-border-default hover:bg-elevated text-sm disabled:opacity-60">
              {testing ? "Testing..." : "Test Connection"}
            </button>
          )}
          <button onClick={save}
            className="h-10 px-4 rounded-md bg-accent hover:bg-accent-hover text-white text-sm font-medium">
            Save Configuration
          </button>
          {testResult && (
            <span className={cn("text-xs", testResult.includes("successful") ? "text-success" : "text-destructive")}>
              {testResult}
            </span>
          )}
        </div>
      </div>
    </section>
  );
}

function ConferencesPanel({
  baseUrl,
  token,
  snapshot,
  onChange,
}: {
  baseUrl: string;
  token: string;
  snapshot: AdminSnapshot | null;
  onChange: () => void;
}) {
  const [title, setTitle] = useState("");
  const [mode, setMode] = useState("audio");

  const submit = async (event: FormEvent) => {
    event.preventDefault();
    try {
      await createConference(baseUrl, token, {
        title,
        mode: mode as "audio" | "video" | "webinar",
      });
      setTitle("");
      setMode("audio");
      toast({ type: "success", title: "Conference created" });
      onChange();
    } catch (err) {
      toast({ type: "error", title: err instanceof Error ? err.message : "Failed to create conference" });
    }
  };

  return (
    <PanelWithForm
      title="Conferences"
      icon={Mic}
      onSubmit={submit}
      fields={[
        ["Title", title, setTitle],
        ["Mode (audio/video/webinar)", mode, setMode],
      ]}
      action="Create conference"
    >
      <div className="overflow-x-auto">
        <table className="w-full text-sm">
          <thead className="text-tertiary">
            <tr className="border-b border-border-subtle">
              {["Title", "Mode", "Participants", "Created"].map((header) => (
                <th key={header} className="text-left py-2 px-2 font-medium">{header}</th>
              ))}
            </tr>
          </thead>
          <tbody>
            {(snapshot?.conferences ?? []).map((conf) => (
              <tr key={conf.id} className="border-b border-border-subtle">
                <td className="py-2 px-2">{conf.title}</td>
                <td className="py-2 px-2 text-secondary">{conf.mode}</td>
                <td className="py-2 px-2">{conf.participants.length}</td>
                <td className="py-2 px-2 text-secondary">{shortDate(conf.created_at)}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </PanelWithForm>
  );
}

function FilesPanel({
  baseUrl,
  token,
  snapshot,
  onChange,
}: {
  baseUrl: string;
  token: string;
  snapshot: AdminSnapshot | null;
  onChange: () => void;
}) {
  const remove = async (id: string) => {
    try {
      await deleteFile(baseUrl, token, id);
      toast({ type: "success", title: "File deleted" });
      onChange();
    } catch (err) {
      toast({ type: "error", title: err instanceof Error ? err.message : "Failed to delete file" });
    }
  };

  return (
    <section className="border border-border-subtle bg-surface rounded-md overflow-hidden">
      <div className="p-3 border-b border-border-subtle flex items-center gap-2">
        <FileText size={17} className="text-accent" />
        <h2 className="font-medium">Files</h2>
      </div>
      <div className="p-3 overflow-x-auto">
        <table className="w-full text-sm">
          <thead className="text-tertiary">
            <tr className="border-b border-border-subtle">
              {["Filename", "Owner", "Size", "Type", "Created", ""].map((header) => (
                <th key={header} className="text-left py-2 px-2 font-medium">{header}</th>
              ))}
            </tr>
          </thead>
          <tbody>
            {(snapshot?.files ?? []).length === 0 ? (
              <tr>
                <td className="py-4 px-2 text-secondary" colSpan={6}>No files</td>
              </tr>
            ) : (
              (snapshot?.files ?? []).map((file) => (
                <tr key={file.id} className="border-b border-border-subtle">
                  <td className="py-2 px-2">{file.filename}</td>
                  <td className="py-2 px-2 text-secondary">{file.owner}</td>
                  <td className="py-2 px-2 text-secondary">{formatSize(file.size)}</td>
                  <td className="py-2 px-2 text-secondary">{file.content_type}</td>
                  <td className="py-2 px-2 text-secondary">{shortDate(file.created_at)}</td>
                  <td className="py-2 px-2 text-right">
                    <IconButton label="Delete file" tone="danger" onClick={() => remove(file.id)}>
                      <Trash2 size={16} />
                    </IconButton>
                  </td>
                </tr>
              ))
            )}
          </tbody>
        </table>
      </div>
    </section>
  );
}

function formatSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

function PanelWithForm({
  title,
  icon: Icon,
  fields,
  action,
  onSubmit,
  children,
}: {
  title: string;
  icon: LucideIcon;
  fields: Array<[string, string, (value: string) => void, string?]>;
  action: string;
  onSubmit: (event: FormEvent) => void;
  children: React.ReactNode;
}) {
  return (
    <section className="border border-border-subtle bg-surface rounded-md overflow-hidden">
      <div className="p-3 border-b border-border-subtle flex items-center gap-2">
        <Icon size={17} className="text-accent" />
        <h2 className="font-medium">{title}</h2>
      </div>
      <form onSubmit={onSubmit} className="p-3 grid md:grid-cols-5 gap-2 border-b border-border-subtle">
        {fields.map(([label, value, onChange, type]) => (
          <Field key={label} label={label} value={value} onChange={onChange} type={type} />
        ))}
        <button className="h-10 self-end rounded-md bg-accent hover:bg-accent-hover text-white text-sm font-medium flex items-center justify-center gap-2">
          <Plus size={16} />
          {action}
        </button>
      </form>
      <div className="p-3">{children}</div>
    </section>
  );
}

function Field({
  label,
  value,
  onChange,
  type = "text",
}: {
  label: string;
  value: string;
  onChange: (value: string) => void;
  type?: string;
}) {
  return (
    <label className="block">
      <span className="block text-xs text-tertiary mb-1">{label}</span>
      <input
        type={type}
        value={value}
        onChange={(event) => onChange(event.target.value)}
        className="w-full h-10 rounded-md bg-base border border-border-default px-3 text-sm outline-none focus:border-border-focus"
        required={!["Matrix user ID", "Display name", "Queue Override", "Agent Override", "Overflow", "Callback threshold (s)", "SLA target (s)"].includes(label)}
      />
    </label>
  );
}

function JsonField({
  label,
  value,
  onChange,
}: {
  label: string;
  value: string;
  onChange: (value: string) => void;
}) {
  return (
    <label className="block">
      <span className="block text-xs text-tertiary mb-1">{label}</span>
      <textarea
        value={value}
        onChange={(event) => onChange(event.target.value)}
        rows={4}
        className="w-full rounded-md bg-base border border-border-default px-3 py-2 text-sm font-mono outline-none focus:border-border-focus"
      />
    </label>
  );
}

function IconButton({
  label,
  tone = "neutral",
  onClick,
  children,
}: {
  label: string;
  tone?: "neutral" | "danger";
  onClick: () => void;
  children: React.ReactNode;
}) {
  return (
    <button
      onClick={onClick}
      className={cn(
        "w-8 h-8 rounded-md hover:bg-elevated inline-flex items-center justify-center",
        tone === "danger"
          ? "text-tertiary hover:text-destructive"
          : "text-tertiary hover:text-primary"
      )}
      aria-label={label}
      title={label}
    >
      {children}
    </button>
  );
}

function ReadOnlyField({ label, value }: { label: string; value: string }) {
  return (
    <label className="block">
      <span className="block text-xs text-tertiary mb-1">{label}</span>
      <input
        value={value}
        readOnly
        className="w-full h-10 rounded-md bg-base border border-border-default px-3 text-sm text-secondary outline-none"
      />
    </label>
  );
}

function Metric({ label, value }: { label: string; value: number }) {
  return (
    <div className="rounded-md border border-border-subtle bg-surface p-3">
      <div className="text-xl font-semibold tabular-nums">{value}</div>
      <div className="text-xs text-secondary">{label}</div>
    </div>
  );
}

function Table({
  title,
  columns,
  rows,
}: {
  title?: string;
  columns: string[];
  rows: ReactNode[][];
}) {
  return (
    <section className="border border-border-subtle bg-surface rounded-md overflow-hidden">
      {title && <h2 className="p-3 border-b border-border-subtle font-medium">{title}</h2>}
      <div className="overflow-x-auto">
        <table className="w-full text-sm">
          <thead className="text-tertiary">
            <tr className="border-b border-border-subtle">
              {columns.map((column) => (
                <th key={column} className="text-left py-2 px-2 font-medium">{column}</th>
              ))}
            </tr>
          </thead>
          <tbody>
            {rows.length === 0 ? (
              <tr>
                <td className="py-4 px-2 text-secondary" colSpan={columns.length}>No records</td>
              </tr>
            ) : (
              rows.map((row, index) => (
                <tr key={index} className="border-b border-border-subtle last:border-b-0">
                  {row.map((cell, cellIndex) => (
                    <td key={cellIndex} className="py-2 px-2 max-w-[260px] align-top break-words">{cell}</td>
                  ))}
                </tr>
              ))
            )}
          </tbody>
        </table>
      </div>
    </section>
  );
}

// ── Call Quality Dashboard ─────────────────────────────────────────

type CallQualityRating = "good" | "warning" | "poor";

interface CallQualitySummary {
  total_reports: number;
  avg_mos: number;
  avg_jitter_ms: number;
  avg_packet_loss_pct: number;
  avg_round_trip_ms: number;
  poor_quality_calls: number;
  warning_quality_calls?: number;
  worst_mos?: number;
}

interface CallQualityReport {
  id: string;
  user_sip_uri?: string;
  codec?: string;
  mos_score?: number;
  jitter_ms?: number;
  packet_loss_pct?: number;
  round_trip_ms?: number;
  rating?: CallQualityRating;
  issues?: string[];
  recommended_action?: string | null;
  reported_at: string;
}

function qualityClass(rating: CallQualityRating | undefined) {
  if (rating === "poor") return "bg-destructive/10 text-destructive border-destructive/20";
  if (rating === "warning") return "bg-warning/10 text-warning border-warning/20";
  return "bg-success/10 text-success border-success/20";
}

function issueLabel(issue: string) {
  return issue.split("_").join(" ");
}

function CqdPanel({ baseUrl, token }: { baseUrl: string; token: string }) {
  const [summary, setSummary] = useState<CallQualitySummary | null>(null);
  const [reports, setReports] = useState<CallQualityReport[]>([]);
  const [userFilter, setUserFilter] = useState("");
  const [callFilter, setCallFilter] = useState("");
  const [ratingFilter, setRatingFilter] = useState<"all" | CallQualityRating>("all");
  const [limit, setLimit] = useState("100");
  const [loading, setLoading] = useState(false);

  const queryString = useCallback(() => {
    const params = new URLSearchParams();
    if (userFilter.trim()) params.set("user_sip_uri", userFilter.trim());
    if (callFilter.trim()) params.set("call_id", callFilter.trim());
    if (ratingFilter !== "all") params.set("rating", ratingFilter);
    if (limit.trim()) params.set("limit", limit.trim());
    const query = params.toString();
    return query ? `?${query}` : "";
  }, [callFilter, limit, ratingFilter, userFilter]);

  const load = useCallback(async () => {
    if (!token) return;
    setLoading(true);
    try {
      const [nextSummary, nextReports] = await Promise.all([
        api<CallQualitySummary>(baseUrl, token, "/v1/call-quality/summary"),
        api<CallQualityReport[]>(baseUrl, token, `/v1/call-quality${queryString()}`),
      ]);
      setSummary(nextSummary);
      setReports(nextReports);
    } finally {
      setLoading(false);
    }
  }, [baseUrl, queryString, token]);

  useEffect(() => {
    load().catch(() => {});
  }, [load]);

  const downloadCsv = async () => {
    try {
      const response = await fetch(`${baseUrl.replace(/\/$/, "")}/v1/call-quality/export.csv${queryString()}`, {
        headers: { Authorization: `Bearer ${token}` },
      });
      if (!response.ok) throw new Error(`Export failed (${response.status})`);
      const blob = await response.blob();
      const url = URL.createObjectURL(blob);
      const a = document.createElement("a");
      a.href = url;
      a.download = `call-quality-${new Date().toISOString().slice(0, 10)}.csv`;
      a.click();
      URL.revokeObjectURL(url);
    } catch (err) {
      toast({ type: "error", title: err instanceof Error ? err.message : "Unable to export CQD data" });
    }
  };

  return (
    <div className="space-y-4">
      {summary && (
        <div className="grid grid-cols-2 md:grid-cols-4 xl:grid-cols-7 gap-3">
          <Metric label="Total Reports" value={summary.total_reports} />
          <Metric label="Warning Calls" value={summary.warning_quality_calls ?? 0} />
          <div className="rounded-md border border-border-subtle bg-surface p-3">
            <div className="text-xl font-semibold tabular-nums">{summary.avg_mos?.toFixed(2)}</div>
            <div className="text-xs text-secondary">Avg MOS Score</div>
          </div>
          <div className="rounded-md border border-border-subtle bg-surface p-3">
            <div className="text-xl font-semibold tabular-nums">{summary.avg_jitter_ms?.toFixed(1)}ms</div>
            <div className="text-xs text-secondary">Avg Jitter</div>
          </div>
          <div className="rounded-md border border-border-subtle bg-surface p-3">
            <div className="text-xl font-semibold tabular-nums">{summary.avg_packet_loss_pct?.toFixed(2)}%</div>
            <div className="text-xs text-secondary">Avg Packet Loss</div>
          </div>
          <Metric label="Poor Quality Calls" value={summary.poor_quality_calls} />
          <div className="rounded-md border border-border-subtle bg-surface p-3">
            <div className="text-xl font-semibold tabular-nums">{summary.worst_mos?.toFixed(2) ?? "0.00"}</div>
            <div className="text-xs text-secondary">Worst MOS</div>
          </div>
        </div>
      )}
      <div className="rounded-md border border-border-subtle bg-surface p-3">
        <div className="grid grid-cols-1 md:grid-cols-[1fr_1fr_140px_100px_auto_auto] gap-2">
          <input
            value={userFilter}
            onChange={(event) => setUserFilter(event.target.value)}
            placeholder="User"
            className="h-10 rounded-md bg-base border border-border-default px-3 text-sm outline-none focus:border-border-focus"
          />
          <input
            value={callFilter}
            onChange={(event) => setCallFilter(event.target.value)}
            placeholder="Call ID"
            className="h-10 rounded-md bg-base border border-border-default px-3 text-sm outline-none focus:border-border-focus"
          />
          <select
            value={ratingFilter}
            onChange={(event) => setRatingFilter(event.target.value as "all" | CallQualityRating)}
            className="h-10 rounded-md bg-base border border-border-default px-3 text-sm outline-none focus:border-border-focus"
          >
            <option value="all">All ratings</option>
            <option value="good">Good</option>
            <option value="warning">Warning</option>
            <option value="poor">Poor</option>
          </select>
          <input
            value={limit}
            onChange={(event) => setLimit(event.target.value)}
            inputMode="numeric"
            placeholder="Limit"
            className="h-10 rounded-md bg-base border border-border-default px-3 text-sm outline-none focus:border-border-focus"
          />
          <button
            onClick={() => load().catch(() => {})}
            disabled={loading}
            className="h-10 px-3 rounded-md border border-border-default hover:bg-elevated text-sm disabled:opacity-60"
          >
            {loading ? "Loading" : "Refresh"}
          </button>
          <button
            onClick={downloadCsv}
            className="h-10 px-3 rounded-md bg-accent hover:bg-accent-hover text-white text-sm inline-flex items-center justify-center gap-2"
          >
            <Download size={15} />
            CSV
          </button>
        </div>
      </div>
      <Table
        title="Recent Quality Reports"
        columns={["User", "Rating", "Codec", "MOS", "Jitter", "Loss", "RTT", "Issues", "Action", "Reported"]}
        rows={reports.slice(-50).reverse().map((r) => [
          r.user_sip_uri?.replace(/^sip:/, "") ?? "",
          <span className={cn("inline-flex rounded-full border px-2 py-0.5 text-[11px] font-medium capitalize", qualityClass(r.rating))}>
            {r.rating ?? "good"}
          </span>,
          r.codec ?? "",
          r.mos_score?.toFixed(2) ?? "",
          `${r.jitter_ms?.toFixed(1)}ms`,
          `${r.packet_loss_pct?.toFixed(2)}%`,
          `${r.round_trip_ms?.toFixed(0)}ms`,
          r.issues && r.issues.length > 0 ? (
            <span className="whitespace-normal text-secondary">{r.issues.map(issueLabel).join(", ")}</span>
          ) : (
            <span className="text-tertiary">None</span>
          ),
          r.recommended_action ? (
            <span className="whitespace-normal text-secondary">{r.recommended_action}</span>
          ) : (
            <span className="text-tertiary">No action</span>
          ),
          shortDate(r.reported_at),
        ])}
      />
    </div>
  );
}

// ── Collaboration Policy Panel ────────────────────────────────────

function CollaborationPolicyPanel({ baseUrl, token }: { baseUrl: string; token: string }) {
  const [policy, setPolicy] = useState<ServerCollaborationPolicy | null>(null);
  const [domains, setDomains] = useState("");
  const [saving, setSaving] = useState(false);
  const [loading, setLoading] = useState(false);

  const load = useCallback(async () => {
    if (!token) return;
    setLoading(true);
    try {
      const nextPolicy = await api<ServerCollaborationPolicy>(baseUrl, token, "/v1/admin/collaboration/policy");
      setPolicy(nextPolicy);
      setDomains(nextPolicy.allowed_external_domains.join(", "));
    } catch (err) {
      toast({ type: "error", title: err instanceof Error ? err.message : "Unable to load policy" });
    } finally {
      setLoading(false);
    }
  }, [baseUrl, token]);

  useEffect(() => {
    load();
  }, [load]);

  const updatePolicy = <K extends keyof ServerCollaborationPolicy>(key: K, value: ServerCollaborationPolicy[K]) => {
    setPolicy((current) => current ? { ...current, [key]: value } : current);
  };

  const save = async () => {
    if (!policy) return;
    const allowed_external_domains = domains
      .split(",")
      .map((domain) => domain.trim().replace(/^@/, "").toLowerCase())
      .filter(Boolean);
    setSaving(true);
    try {
      const saved = await api<ServerCollaborationPolicy>(baseUrl, token, "/v1/admin/collaboration/policy", {
        method: "PUT",
        body: {
          structured_mentions_enabled: policy.structured_mentions_enabled,
          broad_mentions_enabled: policy.broad_mentions_enabled,
          broad_mentions_allowed_roles: policy.broad_mentions_allowed_roles,
          broad_mentions_per_minute: Math.max(1, Number(policy.broad_mentions_per_minute) || 1),
          external_access_enabled: policy.external_access_enabled,
          allowed_external_domains,
          urgent_messages_enabled: policy.urgent_messages_enabled,
          meeting_recording_enabled: policy.meeting_recording_enabled,
        },
      });
      setPolicy(saved);
      setDomains(saved.allowed_external_domains.join(", "));
      toast({ type: "success", title: "Policy saved" });
    } catch (err) {
      toast({ type: "error", title: err instanceof Error ? err.message : "Unable to save policy" });
    } finally {
      setSaving(false);
    }
  };

  if (loading && !policy) {
    return <section className="border border-border-subtle bg-surface rounded-md p-4 text-sm text-secondary">Loading policy...</section>;
  }

  if (!policy) {
    return (
      <section className="border border-border-subtle bg-surface rounded-md p-4">
        <button onClick={load} className="h-9 px-3 rounded-md border border-border-default hover:bg-elevated text-sm inline-flex items-center gap-2">
          <RefreshCw size={16} />
          Retry
        </button>
      </section>
    );
  }

  const roles = policy.broad_mentions_allowed_roles.join(", ");

  return (
    <div className="space-y-4">
      <section className="border border-border-subtle bg-surface rounded-md overflow-hidden">
        <div className="p-3 border-b border-border-subtle flex items-center justify-between gap-3">
          <div className="flex items-center gap-2">
            <Shield size={17} className="text-accent" />
            <h2 className="font-medium">Collaboration policy</h2>
          </div>
          <button
            onClick={save}
            disabled={saving}
            className="h-9 px-3 rounded-md bg-accent hover:bg-accent-hover text-white text-sm font-medium inline-flex items-center gap-2 disabled:opacity-60"
          >
            <Save size={16} />
            {saving ? "Saving..." : "Save"}
          </button>
        </div>
        <div className="p-3 grid lg:grid-cols-2 gap-4">
          <div className="space-y-3">
            <PolicyToggle
              label="External access"
              checked={policy.external_access_enabled}
              onChange={(checked) => updatePolicy("external_access_enabled", checked)}
            />
            <label className="block">
              <span className="block text-xs text-tertiary mb-1">Allowed external domains</span>
              <textarea
                value={domains}
                onChange={(event) => setDomains(event.target.value)}
                rows={3}
                className="w-full rounded-md bg-base border border-border-default px-3 py-2 text-sm outline-none focus:border-border-focus"
                placeholder="partner.example, vendor.example"
              />
            </label>
            <PolicyToggle
              label="Meeting recording"
              checked={policy.meeting_recording_enabled}
              onChange={(checked) => updatePolicy("meeting_recording_enabled", checked)}
            />
            <PolicyToggle
              label="Urgent messages"
              checked={policy.urgent_messages_enabled}
              onChange={(checked) => updatePolicy("urgent_messages_enabled", checked)}
            />
          </div>
          <div className="space-y-3">
            <PolicyToggle
              label="Structured mentions"
              checked={policy.structured_mentions_enabled}
              onChange={(checked) => updatePolicy("structured_mentions_enabled", checked)}
            />
            <PolicyToggle
              label="Broad mentions"
              checked={policy.broad_mentions_enabled}
              onChange={(checked) => updatePolicy("broad_mentions_enabled", checked)}
            />
            <label className="block">
              <span className="block text-xs text-tertiary mb-1">Broad mention roles</span>
              <input
                value={roles}
                onChange={(event) => updatePolicy("broad_mentions_allowed_roles", event.target.value.split(",").map((role) => role.trim()).filter(Boolean))}
                className="w-full h-10 rounded-md bg-base border border-border-default px-3 text-sm outline-none focus:border-border-focus"
              />
            </label>
            <label className="block">
              <span className="block text-xs text-tertiary mb-1">Broad mentions per minute</span>
              <input
                type="number"
                min={1}
                value={policy.broad_mentions_per_minute}
                onChange={(event) => updatePolicy("broad_mentions_per_minute", Math.max(1, Number(event.target.value) || 1))}
                className="w-full h-10 rounded-md bg-base border border-border-default px-3 text-sm outline-none focus:border-border-focus"
              />
            </label>
          </div>
        </div>
      </section>
      <div className="grid grid-cols-2 md:grid-cols-4 gap-2">
        <Metric label="External domains" value={domains.split(",").map((domain) => domain.trim()).filter(Boolean).length} />
        <Metric label="Mention roles" value={policy.broad_mentions_allowed_roles.length} />
        <Metric label="Mention rate" value={policy.broad_mentions_per_minute} />
        <Metric label="Enabled controls" value={[
          policy.external_access_enabled,
          policy.meeting_recording_enabled,
          policy.urgent_messages_enabled,
          policy.structured_mentions_enabled,
          policy.broad_mentions_enabled,
        ].filter(Boolean).length} />
      </div>
    </div>
  );
}

function PolicyToggle({ label, checked, onChange }: { label: string; checked: boolean; onChange: (checked: boolean) => void }) {
  return (
    <label className="flex items-center justify-between gap-3 rounded-md border border-border-subtle bg-base px-3 py-2">
      <span className="text-sm">{label}</span>
      <input
        type="checkbox"
        checked={checked}
        onChange={(event) => onChange(event.target.checked)}
        className="h-4 w-4 accent-accent"
      />
    </label>
  );
}

// ── Retention Panel ───────────────────────────────────────────────

function RetentionPanel({ baseUrl, token }: { baseUrl: string; token: string }) {
  type RetentionPolicyRow = {
    id: string;
    name: string;
    scope: string;
    room_id?: string | null;
    retain_days?: number | null;
    legal_hold: boolean;
    export_enabled: boolean;
    created_by: string;
    updated_at: string;
  };
  type RetentionResult = {
    evaluated_at: string;
    dry_run: boolean;
    matched_messages: number;
    deleted_messages: number;
    skipped_legal_hold_policies: string[];
      policy_results: {
        policy_id: string;
        room_id?: string | null;
        retain_days?: number | null;
        matched_messages: number;
        deleted_messages: number;
        matched_files?: number;
        deleted_files?: number;
        matched_recordings?: number;
        deleted_recordings?: number;
        legal_hold: boolean;
      }[];
  };
  type RoomOption = { id: string; name: string; team_id?: string | null; channel_name?: string | null };

  const [policies, setPolicies] = useState<RetentionPolicyRow[]>([]);
  const [rooms, setRooms] = useState<RoomOption[]>([]);
  const [selectedPolicyId, setSelectedPolicyId] = useState<string | null>(null);
  const [name, setName] = useState("");
  const [scope, setScope] = useState("global");
  const [roomId, setRoomId] = useState("");
  const [retainDays, setRetainDays] = useState("90");
  const [legalHold, setLegalHold] = useState(false);
  const [exportEnabled, setExportEnabled] = useState(true);
  const [saving, setSaving] = useState(false);
  const [running, setRunning] = useState<"preview" | "apply" | null>(null);
  const [lastResult, setLastResult] = useState<RetentionResult | null>(null);
  const [exportRoomId, setExportRoomId] = useState("");
  const [exportQuery, setExportQuery] = useState("");
  const [exportUser, setExportUser] = useState("");
  const [exportFrom, setExportFrom] = useState("");
  const [exportTo, setExportTo] = useState("");
  const [exportLimit, setExportLimit] = useState("250");
  const [exportSummary, setExportSummary] = useState<{ exported_at: string; count: number; room_id?: string | null } | null>(null);
  const [searchingDiscovery, setSearchingDiscovery] = useState(false);

  const load = () => {
    if (!token) return;
    api<RetentionPolicyRow[]>(baseUrl, token, "/v1/admin/governance/retention").then(setPolicies).catch(() => {});
    api<RoomOption[]>(baseUrl, token, "/v1/rooms").then(setRooms).catch(() => {});
  };

  useEffect(load, [baseUrl, token]);

  const resetForm = () => {
    setSelectedPolicyId(null);
    setName("");
    setScope("global");
    setRoomId("");
    setRetainDays("90");
    setLegalHold(false);
    setExportEnabled(true);
  };

  const editPolicy = (policy: RetentionPolicyRow) => {
    setSelectedPolicyId(policy.id);
    setName(policy.name);
    setScope(policy.scope);
    setRoomId(policy.room_id ?? "");
    setRetainDays(policy.retain_days?.toString() ?? "");
    setLegalHold(policy.legal_hold);
    setExportEnabled(policy.export_enabled);
  };

  const savePolicy = async (event: FormEvent) => {
    event.preventDefault();
    if (!name.trim()) return;
    setSaving(true);
    try {
      await api<RetentionPolicyRow>(baseUrl, token, "/v1/admin/governance/retention", {
        method: "PUT",
        body: {
          id: selectedPolicyId,
          name: name.trim(),
          scope,
          room_id: scope === "room" ? roomId || null : null,
          retain_days: retainDays.trim() ? Math.max(1, Number.parseInt(retainDays, 10) || 1) : null,
          legal_hold: legalHold,
          export_enabled: exportEnabled,
        },
      });
      resetForm();
      load();
      toast({ type: "success", title: selectedPolicyId ? "Policy updated" : "Policy saved" });
    } catch {
      toast({ type: "error", title: "Policy save failed" });
    } finally {
      setSaving(false);
    }
  };

  const deletePolicy = async (policy: RetentionPolicyRow) => {
    try {
      await api(baseUrl, token, `/v1/admin/governance/retention/${policy.id}`, { method: "DELETE" });
      if (selectedPolicyId === policy.id) resetForm();
      load();
      toast({ type: "success", title: "Retention policy deleted" });
    } catch {
      toast({ type: "error", title: "Policy delete failed" });
    }
  };

  const enforce = async (dryRun: boolean) => {
    setRunning(dryRun ? "preview" : "apply");
    try {
      const result = await api<RetentionResult>(baseUrl, token, "/v1/admin/governance/retention/enforce", {
        method: dryRun ? "GET" : "POST",
      });
      setLastResult(result);
      toast({
        type: dryRun ? "info" : "success",
        title: dryRun
          ? `${result.deleted_messages} items would be removed`
          : `${result.deleted_messages} items removed`,
      });
      if (!dryRun) load();
    } catch {
      toast({ type: "error", title: dryRun ? "Preview failed" : "Enforcement failed" });
    } finally {
      setRunning(null);
    }
  };

  const discoveryParams = () => {
    const params = new URLSearchParams();
    if (exportRoomId.trim()) params.set("room_id", exportRoomId.trim());
    if (exportQuery.trim()) params.set("q", exportQuery.trim());
    if (exportUser.trim()) params.set("user_uri", exportUser.trim());
    if (exportFrom) params.set("from", new Date(exportFrom).toISOString());
    if (exportTo) params.set("to", new Date(exportTo).toISOString());
    if (exportLimit.trim()) params.set("limit", String(Math.max(1, Number.parseInt(exportLimit, 10) || 250)));
    return params;
  };

  const fetchDiscovery = async (exporting = false) => {
    const params = discoveryParams();
    if (exporting) params.set("export", "true");
    const hasFilters = Array.from(params.keys()).some((key) => key !== "limit" && key !== "export");
    const query = params.toString();
    return api<{ exported_at: string; room_id?: string | null; messages: any[]; files?: any[]; recordings?: any[] }>(
      baseUrl,
      token,
      hasFilters ? `/v1/admin/ediscovery/search?${query}` : `/v1/admin/ediscovery/export${exportRoomId ? `?room_id=${encodeURIComponent(exportRoomId)}` : ""}`
    );
  };

  const previewDiscovery = async () => {
    setSearchingDiscovery(true);
    try {
      const data = await fetchDiscovery();
      const count = data.messages.length + (data.files?.length ?? 0) + (data.recordings?.length ?? 0);
      setExportSummary({ exported_at: data.exported_at, count, room_id: data.room_id });
      toast({ type: "info", title: `${count} items matched` });
    } catch {
      toast({ type: "error", title: "Search failed" });
    } finally {
      setSearchingDiscovery(false);
    }
  };

  const exportDiscovery = async () => {
    try {
      const data = await fetchDiscovery(true);
      const blob = new Blob([JSON.stringify(data, null, 2)], { type: "application/json" });
      const url = URL.createObjectURL(blob);
      const a = document.createElement("a");
      a.href = url;
      a.download = `ediscovery-${exportRoomId || exportQuery || exportUser || "all"}-${new Date().toISOString().slice(0, 10)}.json`;
      a.click();
      URL.revokeObjectURL(url);
      const exportedCount = data.messages.length + (data.files?.length ?? 0) + (data.recordings?.length ?? 0);
      setExportSummary({ exported_at: data.exported_at, count: exportedCount, room_id: data.room_id });
      toast({ type: "success", title: `Exported ${exportedCount} items` });
    } catch {
      toast({ type: "error", title: "Export failed" });
    }
  };

  const stats = useMemo(() => {
    const legalHolds = policies.filter((policy) => policy.legal_hold).length;
    const finiteRetention = policies.filter((policy) => policy.retain_days != null && !policy.legal_hold).length;
    const filePolicies = policies.filter((policy) => matchesFilePolicy(policy.scope)).length;
    const recordingPolicies = policies.filter((policy) => matchesRecordingPolicy(policy.scope)).length;
    return { policies: policies.length, legalHolds, finiteRetention, filePolicies, recordingPolicies };
  }, [policies]);

  const roomName = (id?: string | null) => {
    if (!id) return "All rooms";
    const room = rooms.find((item) => item.id === id);
    return room ? room.name : id.slice(0, 8);
  };

  return (
    <div className="space-y-4">
      <div className="grid grid-cols-2 md:grid-cols-4 gap-3">
        <Metric label="Policies" value={stats.policies} />
        <Metric label="Legal holds" value={stats.legalHolds} />
        <Metric label="Retention rules" value={stats.finiteRetention} />
        <Metric label="Media policies" value={stats.filePolicies + stats.recordingPolicies} />
      </div>

      <div className="grid xl:grid-cols-[1fr_380px] gap-4">
        <section className="border border-border-subtle bg-surface rounded-md overflow-hidden">
          <div className="p-3 border-b border-border-subtle flex items-center justify-between gap-3">
            <div className="flex items-center gap-2">
              <Archive size={17} className="text-accent" />
              <h2 className="font-medium">Retention and legal hold</h2>
            </div>
            <button
              onClick={resetForm}
              className="h-8 px-3 rounded-md border border-border-default hover:bg-elevated text-sm inline-flex items-center gap-2"
            >
              <Plus size={15} />
              New
            </button>
          </div>

          <form onSubmit={savePolicy} className="p-3 grid md:grid-cols-2 xl:grid-cols-4 gap-3 border-b border-border-subtle">
            <label className="block xl:col-span-2">
              <span className="block text-xs text-tertiary mb-1">Policy name</span>
              <input
                value={name}
                onChange={(event) => setName(event.target.value)}
                className="w-full h-10 rounded-md bg-base border border-border-default px-3 text-sm outline-none focus:border-border-focus"
                placeholder="Executive chat retention"
                required
              />
            </label>
            <label className="block">
              <span className="block text-xs text-tertiary mb-1">Scope</span>
              <select
                value={scope}
                onChange={(event) => setScope(event.target.value)}
                className="w-full h-10 rounded-md bg-base border border-border-default px-3 text-sm outline-none focus:border-border-focus"
              >
                <option value="global">Global</option>
                <option value="room">Room or channel</option>
                <option value="files">Files</option>
                <option value="recordings">Recordings</option>
              </select>
            </label>
            <label className="block">
              <span className="block text-xs text-tertiary mb-1">Retain days</span>
              <input
                value={retainDays}
                onChange={(event) => setRetainDays(event.target.value)}
                className="w-full h-10 rounded-md bg-base border border-border-default px-3 text-sm outline-none focus:border-border-focus"
                placeholder="Leave blank to retain indefinitely"
                type="number"
                min={1}
              />
            </label>
            {scope === "room" && (
              <div className="block xl:col-span-2 space-y-2">
                {rooms.length > 0 && (
                  <label className="block">
                    <span className="block text-xs text-tertiary mb-1">Known room or channel</span>
                    <select
                      value={rooms.some((room) => room.id === roomId) ? roomId : ""}
                      onChange={(event) => setRoomId(event.target.value)}
                      className="w-full h-10 rounded-md bg-base border border-border-default px-3 text-sm outline-none focus:border-border-focus"
                    >
                      <option value="">Select a room</option>
                      {rooms.map((room) => (
                        <option key={room.id} value={room.id}>{room.name}</option>
                      ))}
                    </select>
                  </label>
                )}
                <label className="block">
                  <span className="block text-xs text-tertiary mb-1">Room ID</span>
                  <input
                    value={roomId}
                    onChange={(event) => setRoomId(event.target.value)}
                    className="w-full h-10 rounded-md bg-base border border-border-default px-3 text-sm outline-none focus:border-border-focus"
                    placeholder="Paste a room UUID"
                    required
                  />
                </label>
              </div>
            )}
            <label className="h-10 self-end rounded-md border border-border-default px-3 flex items-center gap-2 text-sm">
              <input type="checkbox" checked={legalHold} onChange={(event) => setLegalHold(event.target.checked)} />
              Legal hold
            </label>
            <label className="h-10 self-end rounded-md border border-border-default px-3 flex items-center gap-2 text-sm">
              <input type="checkbox" checked={exportEnabled} onChange={(event) => setExportEnabled(event.target.checked)} />
              eDiscovery export
            </label>
            <button
              disabled={saving || (scope === "room" && !roomId)}
              className="h-10 self-end rounded-md bg-accent hover:bg-accent-hover text-white text-sm font-medium flex items-center justify-center gap-2 disabled:opacity-60"
            >
              <Save size={16} />
              {saving ? "Saving..." : selectedPolicyId ? "Update policy" : "Save policy"}
            </button>
          </form>

          <div className="overflow-x-auto">
            <table className="w-full text-sm">
              <thead className="text-tertiary">
                <tr className="border-b border-border-subtle">
                  {["Policy", "Scope", "Retention", "Controls", "Updated", ""].map((column) => (
                    <th key={column} className="text-left py-2 px-3 font-medium">{column}</th>
                  ))}
                </tr>
              </thead>
              <tbody>
                {policies.length === 0 ? (
                  <tr>
                    <td className="py-4 px-3 text-secondary" colSpan={6}>No retention policies</td>
                  </tr>
                ) : policies.map((policy) => (
                  <tr key={policy.id} className="border-b border-border-subtle last:border-0">
                    <td className="py-2 px-3">
                      <div className="font-medium">{policy.name}</div>
                      <div className="text-xs text-tertiary">{policy.created_by}</div>
                    </td>
                    <td className="py-2 px-3">
                      <div>{policy.scope}</div>
                      <div className="text-xs text-tertiary">{roomName(policy.room_id)}</div>
                    </td>
                    <td className="py-2 px-3">{policy.retain_days ? `${policy.retain_days} days` : "Indefinite"}</td>
                    <td className="py-2 px-3">
                      <div className="flex flex-wrap gap-1">
                        {policy.legal_hold && <Badge tone="warn">Legal hold</Badge>}
                        {policy.export_enabled && <Badge tone="ok">Export</Badge>}
                        {!policy.legal_hold && !policy.export_enabled && <span className="text-tertiary">Standard</span>}
                      </div>
                    </td>
                    <td className="py-2 px-3">{shortDate(policy.updated_at)}</td>
                    <td className="py-2 px-3 text-right">
                      <div className="inline-flex items-center gap-2">
                      <button
                        onClick={() => editPolicy(policy)}
                        className="h-8 px-3 rounded-md border border-border-default hover:bg-elevated text-sm"
                      >
                        Edit
                      </button>
                      <button
                        onClick={() => deletePolicy(policy)}
                        className="h-8 w-8 rounded-md text-destructive hover:bg-destructive/10 inline-flex items-center justify-center"
                        title="Delete policy"
                      >
                        <Trash2 size={15} />
                      </button>
                      </div>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </section>

        <div className="space-y-4">
          <section className="border border-border-subtle bg-surface rounded-md overflow-hidden">
            <div className="p-3 border-b border-border-subtle flex items-center gap-2">
              <CheckCircle2 size={17} className="text-accent" />
              <h2 className="font-medium">Enforcement</h2>
            </div>
            <div className="p-3 space-y-3">
              <div className="grid grid-cols-2 gap-2">
                <button
                  onClick={() => enforce(true)}
                  disabled={running !== null}
                  className="h-10 rounded-md border border-border-default hover:bg-elevated text-sm inline-flex items-center justify-center gap-2 disabled:opacity-60"
                >
                  <Search size={16} />
                  {running === "preview" ? "Checking..." : "Preview"}
                </button>
                <button
                  onClick={() => enforce(false)}
                  disabled={running !== null}
                  className="h-10 rounded-md bg-destructive hover:opacity-90 text-white text-sm inline-flex items-center justify-center gap-2 disabled:opacity-60"
                >
                  <Trash2 size={16} />
                  {running === "apply" ? "Running..." : "Apply"}
                </button>
              </div>
              {lastResult ? (
                <div className="rounded-md border border-border-subtle bg-base p-3 space-y-3">
                  <div className="grid grid-cols-2 gap-2">
                    <Metric label="Matched items" value={lastResult.matched_messages} />
                    <Metric label={lastResult.dry_run ? "Would remove" : "Removed"} value={lastResult.deleted_messages} />
                  </div>
                  <div className="text-xs text-secondary">
                    Evaluated {shortDate(lastResult.evaluated_at)}
                    {lastResult.skipped_legal_hold_policies.length > 0
                      ? `; skipped ${lastResult.skipped_legal_hold_policies.length} legal hold policies`
                      : ""}
                  </div>
                  <div className="space-y-2">
                    {lastResult.policy_results.slice(0, 5).map((result) => (
                      <div key={result.policy_id} className="rounded border border-border-subtle p-2 text-xs">
                        <div className="font-medium">{roomName(result.room_id)}</div>
                        <div className="text-secondary">
                          {result.matched_messages} messages, {result.matched_files ?? 0} files, and {result.matched_recordings ?? 0} recordings matched; {result.deleted_messages + (result.deleted_files ?? 0) + (result.deleted_recordings ?? 0)} {lastResult.dry_run ? "would be removed" : "removed"}
                        </div>
                      </div>
                    ))}
                  </div>
                </div>
              ) : (
                <p className="text-sm text-secondary">Preview before applying retention so admins can see the exact impact.</p>
              )}
            </div>
          </section>

          <section className="border border-border-subtle bg-surface rounded-md overflow-hidden">
            <div className="p-3 border-b border-border-subtle flex items-center gap-2">
              <Download size={17} className="text-accent" />
              <h2 className="font-medium">eDiscovery export</h2>
            </div>
            <div className="p-3 space-y-3">
              <label className="block">
                <span className="block text-xs text-tertiary mb-1">Keyword</span>
                <input
                  value={exportQuery}
                  onChange={(event) => setExportQuery(event.target.value)}
                  className="w-full h-10 rounded-md bg-base border border-border-default px-3 text-sm outline-none focus:border-border-focus"
                  placeholder="Message, filename, transcript, call ID"
                />
              </label>
              <label className="block">
                <span className="block text-xs text-tertiary mb-1">User filter</span>
                <input
                  value={exportUser}
                  onChange={(event) => setExportUser(event.target.value)}
                  className="w-full h-10 rounded-md bg-base border border-border-default px-3 text-sm outline-none focus:border-border-focus"
                  placeholder="sip:user@example.com"
                />
              </label>
              {rooms.length > 0 && (
                <label className="block">
                  <span className="block text-xs text-tertiary mb-1">Known room filter</span>
                  <select
                    value={rooms.some((room) => room.id === exportRoomId) ? exportRoomId : ""}
                    onChange={(event) => setExportRoomId(event.target.value)}
                    className="w-full h-10 rounded-md bg-base border border-border-default px-3 text-sm outline-none focus:border-border-focus"
                  >
                    <option value="">All rooms</option>
                    {rooms.map((room) => (
                      <option key={room.id} value={room.id}>{room.name}</option>
                    ))}
                  </select>
                </label>
              )}
              <label className="block">
                <span className="block text-xs text-tertiary mb-1">Room ID filter</span>
                <input
                  value={exportRoomId}
                  onChange={(event) => setExportRoomId(event.target.value)}
                  className="w-full h-10 rounded-md bg-base border border-border-default px-3 text-sm outline-none focus:border-border-focus"
                  placeholder="Blank exports all rooms"
                />
              </label>
              <div className="grid grid-cols-2 gap-2">
                <label className="block">
                  <span className="block text-xs text-tertiary mb-1">From</span>
                  <input
                    type="datetime-local"
                    value={exportFrom}
                    onChange={(event) => setExportFrom(event.target.value)}
                    className="w-full h-10 rounded-md bg-base border border-border-default px-3 text-sm outline-none focus:border-border-focus"
                  />
                </label>
                <label className="block">
                  <span className="block text-xs text-tertiary mb-1">To</span>
                  <input
                    type="datetime-local"
                    value={exportTo}
                    onChange={(event) => setExportTo(event.target.value)}
                    className="w-full h-10 rounded-md bg-base border border-border-default px-3 text-sm outline-none focus:border-border-focus"
                  />
                </label>
              </div>
              <label className="block">
                <span className="block text-xs text-tertiary mb-1">Result limit</span>
                <input
                  value={exportLimit}
                  onChange={(event) => setExportLimit(event.target.value)}
                  className="w-full h-10 rounded-md bg-base border border-border-default px-3 text-sm outline-none focus:border-border-focus"
                  type="number"
                  min={1}
                  max={1000}
                />
              </label>
              <button
                onClick={previewDiscovery}
                disabled={searchingDiscovery}
                className="w-full h-10 rounded-md border border-border-default hover:bg-elevated text-sm inline-flex items-center justify-center gap-2 disabled:opacity-60"
              >
                <Search size={16} />
                {searchingDiscovery ? "Searching..." : "Preview matches"}
              </button>
              <button
                onClick={exportDiscovery}
                className="w-full h-10 rounded-md bg-accent hover:bg-accent-hover text-white text-sm font-medium inline-flex items-center justify-center gap-2"
              >
                <Download size={16} />
                Download JSON
              </button>
              {exportSummary && (
                <div className="rounded-md border border-border-subtle bg-base p-3 text-sm">
                  <div className="font-medium">{exportSummary.count} items exported</div>
                  <div className="text-xs text-secondary">{roomName(exportSummary.room_id)} - {shortDate(exportSummary.exported_at)}</div>
                </div>
              )}
            </div>
          </section>
        </div>
      </div>
    </div>
  );
}

function Badge({ children, tone }: { children: React.ReactNode; tone: "ok" | "warn" }) {
  return (
    <span
      className={cn(
        "inline-flex h-6 items-center rounded px-2 text-xs",
        tone === "ok"
          ? "bg-emerald-500/10 text-emerald-500"
          : "bg-amber-500/10 text-amber-500"
      )}
    >
      {children}
    </span>
  );
}

function matchesFilePolicy(scope: string) {
  return scope === "global" || scope === "files" || scope === "file";
}

function matchesRecordingPolicy(scope: string) {
  return scope === "global" || scope === "recordings" || scope === "recording";
}

// ── DLP Panel ─────────────────────────────────────────────────────

function DlpPanel({ baseUrl, token }: { baseUrl: string; token: string }) {
  const [policies, setPolicies] = useState<any[]>([]);
  const [violations, setViolations] = useState<any[]>([]);
  const [creating, setCreating] = useState(false);
  const [selectedPolicyId, setSelectedPolicyId] = useState<string | null>(null);
  const [name, setName] = useState("");
  const [description, setDescription] = useState("");
  const [pattern, setPattern] = useState("");
  const [action, setAction] = useState("block");
  const [enabled, setEnabled] = useState(true);
  const [tab, setTab] = useState<"policies" | "violations">("policies");
  const [violationPolicy, setViolationPolicy] = useState("");
  const [violationUser, setViolationUser] = useState("");
  const [violationAction, setViolationAction] = useState<"all" | "block" | "warn" | "audit">("all");
  const [violationLimit, setViolationLimit] = useState("250");
  const [loadingViolations, setLoadingViolations] = useState(false);
  const [scanContent, setScanContent] = useState("");
  const [scanResult, setScanResult] = useState<any | null>(null);
  const [scanning, setScanning] = useState(false);

  const violationQuery = useCallback(() => {
    const params = new URLSearchParams();
    if (violationPolicy.trim()) params.set("policy", violationPolicy.trim());
    if (violationUser.trim()) params.set("user_uri", violationUser.trim());
    if (violationAction !== "all") params.set("action", violationAction);
    if (violationLimit.trim()) params.set("limit", violationLimit.trim());
    const query = params.toString();
    return query ? `?${query}` : "";
  }, [violationAction, violationLimit, violationPolicy, violationUser]);

  const load = () => {
    if (!token) return;
    api(baseUrl, token, "/v1/admin/dlp/policies").then(setPolicies).catch(() => {});
    api(baseUrl, token, `/v1/admin/dlp/violations${violationQuery()}`).then(setViolations).catch(() => {});
  };

  useEffect(load, [baseUrl, token, violationQuery]);

  const refreshViolations = async () => {
    if (!token) return;
    setLoadingViolations(true);
    try {
      setViolations(await api<any[]>(baseUrl, token, `/v1/admin/dlp/violations${violationQuery()}`));
    } catch {
      toast({ type: "error", title: "Unable to load DLP violations" });
    } finally {
      setLoadingViolations(false);
    }
  };

  const exportViolations = async () => {
    try {
      const response = await fetch(`${baseUrl.replace(/\/$/, "")}/v1/admin/dlp/violations/export.csv${violationQuery()}`, {
        headers: { Authorization: `Bearer ${token}` },
      });
      if (!response.ok) throw new Error(`Export failed (${response.status})`);
      const blob = await response.blob();
      const url = URL.createObjectURL(blob);
      const a = document.createElement("a");
      a.href = url;
      a.download = `dlp-violations-${new Date().toISOString().slice(0, 10)}.csv`;
      a.click();
      URL.revokeObjectURL(url);
    } catch (err) {
      toast({ type: "error", title: err instanceof Error ? err.message : "Unable to export DLP violations" });
    }
  };

  const testDlpContent = async () => {
    if (!scanContent.trim()) return;
    setScanning(true);
    try {
      const result = await api<any>(baseUrl, token, "/v1/admin/dlp/scan", {
        method: "POST",
        body: { content: scanContent },
      });
      setScanResult(result);
      toast({
        type: result.allowed ? "success" : "info",
        title: result.allowed ? "No DLP policies matched" : `${result.violations?.length ?? 0} DLP policies matched`,
      });
    } catch (err) {
      toast({ type: "error", title: err instanceof Error ? err.message : "DLP test failed" });
    } finally {
      setScanning(false);
    }
  };

  const resetPolicyForm = () => {
    setSelectedPolicyId(null);
    setName("");
    setDescription("");
    setPattern("");
    setAction("block");
    setEnabled(true);
    setCreating(false);
  };

  const editPolicy = (policy: any) => {
    setSelectedPolicyId(policy.id);
    setName(policy.name ?? "");
    setDescription(policy.description ?? "");
    setPattern(policy.pattern ?? "");
    setAction(policy.action ?? "block");
    setEnabled(Boolean(policy.enabled));
    setCreating(true);
    setTab("policies");
  };

  const savePolicy = async () => {
    if (!name || !pattern) return;
    try {
      await api(baseUrl, token, selectedPolicyId ? `/v1/admin/dlp/policies/${selectedPolicyId}` : "/v1/admin/dlp/policies", {
        method: selectedPolicyId ? "PUT" : "POST",
        body: { name, description, pattern, action, enabled },
      });
      resetPolicyForm();
      load();
      toast({ type: "info", title: selectedPolicyId ? "DLP policy updated" : "DLP policy created" });
    } catch (err) {
      toast({ type: "error", title: err instanceof Error ? err.message : "DLP policy save failed" });
    }
  };

  const togglePolicy = async (policy: any) => {
    try {
      await api(baseUrl, token, `/v1/admin/dlp/policies/${policy.id}`, {
        method: "PUT",
        body: { enabled: !policy.enabled },
      });
      load();
    } catch { toast({ type: "error", title: "Failed" }); }
  };

  const handleDeleteDlpPolicy = async (id: string) => {
    try {
      await api(baseUrl, token, `/v1/admin/dlp/policies/${id}`, { method: "DELETE" });
      load();
    } catch { toast({ type: "error", title: "Failed" }); }
  };

  return (
    <div className="space-y-4">
      <div className="flex items-center gap-2">
        <button
          onClick={() => setTab("policies")}
          className={cn("px-3 py-1.5 rounded text-sm", tab === "policies" ? "bg-accent text-white" : "hover:bg-hover")}
        >
          Policies ({policies.length})
        </button>
        <button
          onClick={() => setTab("violations")}
          className={cn("px-3 py-1.5 rounded text-sm", tab === "violations" ? "bg-accent text-white" : "hover:bg-hover")}
        >
          Violations ({violations.length})
        </button>
        <button onClick={() => creating ? resetPolicyForm() : setCreating(true)} className="ml-auto flex items-center gap-1 px-3 py-1.5 bg-accent text-white rounded text-sm">
          <Plus size={14} /> New Policy
        </button>
      </div>

      {creating && (
        <div className="p-3 border border-border-subtle rounded space-y-2">
          <div className="text-sm font-medium">{selectedPolicyId ? "Edit DLP policy" : "New DLP policy"}</div>
          <input className="w-full rounded border border-border-subtle bg-input px-3 py-2 text-sm" placeholder="Policy name (e.g. Credit Card Numbers)" value={name} onChange={(e) => setName(e.target.value)} />
          <input className="w-full rounded border border-border-subtle bg-input px-3 py-2 text-sm" placeholder="Description" value={description} onChange={(e) => setDescription(e.target.value)} />
          <input className="w-full rounded border border-border-subtle bg-input px-3 py-2 text-sm font-mono" placeholder="Regex pattern (e.g. \b\d{4}[- ]?\d{4}[- ]?\d{4}[- ]?\d{4}\b)" value={pattern} onChange={(e) => setPattern(e.target.value)} />
          <select className="w-full rounded border border-border-subtle bg-input px-3 py-2 text-sm" value={action} onChange={(e) => setAction(e.target.value)}>
            <option value="block">Block</option>
            <option value="warn">Warn</option>
            <option value="audit">Audit Only</option>
          </select>
          <label className="inline-flex items-center gap-2 text-sm text-secondary">
            <input type="checkbox" checked={enabled} onChange={(event) => setEnabled(event.target.checked)} className="accent-accent" />
            Enabled
          </label>
          <div className="flex gap-2">
            <button onClick={savePolicy} className="px-4 py-2 bg-accent text-white rounded text-sm">{selectedPolicyId ? "Update" : "Create"}</button>
            <button onClick={resetPolicyForm} className="px-4 py-2 border border-border-default rounded text-sm hover:bg-elevated">Cancel</button>
          </div>
        </div>
      )}

      {tab === "policies" && (
        <div className="space-y-3">
        <section className="border border-border-subtle bg-surface rounded-md overflow-hidden">
          <div className="p-3 border-b border-border-subtle flex items-center justify-between gap-3">
            <h2 className="font-medium">Policy tester</h2>
            <button
              onClick={testDlpContent}
              disabled={scanning || !scanContent.trim()}
              className="h-8 px-3 rounded-md bg-accent hover:bg-accent-hover text-white text-sm disabled:opacity-60"
            >
              {scanning ? "Testing" : "Test content"}
            </button>
          </div>
          <div className="p-3 grid lg:grid-cols-[1fr_320px] gap-3">
            <textarea
              value={scanContent}
              onChange={(event) => setScanContent(event.target.value)}
              placeholder="Paste sample content to test against enabled DLP policies"
              className="min-h-28 rounded-md bg-base border border-border-default px-3 py-2 text-sm outline-none focus:border-border-focus resize-y"
            />
            <div className="rounded-md border border-border-subtle bg-base p-3 text-sm">
              {!scanResult ? (
                <div className="text-secondary">No test result</div>
              ) : scanResult.violations?.length ? (
                <div className="space-y-2">
                  <div className="font-medium text-destructive">Matched {scanResult.violations.length} policy</div>
                  {scanResult.violations.map((violation: any) => (
                    <div key={violation.id} className="rounded border border-border-subtle p-2">
                      <div className="font-medium">{violation.policy_name}</div>
                      <div className="text-xs text-secondary capitalize">{violation.action_taken}</div>
                    </div>
                  ))}
                </div>
              ) : (
                <div className="text-success">No policies matched</div>
              )}
            </div>
          </div>
        </section>
        <section className="border border-border-subtle bg-surface rounded-md overflow-hidden">
          <h2 className="p-3 border-b border-border-subtle font-medium">DLP Policies</h2>
          <table className="w-full text-sm">
            <thead className="text-tertiary">
              <tr className="border-b border-border-subtle">
                {["Name", "Pattern", "Action", "Enabled", "Created", ""].map((col) => (
                  <th key={col} className="text-left py-2 px-2 font-medium">{col}</th>
                ))}
              </tr>
            </thead>
            <tbody>
              {policies.length === 0 ? (
                <tr><td className="py-4 px-2 text-secondary" colSpan={6}>No policies</td></tr>
              ) : (
                policies.map((p: any) => (
                  <tr key={p.id} className="border-b border-border-subtle last:border-b-0">
                    <td className="py-2 px-2">{p.name}</td>
                    <td className="py-2 px-2 max-w-[200px] truncate font-mono text-xs">{p.pattern}</td>
                    <td className="py-2 px-2">{p.action}</td>
                    <td className="py-2 px-2">{p.enabled ? "Yes" : "No"}</td>
                    <td className="py-2 px-2">{shortDate(p.created_at)}</td>
                    <td className="py-2 px-2">
                      <div className="flex items-center gap-2">
                      <button onClick={() => editPolicy(p)} className="text-xs text-accent hover:underline">
                        Edit
                      </button>
                      <button onClick={() => togglePolicy(p)} className="text-xs text-secondary hover:text-primary">
                        {p.enabled ? "Disable" : "Enable"}
                      </button>
                      <button onClick={() => handleDeleteDlpPolicy(p.id)} className="text-red-500 hover:text-red-400">
                        <Trash2 size={14} />
                      </button>
                      </div>
                    </td>
                  </tr>
                ))
              )}
            </tbody>
          </table>
        </section>
        </div>
      )}

      {tab === "violations" && (
        <div className="space-y-3">
          <div className="rounded-md border border-border-subtle bg-surface p-3">
            <div className="grid grid-cols-1 md:grid-cols-[1fr_1fr_130px_100px_auto_auto] gap-2">
              <input
                value={violationPolicy}
                onChange={(event) => setViolationPolicy(event.target.value)}
                placeholder="Policy"
                className="h-10 rounded-md bg-base border border-border-default px-3 text-sm outline-none focus:border-border-focus"
              />
              <input
                value={violationUser}
                onChange={(event) => setViolationUser(event.target.value)}
                placeholder="User"
                className="h-10 rounded-md bg-base border border-border-default px-3 text-sm outline-none focus:border-border-focus"
              />
              <select
                value={violationAction}
                onChange={(event) => setViolationAction(event.target.value as "all" | "block" | "warn" | "audit")}
                className="h-10 rounded-md bg-base border border-border-default px-3 text-sm outline-none focus:border-border-focus"
              >
                <option value="all">All actions</option>
                <option value="block">Block</option>
                <option value="warn">Warn</option>
                <option value="audit">Audit</option>
              </select>
              <input
                value={violationLimit}
                onChange={(event) => setViolationLimit(event.target.value)}
                inputMode="numeric"
                placeholder="Limit"
                className="h-10 rounded-md bg-base border border-border-default px-3 text-sm outline-none focus:border-border-focus"
              />
              <button
                onClick={refreshViolations}
                disabled={loadingViolations}
                className="h-10 px-3 rounded-md border border-border-default hover:bg-elevated text-sm disabled:opacity-60"
              >
                {loadingViolations ? "Loading" : "Refresh"}
              </button>
              <button
                onClick={exportViolations}
                className="h-10 px-3 rounded-md bg-accent hover:bg-accent-hover text-white text-sm inline-flex items-center justify-center gap-2"
              >
                <Download size={15} />
                CSV
              </button>
            </div>
          </div>
          <Table
            title="DLP Violations"
            columns={["Policy", "User", "Action", "Snippet", "Detected"]}
            rows={violations.map((v: any) => [
              v.policy_name,
              v.user_uri?.replace(/^sip:/, "") ?? "",
              v.action_taken,
              v.content_snippet?.slice(0, 80) ?? "",
              shortDate(v.detected_at),
            ])}
          />
        </div>
      )}
    </div>
  );
}

// ── Information Barriers Panel ─────────────────────────────────────

function BarriersPanel({ baseUrl, token }: { baseUrl: string; token: string }) {
  const [barriers, setBarriers] = useState<any[]>([]);
  const [creating, setCreating] = useState(false);
  const [name, setName] = useState("");
  const [seg1Name, setSeg1Name] = useState("");
  const [seg1Users, setSeg1Users] = useState("");
  const [seg2Name, setSeg2Name] = useState("");
  const [seg2Users, setSeg2Users] = useState("");
  const [blockChat, setBlockChat] = useState(true);
  const [blockCall, setBlockCall] = useState(true);

  const load = useCallback(() => {
    api(baseUrl, token, "/v1/admin/barriers").then(setBarriers).catch(() => {});
  }, [baseUrl, token]);

  useEffect(load, [load]);

  const submit = async (event: FormEvent) => {
    event.preventDefault();
    try {
      await api(baseUrl, token, "/v1/admin/barriers", {
        method: "POST",
        body: {
          name,
          segment1_name: seg1Name,
          segment1_users: seg1Users.split(",").map((u: string) => u.trim()).filter(Boolean),
          segment2_name: seg2Name,
          segment2_users: seg2Users.split(",").map((u: string) => u.trim()).filter(Boolean),
          block_chat: blockChat,
          block_call: blockCall,
        },
      });
      setName(""); setSeg1Name(""); setSeg1Users(""); setSeg2Name(""); setSeg2Users("");
      setCreating(false);
      toast({ type: "success", title: "Information barrier created" });
      load();
    } catch (err) {
      toast({ type: "error", title: err instanceof Error ? err.message : "Failed" });
    }
  };

  const remove = async (id: string) => {
    try {
      await api(baseUrl, token, `/v1/admin/barriers/${id}`, { method: "DELETE" });
      toast({ type: "success", title: "Barrier deleted" });
      load();
    } catch { toast({ type: "error", title: "Failed to delete" }); }
  };

  return (
    <section className="border border-border-subtle bg-surface rounded-md overflow-hidden">
      <div className="p-3 border-b border-border-subtle flex items-center justify-between">
        <div className="flex items-center gap-2">
          <Shield size={17} className="text-accent" />
          <h2 className="font-medium">Information Barriers</h2>
        </div>
        <button onClick={() => setCreating(!creating)} className="h-8 px-3 rounded-md bg-accent hover:bg-accent-hover text-white text-sm flex items-center gap-2">
          <Plus size={14} /> New Barrier
        </button>
      </div>
      {creating && (
        <form onSubmit={submit} className="p-3 border-b border-border-subtle space-y-2">
          <div className="grid md:grid-cols-3 gap-2">
            <Field label="Barrier Name" value={name} onChange={setName} />
            <Field label="Segment 1 Name" value={seg1Name} onChange={setSeg1Name} />
            <Field label="Segment 1 Users (comma-separated)" value={seg1Users} onChange={setSeg1Users} />
          </div>
          <div className="grid md:grid-cols-3 gap-2">
            <Field label="Segment 2 Name" value={seg2Name} onChange={setSeg2Name} />
            <Field label="Segment 2 Users (comma-separated)" value={seg2Users} onChange={setSeg2Users} />
            <div className="flex gap-4 items-end pb-1">
              <label className="flex items-center gap-2 text-sm">
                <input type="checkbox" checked={blockChat} onChange={(e) => setBlockChat(e.target.checked)} /> Block Chat
              </label>
              <label className="flex items-center gap-2 text-sm">
                <input type="checkbox" checked={blockCall} onChange={(e) => setBlockCall(e.target.checked)} /> Block Calls
              </label>
            </div>
          </div>
          <button className="h-9 px-4 rounded-md bg-accent hover:bg-accent-hover text-white text-sm">Create</button>
        </form>
      )}
      <div className="p-3 overflow-x-auto">
        <Table
          title="Barriers"
          columns={["Name", "Segment 1", "Segment 2", "Chat", "Call", "Enabled"]}
          rows={barriers.map((b: any) => [
            b.name,
            `${b.segment1_name} (${(b.segment1_users || []).length})`,
            `${b.segment2_name} (${(b.segment2_users || []).length})`,
            b.block_chat ? "Blocked" : "Allowed",
            b.block_call ? "Blocked" : "Allowed",
            b.enabled ? "Yes" : "No",
          ])}
        />
        {barriers.length > 0 && (
          <div className="mt-2 flex flex-wrap gap-2">
            {barriers.map((b: any) => (
              <button key={b.id} onClick={() => remove(b.id)} className="text-xs text-destructive hover:underline">
                Delete "{b.name}"
              </button>
            ))}
          </div>
        )}
      </div>
    </section>
  );
}

// ── Sensitivity Labels Panel ──────────────────────────────────────

function LabelsPanel({ baseUrl, token }: { baseUrl: string; token: string }) {
  const [labels, setLabels] = useState<any[]>([]);
  const [creating, setCreating] = useState(false);
  const [name, setName] = useState("");
  const [description, setDescription] = useState("");
  const [color, setColor] = useState("#6b7280");
  const [priority, setPriority] = useState("0");
  const [encrypt, setEncrypt] = useState(false);
  const [restrictSharing, setRestrictSharing] = useState(false);
  const [watermark, setWatermark] = useState(false);

  const load = useCallback(() => {
    api(baseUrl, token, "/v1/admin/labels").then(setLabels).catch(() => {});
  }, [baseUrl, token]);

  useEffect(load, [load]);

  const submit = async (event: FormEvent) => {
    event.preventDefault();
    try {
      await api(baseUrl, token, "/v1/admin/labels", {
        method: "POST",
        body: {
          name,
          description,
          color,
          priority: parseInt(priority) || 0,
          encrypt_content: encrypt,
          restrict_sharing: restrictSharing,
          watermark,
        },
      });
      setName(""); setDescription(""); setColor("#6b7280"); setPriority("0");
      setEncrypt(false); setRestrictSharing(false); setWatermark(false);
      setCreating(false);
      toast({ type: "success", title: "Sensitivity label created" });
      load();
    } catch (err) {
      toast({ type: "error", title: err instanceof Error ? err.message : "Failed" });
    }
  };

  const remove = async (id: string) => {
    try {
      await api(baseUrl, token, `/v1/admin/labels/${id}`, { method: "DELETE" });
      toast({ type: "success", title: "Label deleted" }); load();
    } catch { toast({ type: "error", title: "Failed to delete" }); }
  };

  return (
    <section className="border border-border-subtle bg-surface rounded-md overflow-hidden">
      <div className="p-3 border-b border-border-subtle flex items-center justify-between">
        <div className="flex items-center gap-2">
          <FileText size={17} className="text-accent" />
          <h2 className="font-medium">Sensitivity Labels</h2>
        </div>
        <button onClick={() => setCreating(!creating)} className="h-8 px-3 rounded-md bg-accent hover:bg-accent-hover text-white text-sm flex items-center gap-2">
          <Plus size={14} /> New Label
        </button>
      </div>
      {creating && (
        <form onSubmit={submit} className="p-3 border-b border-border-subtle space-y-2">
          <div className="grid md:grid-cols-4 gap-2">
            <Field label="Name" value={name} onChange={setName} />
            <Field label="Description" value={description} onChange={setDescription} />
            <Field label="Color" value={color} onChange={setColor} />
            <Field label="Priority" value={priority} onChange={setPriority} />
          </div>
          <div className="flex gap-4">
            <label className="flex items-center gap-2 text-sm">
              <input type="checkbox" checked={encrypt} onChange={(e) => setEncrypt(e.target.checked)} /> Encrypt Content
            </label>
            <label className="flex items-center gap-2 text-sm">
              <input type="checkbox" checked={restrictSharing} onChange={(e) => setRestrictSharing(e.target.checked)} /> Restrict Sharing
            </label>
            <label className="flex items-center gap-2 text-sm">
              <input type="checkbox" checked={watermark} onChange={(e) => setWatermark(e.target.checked)} /> Watermark
            </label>
          </div>
          <button className="h-9 px-4 rounded-md bg-accent hover:bg-accent-hover text-white text-sm">Create</button>
        </form>
      )}
      <div className="p-3 overflow-x-auto">
        <table className="w-full text-sm">
          <thead className="text-tertiary">
            <tr className="border-b border-border-subtle">
              <th className="text-left py-2 px-2 font-medium">Color</th>
              <th className="text-left py-2 px-2 font-medium">Name</th>
              <th className="text-left py-2 px-2 font-medium">Priority</th>
              <th className="text-left py-2 px-2 font-medium">Encrypt</th>
              <th className="text-left py-2 px-2 font-medium">Restrict</th>
              <th className="text-left py-2 px-2 font-medium">Watermark</th>
              <th className="text-left py-2 px-2 font-medium"></th>
            </tr>
          </thead>
          <tbody>
            {labels.length === 0 ? (
              <tr><td colSpan={7} className="py-4 px-2 text-secondary">No labels</td></tr>
            ) : labels.map((label: any) => (
              <tr key={label.id} className="border-b border-border-subtle">
                <td className="py-2 px-2"><span className="inline-block w-4 h-4 rounded" style={{ backgroundColor: label.color }} /></td>
                <td className="py-2 px-2">{label.name}</td>
                <td className="py-2 px-2">{label.priority}</td>
                <td className="py-2 px-2">{label.encrypt_content ? "Yes" : "No"}</td>
                <td className="py-2 px-2">{label.restrict_sharing ? "Yes" : "No"}</td>
                <td className="py-2 px-2">{label.watermark ? "Yes" : "No"}</td>
                <td className="py-2 px-2 text-right">
                  <IconButton label="Delete" tone="danger" onClick={() => remove(label.id)}><Trash2 size={16} /></IconButton>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </section>
  );
}

// ── Custom Roles Panel ────────────────────────────────────────────

function RolesPanel({ baseUrl, token }: { baseUrl: string; token: string }) {
  const [roles, setRoles] = useState<any[]>([]);
  const [allPermissions, setAllPermissions] = useState<string[]>([]);
  const [creating, setCreating] = useState(false);
  const [name, setName] = useState("");
  const [selectedPerms, setSelectedPerms] = useState<Set<string>>(new Set());

  const load = useCallback(() => {
    api(baseUrl, token, "/v1/admin/roles").then(setRoles).catch(() => {});
    api<string[]>(baseUrl, token, "/v1/admin/roles/permissions").then(setAllPermissions).catch(() => {});
  }, [baseUrl, token]);

  useEffect(load, [load]);

  const togglePerm = (perm: string) => {
    const next = new Set(selectedPerms);
    if (next.has(perm)) next.delete(perm); else next.add(perm);
    setSelectedPerms(next);
  };

  const submit = async (event: FormEvent) => {
    event.preventDefault();
    try {
      await api(baseUrl, token, "/v1/admin/roles", {
        method: "POST",
        body: { name, permissions: Array.from(selectedPerms) },
      });
      setName(""); setSelectedPerms(new Set()); setCreating(false);
      toast({ type: "success", title: "Role created" }); load();
    } catch (err) {
      toast({ type: "error", title: err instanceof Error ? err.message : "Failed" });
    }
  };

  const remove = async (id: string) => {
    try {
      await api(baseUrl, token, `/v1/admin/roles/${id}`, { method: "DELETE" });
      toast({ type: "success", title: "Role deleted" }); load();
    } catch { toast({ type: "error", title: "Failed to delete" }); }
  };

  return (
    <section className="border border-border-subtle bg-surface rounded-md overflow-hidden">
      <div className="p-3 border-b border-border-subtle flex items-center justify-between">
        <div className="flex items-center gap-2">
          <Shield size={17} className="text-accent" />
          <h2 className="font-medium">Custom RBAC Roles</h2>
        </div>
        <button onClick={() => setCreating(!creating)} className="h-8 px-3 rounded-md bg-accent hover:bg-accent-hover text-white text-sm flex items-center gap-2">
          <Plus size={14} /> New Role
        </button>
      </div>
      {creating && (
        <form onSubmit={submit} className="p-3 border-b border-border-subtle space-y-2">
          <Field label="Role Name" value={name} onChange={setName} />
          <div>
            <span className="block text-xs text-tertiary mb-1">Permissions</span>
            <div className="flex flex-wrap gap-2">
              {allPermissions.map((perm) => (
                <label key={perm} className="flex items-center gap-1.5 text-sm">
                  <input type="checkbox" checked={selectedPerms.has(perm)} onChange={() => togglePerm(perm)} />
                  {perm.replace(/_/g, " ")}
                </label>
              ))}
            </div>
          </div>
          <button className="h-9 px-4 rounded-md bg-accent hover:bg-accent-hover text-white text-sm">Create</button>
        </form>
      )}
      <div className="p-3 overflow-x-auto">
        <table className="w-full text-sm">
          <thead className="text-tertiary">
            <tr className="border-b border-border-subtle">
              <th className="text-left py-2 px-2 font-medium">Name</th>
              <th className="text-left py-2 px-2 font-medium">Permissions</th>
              <th className="text-left py-2 px-2 font-medium">Created</th>
              <th className="text-left py-2 px-2 font-medium"></th>
            </tr>
          </thead>
          <tbody>
            {roles.length === 0 ? (
              <tr><td colSpan={4} className="py-4 px-2 text-secondary">No custom roles</td></tr>
            ) : roles.map((role: any) => (
              <tr key={role.id} className="border-b border-border-subtle">
                <td className="py-2 px-2 font-medium">{role.name}</td>
                <td className="py-2 px-2 max-w-[400px] truncate">{(role.permissions || []).join(", ")}</td>
                <td className="py-2 px-2">{shortDate(role.created_at)}</td>
                <td className="py-2 px-2 text-right">
                  <IconButton label="Delete" tone="danger" onClick={() => remove(role.id)}><Trash2 size={16} /></IconButton>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </section>
  );
}

// ── Policy Packages Panel ─────────────────────────────────────────

function PackagesPanel({ baseUrl, token }: { baseUrl: string; token: string }) {
  const [packages, setPackages] = useState<any[]>([]);
  const [creating, setCreating] = useState(false);
  const [name, setName] = useState("");
  const [description, setDescription] = useState("");
  const [policies, setPolicies] = useState("{}");

  const load = useCallback(() => {
    api(baseUrl, token, "/v1/admin/policy-packages").then(setPackages).catch(() => {});
  }, [baseUrl, token]);

  useEffect(load, [load]);

  const submit = async (event: FormEvent) => {
    event.preventDefault();
    try {
      let parsedPolicies;
      try { parsedPolicies = JSON.parse(policies); } catch { toast({ type: "error", title: "Invalid JSON" }); return; }
      await api(baseUrl, token, "/v1/admin/policy-packages", {
        method: "POST",
        body: { name, description, policies: parsedPolicies },
      });
      setName(""); setDescription(""); setPolicies("{}"); setCreating(false);
      toast({ type: "success", title: "Policy package created" }); load();
    } catch (err) {
      toast({ type: "error", title: err instanceof Error ? err.message : "Failed" });
    }
  };

  const remove = async (id: string) => {
    try {
      await api(baseUrl, token, `/v1/admin/policy-packages/${id}`, { method: "DELETE" });
      toast({ type: "success", title: "Package deleted" }); load();
    } catch { toast({ type: "error", title: "Failed to delete" }); }
  };

  return (
    <section className="border border-border-subtle bg-surface rounded-md overflow-hidden">
      <div className="p-3 border-b border-border-subtle flex items-center justify-between">
        <div className="flex items-center gap-2">
          <ClipboardList size={17} className="text-accent" />
          <h2 className="font-medium">Policy Packages</h2>
        </div>
        <button onClick={() => setCreating(!creating)} className="h-8 px-3 rounded-md bg-accent hover:bg-accent-hover text-white text-sm flex items-center gap-2">
          <Plus size={14} /> New Package
        </button>
      </div>
      {creating && (
        <form onSubmit={submit} className="p-3 border-b border-border-subtle space-y-2">
          <div className="grid md:grid-cols-2 gap-2">
            <Field label="Name" value={name} onChange={setName} />
            <Field label="Description" value={description} onChange={setDescription} />
          </div>
          <JsonField label="Policies (JSON)" value={policies} onChange={setPolicies} />
          <button className="h-9 px-4 rounded-md bg-accent hover:bg-accent-hover text-white text-sm">Create</button>
        </form>
      )}
      <div className="p-3 overflow-x-auto">
        <Table
          title="Packages"
          columns={["Name", "Description", "Created"]}
          rows={packages.map((p: any) => [p.name, p.description || "-", shortDate(p.created_at)])}
        />
        {packages.length > 0 && (
          <div className="mt-2 flex flex-wrap gap-2">
            {packages.map((p: any) => (
              <button key={p.id} onClick={() => remove(p.id)} className="text-xs text-destructive hover:underline">
                Delete "{p.name}"
              </button>
            ))}
          </div>
        )}
      </div>
    </section>
  );
}

// ── Analytics Panel ───────────────────────────────────────────────

function AnalyticsPanel({ baseUrl, token }: { baseUrl: string; token: string }) {
  const [analytics, setAnalytics] = useState<any | null>(null);
  const [loading, setLoading] = useState(false);

  const load = useCallback(async () => {
    setLoading(true);
    try {
      setAnalytics(await api(baseUrl, token, "/v1/admin/analytics"));
    } catch {
      toast({ type: "error", title: "Failed to load analytics" });
    } finally {
      setLoading(false);
    }
  }, [baseUrl, token]);

  useEffect(() => { load(); }, [load]);

  if (!analytics) {
    return (
      <section className="border border-border-subtle bg-surface rounded-md p-6 text-center text-secondary">
        {loading ? "Loading analytics..." : "No data"}
      </section>
    );
  }

  return (
    <section className="space-y-4">
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-2">
          <BarChart3 size={17} className="text-accent" />
          <h2 className="font-medium text-lg">Usage Analytics</h2>
        </div>
        <button onClick={load} disabled={loading} className="h-9 px-3 rounded-md border border-border-default hover:bg-elevated text-sm flex items-center gap-2 disabled:opacity-60">
          <RefreshCw size={16} className={loading ? "animate-spin" : ""} /> Refresh
        </button>
      </div>
      <div className="grid grid-cols-2 md:grid-cols-4 gap-3">
        <Metric label="Total Users" value={analytics.total_users} />
        <Metric label="Active Users" value={analytics.active_users} />
        <Metric label="Online Now" value={analytics.online_users} />
        <Metric label="Total Messages" value={analytics.total_messages} />
        <Metric label="Total Calls" value={analytics.total_calls} />
        <Metric label="Meetings" value={analytics.total_meetings} />
        <Metric label="Files" value={analytics.total_files} />
        <Metric label="Storage (MB)" value={Math.round((analytics.total_storage_bytes || 0) / 1048576)} />
      </div>

      {/* Users section with import/export */}
      <div className="border border-border-subtle bg-surface rounded-md overflow-hidden">
        <div className="p-3 border-b border-border-subtle flex items-center justify-between">
          <h3 className="font-medium">Bulk User Operations</h3>
          <div className="flex items-center gap-2">
            <label className="h-8 px-3 rounded-md border border-border-default hover:bg-elevated text-sm flex items-center gap-2 cursor-pointer">
              <UserPlus size={14} /> Import CSV
              <input type="file" accept=".csv" className="hidden" onChange={async (e) => {
                const file = e.target.files?.[0];
                if (!file) return;
                const text = await file.text();
                try {
                  const response = await fetch(`${baseUrl.replace(/\/$/, "")}/v1/admin/users/import`, {
                    method: "POST",
                    headers: { Authorization: `Bearer ${token}`, "Content-Type": "text/csv" },
                    body: text,
                  });
                  if (!response.ok) throw new Error(`Import failed (${response.status})`);
                  const result = await response.json();
                  toast({ type: "success", title: `Imported ${result.imported}, skipped ${result.skipped}` });
                  if (result.errors?.length) toast({ type: "info", title: `${result.errors.length} errors` });
                } catch (err) {
                  toast({ type: "error", title: err instanceof Error ? err.message : "Import failed" });
                }
                e.target.value = "";
              }} />
            </label>
            <button
              onClick={async () => {
                try {
                  const response = await fetch(`${baseUrl.replace(/\/$/, "")}/v1/admin/users/export`, {
                    headers: { Authorization: `Bearer ${token}` },
                  });
                  if (!response.ok) throw new Error(`Export failed (${response.status})`);
                  const blob = await response.blob();
                  const url = URL.createObjectURL(blob);
                  const a = document.createElement("a");
                  a.href = url;
                  a.download = `users-${new Date().toISOString().slice(0, 10)}.csv`;
                  a.click();
                  URL.revokeObjectURL(url);
                } catch (err) {
                  toast({ type: "error", title: err instanceof Error ? err.message : "Export failed" });
                }
              }}
              className="h-8 px-3 rounded-md bg-accent hover:bg-accent-hover text-white text-sm flex items-center gap-2"
            >
              <Download size={14} /> Export CSV
            </button>
          </div>
        </div>
        <div className="p-3 text-sm text-secondary">
          CSV format for import: <code className="bg-base px-1 rounded">display_name,sip_uri,password,role</code>
        </div>
      </div>
    </section>
  );
}


// ─── Recording Policies Panel ───

interface RecordingPolicy {
  id: string;
  name: string;
  trigger: string;
  target_ids: string[];
  enabled: boolean;
  created_at: string;
}

function RecordingPoliciesPanel({ baseUrl, token }: { baseUrl: string; token: string }) {
  const [policies, setPolicies] = useState<RecordingPolicy[]>([]);
  const [name, setName] = useState("");
  const [trigger, setTrigger] = useState("all_calls");
  const [targetIds, setTargetIds] = useState("");

  useEffect(() => {
    api<RecordingPolicy[]>(baseUrl, token, "/v1/admin/recording-policies")
      .then(setPolicies)
      .catch(() => {});
  }, [baseUrl, token]);

  const create = async () => {
    if (!name) return;
    try {
      const policy = await api<RecordingPolicy>(baseUrl, token, "/v1/admin/recording-policies", {
        method: "POST",
        body: {
          name,
          trigger,
          target_ids: targetIds ? targetIds.split(",").map((s) => s.trim()) : [],
          enabled: true,
        },
      });
      setPolicies([...policies, policy]);
      setName("");
      setTargetIds("");
      toast({ type: "success", title: "Recording policy created" });
    } catch (err) {
      toast({ type: "error", title: "Failed", description: String(err) });
    }
  };

  const remove = async (id: string) => {
    try {
      await api(baseUrl, token, `/v1/admin/recording-policies/${id}`, { method: "DELETE" });
      setPolicies(policies.filter((p) => p.id !== id));
      toast({ type: "success", title: "Policy deleted" });
    } catch (err) {
      toast({ type: "error", title: "Failed", description: String(err) });
    }
  };

  return (
    <div className="space-y-4">
      <h2 className="text-base font-semibold">Recording Policies</h2>
      <p className="text-sm text-secondary">Auto-record calls based on compliance policies.</p>

      <div className="grid grid-cols-1 md:grid-cols-3 gap-2">
        <Field label="Name" value={name} onChange={setName} />
        <div>
          <label className="text-xs font-medium text-secondary block mb-1">Trigger</label>
          <select
            value={trigger}
            onChange={(e) => setTrigger(e.target.value)}
            className="w-full h-10 rounded-md border border-border-default bg-surface px-3 text-sm"
          >
            <option value="all_calls">All Calls</option>
            <option value="all_external">All External</option>
            <option value="specific_users">Specific Users</option>
            <option value="specific_queues">Specific Queues</option>
          </select>
        </div>
        <Field label="Target IDs (comma-sep)" value={targetIds} onChange={setTargetIds} />
      </div>
      <button onClick={create} className="h-9 px-4 rounded-md bg-accent text-white text-sm font-medium">
        <Plus size={14} className="inline mr-1" />Create Policy
      </button>

      <div className="space-y-2">
        {policies.map((p) => (
          <div key={p.id} className="flex items-center justify-between p-3 rounded-md border border-border-default bg-surface">
            <div>
              <p className="text-sm font-medium">{p.name}</p>
              <p className="text-xs text-secondary">
                Trigger: {p.trigger} | Targets: {p.target_ids.join(", ") || "all"} | {p.enabled ? "Enabled" : "Disabled"}
              </p>
            </div>
            <button onClick={() => remove(p.id)} className="p-1 text-destructive hover:text-destructive/80">
              <Trash2 size={14} />
            </button>
          </div>
        ))}
        {policies.length === 0 && <p className="text-sm text-tertiary">No recording policies configured.</p>}
      </div>
    </div>
  );
}

// ─── Hold Music Panel ───

interface HoldMusicEntry {
  id: string;
  name: string;
  file_path: string;
  queue_id: string | null;
  is_default: boolean;
  uploaded_by: string;
  created_at: string;
}

function HoldMusicPanel({ baseUrl, token }: { baseUrl: string; token: string }) {
  const [entries, setEntries] = useState<HoldMusicEntry[]>([]);
  const fileInputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    api<HoldMusicEntry[]>(baseUrl, token, "/v1/admin/hold-music")
      .then(setEntries)
      .catch(() => {});
  }, [baseUrl, token]);

  const upload = async (file: File) => {
    try {
      const buffer = await file.arrayBuffer();
      const response = await fetch(`${baseUrl.replace(/\/+$/, "")}/v1/admin/hold-music`, {
        method: "POST",
        headers: {
          "Content-Type": file.type || "audio/mpeg",
          Authorization: `Bearer ${token}`,
          "X-Pale-Filename": file.name,
        },
        body: buffer,
      });
      if (!response.ok) throw new Error("Upload failed");
      const entry = await response.json();
      setEntries([...entries, entry]);
      toast({ type: "success", title: "Hold music uploaded" });
    } catch (err) {
      toast({ type: "error", title: "Upload failed", description: String(err) });
    }
  };

  const remove = async (id: string) => {
    try {
      await api(baseUrl, token, `/v1/admin/hold-music/${id}`, { method: "DELETE" });
      setEntries(entries.filter((e) => e.id !== id));
      toast({ type: "success", title: "Hold music deleted" });
    } catch (err) {
      toast({ type: "error", title: "Failed", description: String(err) });
    }
  };

  return (
    <div className="space-y-4">
      <h2 className="text-base font-semibold">Hold Music</h2>
      <p className="text-sm text-secondary">Configure custom music on hold for calls and queues.</p>

      <button
        onClick={() => fileInputRef.current?.click()}
        className="h-9 px-4 rounded-md bg-accent text-white text-sm font-medium"
      >
        <Upload size={14} className="inline mr-1" />Upload Audio
      </button>
      <input
        ref={fileInputRef}
        type="file"
        accept="audio/*"
        className="hidden"
        onChange={(e) => {
          const file = e.target.files?.[0];
          if (file) upload(file);
        }}
      />

      <div className="space-y-2">
        {entries.map((entry) => (
          <div key={entry.id} className="flex items-center justify-between p-3 rounded-md border border-border-default bg-surface">
            <div>
              <p className="text-sm font-medium">{entry.name}</p>
              <p className="text-xs text-secondary">
                {entry.uploaded_by} &middot; {entry.is_default ? "Default" : "Custom"}
                {entry.queue_id ? ` · Queue: ${entry.queue_id}` : ""}
              </p>
            </div>
            <button onClick={() => remove(entry.id)} className="p-1 text-destructive hover:text-destructive/80">
              <Trash2 size={14} />
            </button>
          </div>
        ))}
        {entries.length === 0 && <p className="text-sm text-tertiary">No hold music configured.</p>}
      </div>
    </div>
  );
}


function shortDate(value: string) {
  return new Date(value).toLocaleString([], {
    month: "short",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
  });
}

// ── Meeting Templates Panel ───────────────────────────────────────

interface MeetingTemplateData {
  id: string;
  name: string;
  description: string;
  default_lobby: boolean;
  default_mute_on_join: boolean;
  default_allow_reactions: boolean;
  default_recording: boolean;
  max_participants: number | null;
  allowed_roles: string[];
  created_at: string;
  created_by: string;
}

function MeetingTemplatesPanel({ baseUrl, token }: { baseUrl: string; token: string }) {
  const [templates, setTemplates] = useState<MeetingTemplateData[]>([]);
  const [creating, setCreating] = useState(false);
  const [name, setName] = useState("");
  const [description, setDescription] = useState("");
  const [defaultLobby, setDefaultLobby] = useState(false);
  const [defaultMuteOnJoin, setDefaultMuteOnJoin] = useState(false);
  const [defaultAllowReactions, setDefaultAllowReactions] = useState(true);
  const [defaultRecording, setDefaultRecording] = useState(false);
  const [maxParticipants, setMaxParticipants] = useState<string>("");

  const load = useCallback(async () => {
    try {
      const data = await api<MeetingTemplateData[]>(baseUrl, token, "/v1/admin/meeting-templates");
      setTemplates(data);
    } catch { /* ignore */ }
  }, [baseUrl, token]);

  useEffect(() => { load(); }, [load]);

  const create = async () => {
    if (!name.trim()) return;
    try {
      await api(baseUrl, token, "/v1/admin/meeting-templates", {
        method: "POST",
        body: {
          name: name.trim(),
          description: description.trim(),
          default_lobby: defaultLobby,
          default_mute_on_join: defaultMuteOnJoin,
          default_allow_reactions: defaultAllowReactions,
          default_recording: defaultRecording,
          max_participants: maxParticipants ? Number(maxParticipants) : null,
        },
      });
      setCreating(false);
      setName("");
      setDescription("");
      setDefaultLobby(false);
      setDefaultMuteOnJoin(false);
      setDefaultAllowReactions(true);
      setDefaultRecording(false);
      setMaxParticipants("");
      load();
      toast({ type: "success", title: "Template created" });
    } catch { toast({ type: "error", title: "Failed to create template" }); }
  };

  const remove = async (id: string) => {
    try {
      await api(baseUrl, token, `/v1/admin/meeting-templates/${id}`, { method: "DELETE" });
      load();
      toast({ type: "success", title: "Template deleted" });
    } catch { toast({ type: "error", title: "Failed to delete template" }); }
  };

  return (
    <div className="space-y-4">
      <div className="flex items-center justify-between">
        <h2 className="text-sm font-semibold">Meeting Templates</h2>
        <button
          onClick={() => setCreating(!creating)}
          className="flex items-center gap-1 text-xs px-2 py-1 bg-accent text-white rounded hover:bg-accent/90"
        >
          <Plus size={12} />
          {creating ? "Cancel" : "New Template"}
        </button>
      </div>

      {creating && (
        <div className="space-y-2 p-3 border border-border-subtle rounded">
          <input
            className="w-full text-sm rounded border border-border-subtle bg-input px-2 py-1.5"
            placeholder="Template name"
            value={name}
            onChange={(e) => setName(e.target.value)}
          />
          <input
            className="w-full text-sm rounded border border-border-subtle bg-input px-2 py-1.5"
            placeholder="Description"
            value={description}
            onChange={(e) => setDescription(e.target.value)}
          />
          <input
            className="w-full text-sm rounded border border-border-subtle bg-input px-2 py-1.5"
            placeholder="Max participants (optional)"
            type="number"
            value={maxParticipants}
            onChange={(e) => setMaxParticipants(e.target.value)}
          />
          <div className="grid grid-cols-2 gap-2">
            <label className="flex items-center gap-2 text-xs">
              <input type="checkbox" checked={defaultLobby} onChange={(e) => setDefaultLobby(e.target.checked)} />
              Lobby enabled
            </label>
            <label className="flex items-center gap-2 text-xs">
              <input type="checkbox" checked={defaultMuteOnJoin} onChange={(e) => setDefaultMuteOnJoin(e.target.checked)} />
              Mute on join
            </label>
            <label className="flex items-center gap-2 text-xs">
              <input type="checkbox" checked={defaultAllowReactions} onChange={(e) => setDefaultAllowReactions(e.target.checked)} />
              Allow reactions
            </label>
            <label className="flex items-center gap-2 text-xs">
              <input type="checkbox" checked={defaultRecording} onChange={(e) => setDefaultRecording(e.target.checked)} />
              Auto-record
            </label>
          </div>
          <button onClick={create} className="w-full py-1.5 bg-accent text-white rounded text-sm">
            Create Template
          </button>
        </div>
      )}

      {templates.length === 0 ? (
        <p className="text-xs text-secondary text-center py-4">No meeting templates configured</p>
      ) : (
        <div className="space-y-2">
          {templates.map((t) => (
            <div key={t.id} className="p-3 border border-border-subtle rounded space-y-1">
              <div className="flex items-center justify-between">
                <span className="text-sm font-medium">{t.name}</span>
                <button
                  onClick={() => remove(t.id)}
                  className="text-xs text-destructive hover:underline"
                >
                  <Trash2 size={12} className="inline mr-1" />
                  Delete
                </button>
              </div>
              {t.description && <p className="text-xs text-secondary">{t.description}</p>}
              <div className="flex flex-wrap gap-2 text-[10px]">
                {t.default_lobby && <span className="px-1.5 py-0.5 bg-blue-500/10 text-blue-600 rounded">Lobby</span>}
                {t.default_mute_on_join && <span className="px-1.5 py-0.5 bg-amber-500/10 text-amber-600 rounded">Mute on join</span>}
                {t.default_allow_reactions && <span className="px-1.5 py-0.5 bg-green-500/10 text-green-600 rounded">Reactions</span>}
                {t.default_recording && <span className="px-1.5 py-0.5 bg-red-500/10 text-red-600 rounded">Recording</span>}
                {t.max_participants && <span className="px-1.5 py-0.5 bg-purple-500/10 text-purple-600 rounded">Max: {t.max_participants}</span>}
              </div>
              <div className="text-[10px] text-tertiary">Created by {t.created_by} on {shortDate(t.created_at)}</div>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}

// ── API Clients Panel ───────────────────────────────────────

interface ApiClientData {
  id: string;
  name: string;
  client_id: string;
  scopes: string[];
  redirect_uris: string[];
  created_by: string;
  created_at: string;
}

function ApiClientsPanel({ baseUrl, token }: { baseUrl: string; token: string }) {
  const [clients, setClients] = useState<ApiClientData[]>([]);
  const [creating, setCreating] = useState(false);
  const [name, setName] = useState("");
  const [scopes, setScopes] = useState("");
  const [lastSecret, setLastSecret] = useState<string | null>(null);

  const load = useCallback(async () => {
    try {
      const data = await api<ApiClientData[]>(baseUrl, token, "/v1/admin/api-clients");
      setClients(data);
    } catch { /* ignore */ }
  }, [baseUrl, token]);

  useEffect(() => { load(); }, [load]);

  const create = async () => {
    if (!name.trim()) return;
    try {
      const resp = await api<{ client: ApiClientData; client_secret: string }>(baseUrl, token, "/v1/admin/api-clients", {
        method: "POST",
        body: {
          name: name.trim(),
          scopes: scopes.split(",").map(s => s.trim()).filter(Boolean),
        },
      });
      setLastSecret(resp.client_secret);
      setCreating(false);
      setName("");
      setScopes("");
      load();
      toast({ type: "success", title: "API client created" });
    } catch { toast({ type: "error", title: "Failed to create API client" }); }
  };

  const remove = async (id: string) => {
    try {
      await api(baseUrl, token, `/v1/admin/api-clients/${id}`, { method: "DELETE" });
      load();
      toast({ type: "success", title: "API client deleted" });
    } catch { toast({ type: "error", title: "Failed to delete API client" }); }
  };

  return (
    <div className="space-y-4">
      <div className="flex items-center justify-between">
        <h2 className="text-sm font-semibold">API Clients (OAuth)</h2>
        <button
          onClick={() => { setCreating(!creating); setLastSecret(null); }}
          className="flex items-center gap-1 text-xs px-2 py-1 bg-accent text-white rounded hover:bg-accent/90"
        >
          <Plus size={12} />
          {creating ? "Cancel" : "New Client"}
        </button>
      </div>

      {lastSecret && (
        <div className="p-3 bg-green-500/10 border border-green-500/20 rounded">
          <p className="text-xs font-medium text-green-700 mb-1">Client secret (copy now, shown once):</p>
          <code className="text-xs break-all select-all">{lastSecret}</code>
        </div>
      )}

      {creating && (
        <div className="space-y-2 p-3 border border-border-subtle rounded">
          <input
            className="w-full text-sm rounded border border-border-subtle bg-input px-2 py-1.5"
            placeholder="Client name"
            value={name}
            onChange={(e) => setName(e.target.value)}
          />
          <input
            className="w-full text-sm rounded border border-border-subtle bg-input px-2 py-1.5"
            placeholder="Scopes (comma-separated, e.g. read,write)"
            value={scopes}
            onChange={(e) => setScopes(e.target.value)}
          />
          <button onClick={create} className="w-full py-1.5 bg-accent text-white rounded text-sm">
            Create Client
          </button>
        </div>
      )}

      {clients.length === 0 ? (
        <p className="text-xs text-secondary text-center py-4">No API clients configured</p>
      ) : (
        <div className="space-y-2">
          {clients.map((c) => (
            <div key={c.id} className="p-3 border border-border-subtle rounded space-y-1">
              <div className="flex items-center justify-between">
                <span className="text-sm font-medium">{c.name}</span>
                <button onClick={() => remove(c.id)} className="text-xs text-destructive hover:underline">
                  <Trash2 size={12} className="inline mr-1" />Delete
                </button>
              </div>
              <div className="text-xs text-secondary">Client ID: <code>{c.client_id}</code></div>
              {c.scopes.length > 0 && (
                <div className="flex flex-wrap gap-1">
                  {c.scopes.map(s => (
                    <span key={s} className="text-[10px] px-1.5 py-0.5 bg-blue-500/10 text-blue-600 rounded">{s}</span>
                  ))}
                </div>
              )}
              <div className="text-[10px] text-tertiary">Created by {c.created_by} on {shortDate(c.created_at)}</div>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}

// ── Bots Panel ───────────────────────────────────────

interface BotData {
  id: string;
  name: string;
  webhook_url: string;
  events: string[];
  api_token: string;
  enabled: boolean;
  owner_uri: string;
  created_at: string;
}

function BotsPanel({ baseUrl, token }: { baseUrl: string; token: string }) {
  const [bots, setBots] = useState<BotData[]>([]);
  const [creating, setCreating] = useState(false);
  const [name, setName] = useState("");
  const [webhookUrl, setWebhookUrl] = useState("");
  const [events, setEvents] = useState("");

  const load = useCallback(async () => {
    try {
      const data = await api<BotData[]>(baseUrl, token, "/v1/admin/bots");
      setBots(data);
    } catch { /* ignore */ }
  }, [baseUrl, token]);

  useEffect(() => { load(); }, [load]);

  const create = async () => {
    if (!name.trim() || !webhookUrl.trim()) return;
    try {
      await api(baseUrl, token, "/v1/admin/bots", {
        method: "POST",
        body: {
          name: name.trim(),
          webhook_url: webhookUrl.trim(),
          events: events.split(",").map(s => s.trim()).filter(Boolean),
        },
      });
      setCreating(false);
      setName("");
      setWebhookUrl("");
      setEvents("");
      load();
      toast({ type: "success", title: "Bot created" });
    } catch { toast({ type: "error", title: "Failed to create bot" }); }
  };

  const toggle = async (bot: BotData) => {
    try {
      await api(baseUrl, token, `/v1/admin/bots/${bot.id}`, {
        method: "PUT",
        body: { enabled: !bot.enabled },
      });
      load();
    } catch { toast({ type: "error", title: "Failed to update bot" }); }
  };

  const remove = async (id: string) => {
    try {
      await api(baseUrl, token, `/v1/admin/bots/${id}`, { method: "DELETE" });
      load();
      toast({ type: "success", title: "Bot deleted" });
    } catch { toast({ type: "error", title: "Failed to delete bot" }); }
  };

  return (
    <div className="space-y-4">
      <div className="flex items-center justify-between">
        <h2 className="text-sm font-semibold">Bots</h2>
        <button
          onClick={() => setCreating(!creating)}
          className="flex items-center gap-1 text-xs px-2 py-1 bg-accent text-white rounded hover:bg-accent/90"
        >
          <Plus size={12} />
          {creating ? "Cancel" : "New Bot"}
        </button>
      </div>

      {creating && (
        <div className="space-y-2 p-3 border border-border-subtle rounded">
          <input
            className="w-full text-sm rounded border border-border-subtle bg-input px-2 py-1.5"
            placeholder="Bot name"
            value={name}
            onChange={(e) => setName(e.target.value)}
          />
          <input
            className="w-full text-sm rounded border border-border-subtle bg-input px-2 py-1.5"
            placeholder="Webhook URL"
            value={webhookUrl}
            onChange={(e) => setWebhookUrl(e.target.value)}
          />
          <input
            className="w-full text-sm rounded border border-border-subtle bg-input px-2 py-1.5"
            placeholder="Events (comma-separated, e.g. message,call,meeting or *)"
            value={events}
            onChange={(e) => setEvents(e.target.value)}
          />
          <button onClick={create} className="w-full py-1.5 bg-accent text-white rounded text-sm">
            Create Bot
          </button>
        </div>
      )}

      {bots.length === 0 ? (
        <p className="text-xs text-secondary text-center py-4">No bots configured</p>
      ) : (
        <div className="space-y-2">
          {bots.map((b) => (
            <div key={b.id} className="p-3 border border-border-subtle rounded space-y-1">
              <div className="flex items-center justify-between">
                <span className="text-sm font-medium">{b.name}</span>
                <div className="flex items-center gap-2">
                  <button onClick={() => toggle(b)} className={cn("text-xs px-2 py-0.5 rounded", b.enabled ? "bg-green-500/10 text-green-600" : "bg-red-500/10 text-red-600")}>
                    {b.enabled ? "Enabled" : "Disabled"}
                  </button>
                  <button onClick={() => remove(b.id)} className="text-xs text-destructive hover:underline">
                    <Trash2 size={12} />
                  </button>
                </div>
              </div>
              <div className="text-xs text-secondary truncate">Webhook: {b.webhook_url}</div>
              {b.events.length > 0 && (
                <div className="flex flex-wrap gap-1">
                  {b.events.map(e => (
                    <span key={e} className="text-[10px] px-1.5 py-0.5 bg-purple-500/10 text-purple-600 rounded">{e}</span>
                  ))}
                </div>
              )}
              <div className="text-[10px] text-tertiary">Token: <code className="select-all">{b.api_token}</code></div>
              <div className="text-[10px] text-tertiary">Owner: {b.owner_uri} | {shortDate(b.created_at)}</div>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}

// ── Connectors Panel ───────────────────────────────────────

interface ConnectorData {
  id: string;
  name: string;
  connector_type: string;
  webhook_url: string;
  events: string[];
  enabled: boolean;
  created_by: string;
  created_at: string;
}

function ConnectorsPanel({ baseUrl, token }: { baseUrl: string; token: string }) {
  const [connectors, setConnectors] = useState<ConnectorData[]>([]);
  const [creating, setCreating] = useState(false);
  const [name, setName] = useState("");
  const [connectorType, setConnectorType] = useState("webhook");
  const [webhookUrl, setWebhookUrl] = useState("");
  const [events, setEvents] = useState("");

  const load = useCallback(async () => {
    try {
      const data = await api<ConnectorData[]>(baseUrl, token, "/v1/admin/connectors");
      setConnectors(data);
    } catch { /* ignore */ }
  }, [baseUrl, token]);

  useEffect(() => { load(); }, [load]);

  const create = async () => {
    if (!name.trim() || !webhookUrl.trim()) return;
    try {
      await api(baseUrl, token, "/v1/admin/connectors", {
        method: "POST",
        body: {
          name: name.trim(),
          type: connectorType,
          webhook_url: webhookUrl.trim(),
          events: events.split(",").map(s => s.trim()).filter(Boolean),
        },
      });
      setCreating(false);
      setName("");
      setWebhookUrl("");
      setEvents("");
      load();
      toast({ type: "success", title: "Connector created" });
    } catch { toast({ type: "error", title: "Failed to create connector" }); }
  };

  const toggle = async (c: ConnectorData) => {
    try {
      await api(baseUrl, token, `/v1/admin/connectors/${c.id}`, {
        method: "PUT",
        body: { enabled: !c.enabled },
      });
      load();
    } catch { toast({ type: "error", title: "Failed to update connector" }); }
  };

  const remove = async (id: string) => {
    try {
      await api(baseUrl, token, `/v1/admin/connectors/${id}`, { method: "DELETE" });
      load();
      toast({ type: "success", title: "Connector deleted" });
    } catch { toast({ type: "error", title: "Failed to delete connector" }); }
  };

  return (
    <div className="space-y-4">
      <div className="flex items-center justify-between">
        <h2 className="text-sm font-semibold">Outbound Connectors</h2>
        <button
          onClick={() => setCreating(!creating)}
          className="flex items-center gap-1 text-xs px-2 py-1 bg-accent text-white rounded hover:bg-accent/90"
        >
          <Plus size={12} />
          {creating ? "Cancel" : "New Connector"}
        </button>
      </div>

      {creating && (
        <div className="space-y-2 p-3 border border-border-subtle rounded">
          <input
            className="w-full text-sm rounded border border-border-subtle bg-input px-2 py-1.5"
            placeholder="Connector name"
            value={name}
            onChange={(e) => setName(e.target.value)}
          />
          <select
            className="w-full text-sm rounded border border-border-subtle bg-input px-2 py-1.5"
            value={connectorType}
            onChange={(e) => setConnectorType(e.target.value)}
          >
            <option value="webhook">Webhook</option>
            <option value="slack">Slack</option>
            <option value="teams">Teams</option>
            <option value="jira">Jira</option>
            <option value="custom">Custom</option>
          </select>
          <input
            className="w-full text-sm rounded border border-border-subtle bg-input px-2 py-1.5"
            placeholder="Webhook URL"
            value={webhookUrl}
            onChange={(e) => setWebhookUrl(e.target.value)}
          />
          <input
            className="w-full text-sm rounded border border-border-subtle bg-input px-2 py-1.5"
            placeholder="Events (comma-separated, e.g. call.ended,message.sent or *)"
            value={events}
            onChange={(e) => setEvents(e.target.value)}
          />
          <button onClick={create} className="w-full py-1.5 bg-accent text-white rounded text-sm">
            Create Connector
          </button>
        </div>
      )}

      {connectors.length === 0 ? (
        <p className="text-xs text-secondary text-center py-4">No connectors configured</p>
      ) : (
        <div className="space-y-2">
          {connectors.map((c) => (
            <div key={c.id} className="p-3 border border-border-subtle rounded space-y-1">
              <div className="flex items-center justify-between">
                <span className="text-sm font-medium">{c.name}</span>
                <div className="flex items-center gap-2">
                  <span className="text-[10px] px-1.5 py-0.5 bg-gray-500/10 text-gray-600 rounded">{c.connector_type}</span>
                  <button onClick={() => toggle(c)} className={cn("text-xs px-2 py-0.5 rounded", c.enabled ? "bg-green-500/10 text-green-600" : "bg-red-500/10 text-red-600")}>
                    {c.enabled ? "Enabled" : "Disabled"}
                  </button>
                  <button onClick={() => remove(c.id)} className="text-xs text-destructive hover:underline">
                    <Trash2 size={12} />
                  </button>
                </div>
              </div>
              <div className="text-xs text-secondary truncate">URL: {c.webhook_url}</div>
              {c.events.length > 0 && (
                <div className="flex flex-wrap gap-1">
                  {c.events.map(e => (
                    <span key={e} className="text-[10px] px-1.5 py-0.5 bg-amber-500/10 text-amber-600 rounded">{e}</span>
                  ))}
                </div>
              )}
              <div className="text-[10px] text-tertiary">Created by {c.created_by} on {shortDate(c.created_at)}</div>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
