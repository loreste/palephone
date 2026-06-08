import { FormEvent, useEffect, useMemo, useState } from "react";
import {
  Activity,
  ClipboardList,
  FileText,
  GitBranch,
  Lock,
  Mic,
  Plus,
  RadioTower,
  RefreshCw,
  Router,
  Server,
  Trash2,
  UserPlus,
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
  setAdminSipAccountEnabled,
  updateRoutingRule,
  type AdminSnapshot,
} from "@/lib/adminApi";
import { toast } from "@/components/ui/Toast";
import { useServerStore } from "@/store/serverStore";
import { paleServerApi } from "@/lib/tauri";

// Helper: all server calls go through Tauri invoke (not webview fetch)
async function api<T = any>(baseUrl: string, token: string, path: string, opts?: { method?: string; body?: unknown }): Promise<T> {
  return paleServerApi<T>(baseUrl, token, path, opts);
}

type AdminTab = "overview" | "users" | "sip" | "routing" | "ring_groups" | "ivr" | "queues" | "extensions" | "hours" | "holidays" | "paging" | "media" | "calls" | "cdrs" | "agents" | "wallboard" | "qa" | "conferences" | "files" | "directory" | "audit";

const adminTabs: { id: AdminTab; label: string; icon: LucideIcon }[] = [
  { id: "overview", label: "Overview", icon: Activity },
  { id: "users", label: "Users", icon: Users },
  { id: "sip", label: "SIP", icon: Server },
  { id: "extensions", label: "Extensions", icon: Server },
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
  { id: "conferences", label: "Conferences", icon: Mic },
  { id: "files", label: "Files", icon: FileText },
  { id: "directory", label: "Directory", icon: Users },
  { id: "audit", label: "Audit", icon: ClipboardList },
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
  const disconnectServer = useServerStore((s) => s.disconnect);

  // Sync token from serverStore if it changes (e.g. after wizard login)
  useEffect(() => {
    if (serverToken && serverToken !== token) {
      setToken(serverToken);
      sessionStorage.setItem("pale.admin.token", serverToken);
    }
  }, [serverToken]);

  const authenticated = token.length > 0;

  const refresh = async () => {
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
  };

  useEffect(() => {
    if (authenticated) {
      refresh();
      setServerConnection(baseUrl, token);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [authenticated]);

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
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [authenticated, token]);

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
                sessionStorage.removeItem("pale.admin.token");
                setToken("");
                setSnapshot(null);
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
        {activeTab === "extensions" && <CrudPanel baseUrl={baseUrl} token={token} endpoint="extensions" title="Extensions" icon={Server} columns={["Extension", "Destination", "Type", "Label"]} rowFn={(e: any) => [e.extension, e.destination, e.destination_type, e.label || "-"]} fields={[["Extension", "extension"], ["Destination (SIP URI)", "destination"], ["Type", "destination_type", "user"], ["Label", "label"]]} />}
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
        {activeTab === "calls" && <CallsPanel snapshot={snapshot} />}
        {activeTab === "conferences" && <ConferencesPanel baseUrl={baseUrl} token={token} snapshot={snapshot} onChange={refresh} />}
        {activeTab === "files" && <FilesPanel baseUrl={baseUrl} token={token} snapshot={snapshot} onChange={refresh} />}
        {activeTab === "directory" && <DirectoryPanel baseUrl={baseUrl} token={token} />}
        {activeTab === "audit" && <AuditPanel snapshot={snapshot} />}
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

  const submit = async (event: FormEvent) => {
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
      toast({ type: "success", title: "User deleted" });
      onChange();
    } catch (err) {
      toast({ type: "error", title: err instanceof Error ? err.message : "Failed to delete user" });
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

  return (
    <section className="border border-border-subtle bg-surface rounded-md overflow-hidden">
      <div className="p-3 border-b border-border-subtle flex items-center gap-2">
        <UserPlus size={17} className="text-accent" />
        <h2 className="font-medium">Users</h2>
      </div>
      <form onSubmit={submit} className="p-3 grid md:grid-cols-5 gap-2 border-b border-border-subtle">
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
      <div className="p-3 overflow-x-auto">
        <table className="w-full text-sm">
          <thead className="text-tertiary">
            <tr className="border-b border-border-subtle">
              {["Name", "SIP URI", "Role", ""].map((header) => (
                <th key={header} className="text-left py-2 px-2 font-medium">{header}</th>
              ))}
            </tr>
          </thead>
          <tbody>
            {(snapshot?.users ?? []).map((user) => (
              <tr key={user.id} className="border-b border-border-subtle">
                <td className="py-2 px-2">{user.display_name}</td>
                <td className="py-2 px-2 text-secondary">{user.sip_uri}</td>
                <td className="py-2 px-2">
                  <span className={cn(
                    "px-2 py-0.5 rounded-full text-xs font-medium",
                    (user as any).role === "admin" ? "bg-accent/20 text-accent" : "bg-elevated text-secondary"
                  )}>
                    {(user as any).role || "user"}
                  </span>
                </td>
                <td className="py-2 px-2 text-right">
                  <div className="inline-flex items-center gap-1">
                    <button
                      onClick={() => toggleRole(user, (user as any).role || "user")}
                      className="h-8 px-2 rounded-md hover:bg-elevated text-xs text-secondary hover:text-primary"
                    >
                      {(user as any).role === "admin" ? "Demote" : "Promote"}
                    </button>
                    <IconButton label="Delete user" tone="danger" onClick={() => remove(user.id)}>
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
  const [priority, setPriority] = useState("100");

  const submit = async (event: FormEvent) => {
    event.preventDefault();
    try {
      await createRoutingRule(baseUrl, token, {
        name,
        source_pattern: sourcePattern,
        destination_pattern: destinationPattern,
        target,
        priority: Number(priority),
        enabled: true,
      });
      setName("");
      setTarget("");
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
        priority: rule.priority,
        enabled: !rule.enabled,
      });
      toast({ type: "success", title: !rule.enabled ? "Routing rule enabled" : "Routing rule disabled" });
      onChange();
    } catch (err) {
      toast({ type: "error", title: err instanceof Error ? err.message : "Failed to update routing rule" });
    }
  };

  return (
    <PanelWithForm
      title="Routing rules"
      icon={GitBranch}
      onSubmit={submit}
      fields={[
        ["Name", name, setName],
        ["Source pattern", sourcePattern, setSourcePattern],
        ["Destination pattern", destinationPattern, setDestinationPattern],
        ["Target", target, setTarget],
        ["Priority", priority, setPriority, "number"],
      ]}
      action="Add route"
    >
      <div className="overflow-x-auto">
        <table className="w-full text-sm">
          <thead className="text-tertiary">
            <tr className="border-b border-border-subtle">
              {["Priority", "Name", "Source", "Destination", "Target", "Status", ""].map((header) => (
                <th key={header} className="text-left py-2 px-2 font-medium">{header}</th>
              ))}
            </tr>
          </thead>
          <tbody>
            {(snapshot?.routingRules ?? []).map((rule) => (
              <tr key={rule.id} className="border-b border-border-subtle">
                <td className="py-2 px-2">{rule.priority}</td>
                <td className="py-2 px-2">{rule.name}</td>
                <td className="py-2 px-2 text-secondary">{rule.source_pattern}</td>
                <td className="py-2 px-2 text-secondary">{rule.destination_pattern}</td>
                <td className="py-2 px-2">{rule.target}</td>
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
    </PanelWithForm>
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

function AuditPanel({ snapshot }: { snapshot: AdminSnapshot | null }) {
  return (
    <Table
      title="Audit log"
      columns={["Time", "Principal", "Action", "Target"]}
      rows={(snapshot?.auditEvents ?? []).slice(0, 100).map((event) => [
        shortDate(event.created_at),
        event.principal,
        event.action,
        event.target ?? "-",
      ])}
    />
  );
}

function RingGroupsPanel({ baseUrl, token }: { baseUrl: string; token: string }) {
  const [groups, setGroups] = useState<any[]>([]);
  const [name, setName] = useState("");
  const [extension, setExtension] = useState("");
  const [strategy, setStrategy] = useState("simultaneous");
  const [members, setMembers] = useState("");
  const [fallback, setFallback] = useState("");

  const load = async () => {
    try {
      const data = await api<any[]>(baseUrl, token, "/v1/ring-groups");
      setGroups(data);
    } catch { /* ignore */ }
  };

  useEffect(() => { load(); }, [baseUrl, token]);

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
      // File uploads need special handling — use paleServerUploadFile from tauri.ts
      const { paleServerUploadFile } = await import("@/lib/tauri");
      const record = await paleServerUploadFile(baseUrl, token, file);
      setGreetingFileId(record.id);
      toast({ type: "success", title: `Uploaded: ${file.name}` });
    } catch (err) {
      toast({ type: "error", title: err instanceof Error ? err.message : "Upload failed" });
    }
    setUploading(false);
  };

  const load = async () => {
    try {
      const data = await api<any[]>(baseUrl, token, "/v1/ivrs");
      setIvrs(data);
    } catch { /* ignore */ }
  };

  useEffect(() => { load(); }, [baseUrl, token]);

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

  const load = async () => {
    try {
      const data = await api<any[]>(baseUrl, token, `/v1/${endpoint}`);
      setItems(data);
    } catch { /* ignore */ }
  };

  useEffect(() => { load(); }, [baseUrl, token]);

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

function QueuesPanel({ baseUrl, token }: { baseUrl: string; token: string }) {
  const [queues, setQueues] = useState<any[]>([]);
  const [name, setName] = useState("");
  const [extension, setExtension] = useState("");
  const [strategy, setStrategy] = useState("round_robin");
  const [agents, setAgents] = useState("");
  const [overflow, setOverflow] = useState("");

  const load = async () => {
    try {
      const data = await api<any[]>(baseUrl, token, "/v1/queues");
      setQueues(data);
    } catch { /* ignore */ }
  };

  useEffect(() => { load(); }, [baseUrl, token]);

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
        },
      });
      setName(""); setExtension(""); setAgents(""); setOverflow("");
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
      <form onSubmit={submit} className="p-3 grid md:grid-cols-6 gap-2 border-b border-border-subtle">
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

  const load = async () => {
    try {
      const data = await api<any[]>(baseUrl, token, "/v1/agents");
      setAgents(data);
    } catch { /* ignore */ }
  };
  useEffect(() => { load(); }, [baseUrl, token]);

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
      await api(baseUrl, token, `/v1/agents/${encodeURIComponent(uri)}/state`, {
        method: "PUT",
        body: { state },
      });
      load();
    } catch { /* ignore */ }
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
                <tr key={a.user_sip_uri} className="border-b border-border-subtle">
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
    </section>
  );
}

function WallboardPanel({ baseUrl, token }: { baseUrl: string; token: string }) {
  const [data, setData] = useState<any>(null);

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

  if (!data) return <p className="text-sm text-tertiary py-8 text-center">Loading wallboard...</p>;

  return (
    <div className="space-y-4">
      <div className="grid grid-cols-2 md:grid-cols-5 gap-2">
        <Metric label="Agents Available" value={data.agents?.available ?? 0} />
        <Metric label="On Call" value={data.agents?.on_call ?? 0} />
        <Metric label="Wrap Up" value={data.agents?.wrap_up ?? 0} />
        <Metric label="On Break" value={data.agents?.on_break ?? 0} />
        <Metric label="Offline" value={data.agents?.offline ?? 0} />
      </div>

      {(data.queues || []).map((q: any) => (
        <section key={q.queue_id} className="border border-border-subtle bg-surface rounded-md overflow-hidden">
          <div className="p-3 border-b border-border-subtle flex items-center justify-between">
            <div className="flex items-center gap-2">
              <span className={cn("w-2 h-2 rounded-full", q.calls_waiting > 0 ? "bg-warning animate-pulse" : "bg-success")} />
              <h3 className="font-medium">{q.queue_name}</h3>
            </div>
            <span className="text-xs text-tertiary">SLA: {q.sla_percentage.toFixed(0)}%</span>
          </div>
          <div className="grid grid-cols-3 md:grid-cols-6 gap-2 p-3">
            <div className="text-center"><div className="text-xl font-semibold">{q.calls_waiting}</div><div className="text-[10px] text-tertiary">Waiting</div></div>
            <div className="text-center"><div className="text-xl font-semibold">{q.calls_active}</div><div className="text-[10px] text-tertiary">Active</div></div>
            <div className="text-center"><div className="text-xl font-semibold">{q.agents_available}</div><div className="text-[10px] text-tertiary">Available</div></div>
            <div className="text-center"><div className="text-xl font-semibold">{q.longest_wait_secs}s</div><div className="text-[10px] text-tertiary">Longest Wait</div></div>
            <div className="text-center"><div className="text-xl font-semibold">{q.calls_answered}</div><div className="text-[10px] text-tertiary">Answered</div></div>
            <div className="text-center"><div className="text-xl font-semibold">{q.calls_abandoned}</div><div className="text-[10px] text-tertiary">Abandoned</div></div>
          </div>
        </section>
      ))}

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
    } catch (err) { toast({ type: "error", title: "Failed" }); }
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
    } catch (err) {
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
        required={label !== "Matrix user ID" && label !== "Display name"}
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
  rows: string[][];
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
                    <td key={cellIndex} className="py-2 px-2 max-w-[260px] truncate">{cell}</td>
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

function shortDate(value: string) {
  return new Date(value).toLocaleString([], {
    month: "short",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
  });
}
