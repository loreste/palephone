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

type AdminTab = "overview" | "users" | "sip" | "routing" | "media" | "calls" | "conferences" | "files" | "audit";

const adminTabs: { id: AdminTab; label: string; icon: LucideIcon }[] = [
  { id: "overview", label: "Overview", icon: Activity },
  { id: "users", label: "Users", icon: Users },
  { id: "sip", label: "SIP", icon: Server },
  { id: "routing", label: "Routing", icon: GitBranch },
  { id: "media", label: "Media", icon: RadioTower },
  { id: "calls", label: "Calls", icon: Router },
  { id: "conferences", label: "Conferences", icon: Mic },
  { id: "files", label: "Files", icon: FileText },
  { id: "audit", label: "Audit", icon: ClipboardList },
];

export function AdminView() {
  const serverBaseUrl = useServerStore((s) => s.baseUrl);
  const [baseUrl] = useState(serverBaseUrl || adminBaseUrl());
  const [token, setToken] = useState(() => sessionStorage.getItem("pale.admin.token") || "");
  const [username, setUsername] = useState("admin");
  const [password, setPassword] = useState("");
  const [activeTab, setActiveTab] = useState<AdminTab>("overview");
  const [snapshot, setSnapshot] = useState<AdminSnapshot | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const setServerConnection = useServerStore((s) => s.setConnection);
  const disconnectServer = useServerStore((s) => s.disconnect);

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
        {activeTab === "media" && <MediaPanel snapshot={snapshot} />}
        {activeTab === "calls" && <CallsPanel snapshot={snapshot} />}
        {activeTab === "conferences" && <ConferencesPanel baseUrl={baseUrl} token={token} snapshot={snapshot} onChange={refresh} />}
        {activeTab === "files" && <FilesPanel baseUrl={baseUrl} token={token} snapshot={snapshot} onChange={refresh} />}
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
      const res = await fetch(`${baseUrl.replace(/\/+$/, "")}/v1/users/${user.id}/role`, {
        method: "PUT",
        headers: { "Content-Type": "application/json", Authorization: `Bearer ${token}` },
        body: JSON.stringify({ role: newRole }),
      });
      if (!res.ok) throw new Error("Failed");
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
  return (
    <div className="space-y-3">
      <Table
        title="Calls"
        columns={["Caller", "Callees", "Media", "Status", "Updated"]}
        rows={(snapshot?.calls ?? []).map((call) => [
          call.caller,
          call.callees.join(", "),
          call.media.join(", "),
          call.status,
          shortDate(call.updated_at),
        ])}
      />
      <Table
        title="Dialogs"
        columns={["Call-ID", "From", "To", "Status"]}
        rows={(snapshot?.dialogs ?? []).map((dialog) => [
          dialog.call_id,
          dialog.from_uri,
          dialog.to_uri,
          dialog.status,
        ])}
      />
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
