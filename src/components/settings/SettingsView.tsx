import { useState, useEffect, useRef, useCallback } from "react";
import { User, Users, Volume2, Globe, Info, Server, Bell, Phone, LogOut, Sun, Moon, Palette, Shield, Monitor, Smartphone, Laptop, Trash2, Clock, Calendar, Plus, Languages } from "lucide-react";
import { cn } from "@/lib/cn";
import { useAccountStore } from "@/store/accountStore";
import { useServerStore } from "@/store/serverStore";
import { useUiStore } from "@/store/uiStore";
import { t, getLocale, setLocale, LOCALE_LABELS, type Locale } from "@/lib/i18n";
import { AudioSettings } from "./AudioSettings";
import { NetworkSettings } from "./NetworkSettings";
import { registerAccount, storeSipPassword, getConfig, saveSettings, paleServerApi } from "@/lib/tauri";
import { adminLogin, adminLogout, adminBaseUrl, getMfaStatus, setupMfa, verifyMfa, disableMfa, listSessions, revokeSession, revokeAllSessions } from "@/lib/adminApi";
import type { MfaSetupResponse, SessionInfo } from "@/lib/adminApi";
import { disconnectServer, signOut } from "@/lib/session";
import { toast } from "@/components/ui/Toast";
import type { SipAccount } from "@/types";

type SettingsTab = "account" | "audio" | "network" | "server" | "calls" | "call_analytics" | "call_groups" | "delegation" | "notifications" | "security" | "appearance" | "language" | "ooo" | "calendar" | "contacts" | "about";

const settingsTabs: { id: SettingsTab; label: string; icon: typeof User }[] = [
  { id: "account", label: "Account", icon: User },
  { id: "calls", label: "Calls", icon: Phone },
  { id: "call_analytics", label: "Analytics", icon: Phone },
  { id: "call_groups", label: "Groups", icon: Phone },
  { id: "delegation", label: "Delegation", icon: Users },
  { id: "audio", label: "Audio", icon: Volume2 },
  { id: "network", label: "Network", icon: Globe },
  { id: "server", label: "Server", icon: Server },
  { id: "security", label: "Security", icon: Shield },
  { id: "notifications", label: "Notifications", icon: Bell },
  { id: "appearance", label: "Appearance", icon: Palette },
  { id: "language", label: "Language", icon: Languages },
  { id: "ooo", label: "Out of Office", icon: Clock },
  { id: "calendar", label: "Calendar", icon: Calendar },
  { id: "contacts", label: "Contacts", icon: Users },
  { id: "about", label: "About", icon: Info },
];

export function SettingsView() {
  const [activeTab, setActiveTab] = useState<SettingsTab>("account");

  return (
    <div className="flex flex-col h-full">
      {/* Settings header */}
      <div className="px-4 pt-4 pb-2">
        <h1 className="text-lg font-semibold text-primary">{t("settings.title")}</h1>
      </div>

      {/* Tab bar */}
      <div className="flex gap-1 px-4 pb-3 flex-wrap">
        {settingsTabs.map(({ id, label, icon: Icon }) => (
          <button
            key={id}
            onClick={() => setActiveTab(id)}
            className={cn(
              "flex items-center gap-1.5 px-3 py-1.5 rounded-md text-xs font-medium",
              "transition-colors",
              activeTab === id
                ? "bg-accent-muted text-accent"
                : "text-tertiary hover:text-secondary hover:bg-elevated"
            )}
          >
            <Icon size={14} />
            {label}
          </button>
        ))}
      </div>

      {/* Tab content */}
      <div className="flex-1 overflow-y-auto px-4 pb-4">
        {activeTab === "account" && <AccountSettingsPanel />}
        {activeTab === "audio" && <AudioSettings />}
        {activeTab === "calls" && <CallSettingsPanel />}
        {activeTab === "call_analytics" && <CallAnalyticsPanel />}
        {activeTab === "call_groups" && <CallGroupsPanel />}
        {activeTab === "delegation" && <DelegationPanel />}
        {activeTab === "network" && <NetworkSettings />}
        {activeTab === "server" && <ServerSettingsPanel />}
        {activeTab === "security" && <SecuritySettingsPanel />}
        {activeTab === "notifications" && <NotificationSettingsPanel />}
        {activeTab === "appearance" && <AppearancePanel />}
        {activeTab === "language" && <LanguagePanel />}
        {activeTab === "ooo" && <OutOfOfficePanel />}
        {activeTab === "calendar" && <CalendarIntegrationPanel />}
        {activeTab === "contacts" && <ContactSyncPanel />}
        {activeTab === "about" && <AboutPanel />}
      </div>

      {/* Sign Out */}
      <div className="px-4 pb-4">
        <button
          onClick={signOut}
          className={cn(
            "w-full flex items-center justify-center gap-2 px-4 py-2.5 rounded-md text-sm font-medium",
            "bg-destructive/10 text-destructive hover:bg-destructive/20 transition-colors"
          )}
        >
          <LogOut size={16} />
          {t("settings.signOut")}
        </button>
      </div>
    </div>
  );
}

function AccountSettingsPanel() {
  const { account, setAccount } = useAccountStore();
  const [form, setForm] = useState<SipAccount>({
    displayName: account?.displayName ?? "",
    sipUri: account?.sipUri ?? "",
    registrarUri: account?.registrarUri ?? "",
    authUsername: account?.authUsername ?? "",
    transport: account?.transport ?? "tls",
  });
  const [password, setPassword] = useState("");

  const handleSave = async () => {
    setAccount(form);
    useAccountStore.getState().setRegState("registering");
    try {
      // Store password in OS keychain (never on disk)
      if (password) {
        await storeSipPassword(form.sipUri, password).catch(() => {});
      }

      // Persist account config (minus password) to disk
      const currentConfig = await getConfig().catch(() => null);
      if (currentConfig) {
        currentConfig.account = {
          display_name: form.displayName,
          sip_uri: form.sipUri,
          registrar_uri: form.registrarUri,
          auth_username: form.authUsername,
          transport: form.transport,
          reg_expiry: 3600,
        };
        await saveSettings(currentConfig).catch(() => {});
      }

      // Register with SIP server
      await registerAccount({
        display_name: form.displayName,
        sip_uri: form.sipUri,
        registrar_uri: form.registrarUri,
        auth_username: form.authUsername,
        auth_password: password,
        transport: form.transport,
      });
      toast({ type: "info", title: "Registering..." });
    } catch (err) {
      toast({ type: "error", title: "Failed to register", description: String(err) });
      useAccountStore.getState().setRegState("unregistered", String(err));
    }
  };

  return (
    <div className="space-y-4">
      <SectionHeader title="SIP Account" />
      <FormField
        label="Display Name"
        value={form.displayName}
        onChange={(v) => setForm({ ...form, displayName: v })}
        placeholder="John Doe"
      />
      <FormField
        label="SIP URI"
        value={form.sipUri}
        onChange={(v) => setForm({ ...form, sipUri: v })}
        placeholder="user@sip.example.com"
      />
      <FormField
        label="Auth Username"
        value={form.authUsername}
        onChange={(v) => setForm({ ...form, authUsername: v })}
        placeholder="username"
      />
      <FormField
        label="Password"
        value={password}
        onChange={setPassword}
        placeholder="password"
        type="password"
      />
      <FormField
        label="Registrar"
        value={form.registrarUri}
        onChange={(v) => setForm({ ...form, registrarUri: v })}
        placeholder="sip.example.com"
      />

      {/* Transport select */}
      <div className="space-y-1.5">
        <label className="text-xs font-medium text-secondary">Transport</label>
        <select
          value={form.transport}
          onChange={(e) =>
            setForm({
              ...form,
              transport: e.target.value as "udp" | "tcp" | "tls",
            })
          }
          className={cn(
            "w-full bg-surface border border-border-subtle rounded-md",
            "px-3 py-2 text-sm text-primary",
            "focus:outline-none focus:border-border-focus focus:ring-1 focus:ring-accent/30"
          )}
        >
          <option value="tls">TLS</option>
          <option value="tcp">TCP</option>
          <option value="udp">UDP</option>
        </select>
      </div>

      {/* Buttons */}
      <div className="flex gap-2 pt-2">
        <button
          className={cn(
            "flex-1 px-4 py-2 rounded-md text-sm font-medium",
            "bg-elevated text-secondary hover:bg-overlay transition-colors"
          )}
        >
          Cancel
        </button>
        <button
          onClick={handleSave}
          className={cn(
            "flex-1 px-4 py-2 rounded-md text-sm font-medium",
            "bg-accent text-inverse hover:bg-accent-hover transition-colors"
          )}
        >
          Save
        </button>
      </div>
    </div>
  );
}

function ServerSettingsPanel() {
  const { baseUrl, connected, setConnection } = useServerStore();
  const [url, setUrl] = useState(baseUrl ?? adminBaseUrl());
  const [username, setUsername] = useState("admin");
  const [password, setPassword] = useState("");
  const [testing, setTesting] = useState(false);

  const handleConnect = async () => {
    if (!url || !password) return;
    setTesting(true);
    try {
      const session = await adminLogin(url, username, password);
      sessionStorage.setItem("pale.admin.token", session.token);
      setConnection(url, session.token, session.expires_at, "admin", session.principal);
      setPassword("");

      // Persist server URL in app config
      const config = await getConfig().catch(() => null);
      if (config) {
        config.server = {
          url,
          username,
          auto_connect: true,
          role: "admin",
          display_name: session.principal,
        };
        await saveSettings(config).catch(() => {});
      }

      toast({ type: "success", title: "Connected to server" });
    } catch (err) {
      toast({ type: "error", title: err instanceof Error ? err.message : "Connection failed" });
    } finally {
      setTesting(false);
    }
  };

  const handleDisconnect = () => {
    const token = sessionStorage.getItem("pale.admin.token");
    if (token && baseUrl) {
      adminLogout(baseUrl, token).catch(() => {});
    }
    // Clears server connection plus stale presence, server rooms, and files
    disconnectServer();
    toast({ type: "info", title: "Disconnected from server" });
  };

  const handleTest = async () => {
    setTesting(true);
    try {
      const response = await fetch(`${url.replace(/\/+$/, "")}/health`);
      const data = await response.json();
      if (data.ok) {
        toast({ type: "success", title: "Server is reachable" });
      } else {
        toast({ type: "error", title: "Unexpected response" });
      }
    } catch {
      toast({ type: "error", title: "Server unreachable" });
    } finally {
      setTesting(false);
    }
  };

  return (
    <div className="space-y-4">
      <SectionHeader title="Pale Server" />

      <div className="flex items-center gap-2 text-sm">
        <span
          className={cn(
            "w-2 h-2 rounded-full",
            connected ? "bg-success" : "bg-tertiary"
          )}
        />
        <span className="text-secondary">
          {connected ? "Connected" : "Not connected"}
        </span>
      </div>

      <FormField
        label="Server URL"
        value={url}
        onChange={setUrl}
        placeholder="http://127.0.0.1:8080"
      />

      {!connected && (
        <>
          <FormField
            label="Username"
            value={username}
            onChange={setUsername}
            placeholder="admin"
          />
          <FormField
            label="Password"
            value={password}
            onChange={setPassword}
            placeholder="password"
            type="password"
          />
        </>
      )}

      <div className="flex gap-2 pt-2">
        <button
          onClick={handleTest}
          disabled={testing}
          className={cn(
            "flex-1 px-4 py-2 rounded-md text-sm font-medium",
            "bg-elevated text-secondary hover:bg-overlay transition-colors",
            "disabled:opacity-60"
          )}
        >
          {testing ? "Testing..." : "Test Connection"}
        </button>
        {connected ? (
          <button
            onClick={handleDisconnect}
            className={cn(
              "flex-1 px-4 py-2 rounded-md text-sm font-medium",
              "bg-destructive/10 text-destructive hover:bg-destructive/20 transition-colors"
            )}
          >
            Disconnect
          </button>
        ) : (
          <button
            onClick={handleConnect}
            disabled={testing || !password}
            className={cn(
              "flex-1 px-4 py-2 rounded-md text-sm font-medium",
              "bg-accent text-inverse hover:bg-accent-hover transition-colors",
              "disabled:opacity-60"
            )}
          >
            Connect
          </button>
        )}
      </div>

      {connected && <ChangePasswordSection />}
    </div>
  );
}

function ChangePasswordSection() {
  const { baseUrl, token } = useServerStore();
  const [currentPw, setCurrentPw] = useState("");
  const [newPw, setNewPw] = useState("");
  const [confirmPw, setConfirmPw] = useState("");
  const [changing, setChanging] = useState(false);

  const handleChangePassword = async () => {
    if (!baseUrl || !token) return;
    if (newPw !== confirmPw) {
      toast({ type: "error", title: "New passwords do not match" });
      return;
    }
    if (newPw.length < 6) {
      toast({ type: "error", title: "New password must be at least 6 characters" });
      return;
    }
    setChanging(true);
    try {
      const res = await fetch(`${baseUrl.replace(/\/+$/, "")}/v1/auth/password`, {
        method: "PUT",
        headers: {
          "Content-Type": "application/json",
          Authorization: `Bearer ${token}`,
        },
        body: JSON.stringify({
          old_password: currentPw,
          new_password: newPw,
        }),
      });
      if (!res.ok) {
        const data = await res.json().catch(() => null);
        throw new Error(data?.error ?? "Failed to change password");
      }
      toast({ type: "success", title: "Password changed successfully" });
      setCurrentPw("");
      setNewPw("");
      setConfirmPw("");
    } catch (err) {
      toast({ type: "error", title: err instanceof Error ? err.message : "Failed to change password" });
    } finally {
      setChanging(false);
    }
  };

  return (
    <>
      <SectionHeader title="Change Password" />
      <FormField
        label="Current Password"
        value={currentPw}
        onChange={setCurrentPw}
        placeholder="Enter current password"
        type="password"
      />
      <FormField
        label="New Password"
        value={newPw}
        onChange={setNewPw}
        placeholder="Enter new password"
        type="password"
      />
      <FormField
        label="Confirm New Password"
        value={confirmPw}
        onChange={setConfirmPw}
        placeholder="Confirm new password"
        type="password"
      />
      <div className="flex gap-2 pt-2">
        <button
          onClick={handleChangePassword}
          disabled={changing || !currentPw || !newPw || !confirmPw}
          className={cn(
            "flex-1 px-4 py-2 rounded-md text-sm font-medium",
            "bg-accent text-inverse hover:bg-accent-hover transition-colors",
            "disabled:opacity-60"
          )}
        >
          {changing ? "Changing..." : "Change Password"}
        </button>
      </div>
    </>
  );
}

function SecuritySettingsPanel() {
  return (
    <div className="space-y-6">
      <MfaSection />
      <SessionsSection />
    </div>
  );
}

function MfaSection() {
  const { baseUrl, token, connected } = useServerStore();
  const [mfaEnabled, setMfaEnabled] = useState<boolean | null>(null);
  const [setupData, setSetupData] = useState<MfaSetupResponse | null>(null);
  const [verifyCode, setVerifyCode] = useState("");
  const [loading, setLoading] = useState(false);

  useEffect(() => {
    if (!connected || !baseUrl || !token) return;
    getMfaStatus(baseUrl, token)
      .then((status) => setMfaEnabled(status.enabled))
      .catch(() => setMfaEnabled(null));
  }, [connected, baseUrl, token]);

  const handleSetup = async () => {
    if (!baseUrl || !token) return;
    setLoading(true);
    try {
      const data = await setupMfa(baseUrl, token);
      setSetupData(data);
    } catch (err) {
      toast({ type: "error", title: "Failed to set up MFA", description: String(err) });
    } finally {
      setLoading(false);
    }
  };

  const handleVerify = async () => {
    if (!baseUrl || !token || !verifyCode) return;
    setLoading(true);
    try {
      await verifyMfa(baseUrl, token, verifyCode);
      setMfaEnabled(true);
      setSetupData(null);
      setVerifyCode("");
      toast({ type: "success", title: "MFA enabled successfully" });
    } catch (err) {
      toast({ type: "error", title: "Invalid code", description: String(err) });
    } finally {
      setLoading(false);
    }
  };

  const handleDisable = async () => {
    if (!baseUrl || !token) return;
    setLoading(true);
    try {
      await disableMfa(baseUrl, token);
      setMfaEnabled(false);
      toast({ type: "info", title: "MFA disabled" });
    } catch (err) {
      toast({ type: "error", title: "Failed to disable MFA", description: String(err) });
    } finally {
      setLoading(false);
    }
  };

  if (!connected) {
    return (
      <div className="space-y-3">
        <SectionHeader title="Multi-Factor Authentication" />
        <p className="text-sm text-tertiary">Connect to a server to manage MFA.</p>
      </div>
    );
  }

  return (
    <div className="space-y-3">
      <SectionHeader title="Multi-Factor Authentication" />

      {mfaEnabled === null && (
        <p className="text-sm text-tertiary">Loading MFA status...</p>
      )}

      {mfaEnabled === false && !setupData && (
        <div className="space-y-2">
          <p className="text-sm text-secondary">
            Add an extra layer of security to your account with a time-based one-time password (TOTP).
          </p>
          <button
            onClick={handleSetup}
            disabled={loading}
            className={cn(
              "px-4 py-2 rounded-md text-sm font-medium",
              "bg-accent text-inverse hover:bg-accent-hover transition-colors",
              loading && "opacity-50 cursor-not-allowed"
            )}
          >
            {loading ? "Setting up..." : "Enable MFA"}
          </button>
        </div>
      )}

      {setupData && (
        <div className="space-y-3">
          <p className="text-sm text-secondary">
            Scan this QR code with your authenticator app, or enter the secret manually.
          </p>
          <div className="bg-elevated rounded-lg p-4 space-y-2">
            <p className="text-xs text-tertiary font-mono break-all">
              {setupData.provisioning_uri}
            </p>
            <div className="space-y-1">
              <p className="text-xs font-medium text-secondary">Secret key:</p>
              <p className="text-xs font-mono text-primary bg-surface px-2 py-1 rounded break-all">
                {setupData.secret_base32}
              </p>
            </div>
          </div>

          <div className="space-y-2">
            <p className="text-xs font-medium text-secondary">Backup codes (save these):</p>
            <div className="grid grid-cols-2 gap-1">
              {setupData.backup_codes.map((code, i) => (
                <span key={i} className="text-xs font-mono text-primary bg-elevated px-2 py-1 rounded">
                  {code}
                </span>
              ))}
            </div>
          </div>

          <FormField
            label="Enter verification code"
            value={verifyCode}
            onChange={setVerifyCode}
            placeholder="123456"
          />
          <button
            onClick={handleVerify}
            disabled={loading || !verifyCode}
            className={cn(
              "w-full px-4 py-2 rounded-md text-sm font-medium",
              "bg-accent text-inverse hover:bg-accent-hover transition-colors",
              (loading || !verifyCode) && "opacity-50 cursor-not-allowed"
            )}
          >
            {loading ? "Verifying..." : "Verify & Enable"}
          </button>
        </div>
      )}

      {mfaEnabled === true && (
        <div className="space-y-2">
          <div className="flex items-center gap-2">
            <Shield size={16} className="text-success" />
            <span className="text-sm text-primary font-medium">MFA is enabled</span>
          </div>
          <p className="text-sm text-tertiary">
            Your account is protected with TOTP-based multi-factor authentication.
          </p>
          <button
            onClick={handleDisable}
            disabled={loading}
            className={cn(
              "px-4 py-2 rounded-md text-sm font-medium",
              "bg-destructive/10 text-destructive hover:bg-destructive/20 transition-colors",
              loading && "opacity-50 cursor-not-allowed"
            )}
          >
            {loading ? "Disabling..." : "Disable MFA"}
          </button>
        </div>
      )}
    </div>
  );
}

function SessionsSection() {
  const { baseUrl, token, connected } = useServerStore();
  const [sessions, setSessions] = useState<SessionInfo[]>([]);
  const [loading, setLoading] = useState(false);

  const loadSessions = useCallback(async () => {
    if (!connected || !baseUrl || !token) return;
    try {
      const data = await listSessions(baseUrl, token);
      setSessions(data);
    } catch {
      // silently fail if endpoint not available
    }
  }, [connected, baseUrl, token]);

  useEffect(() => {
    loadSessions();
  }, [loadSessions]);

  const handleRevoke = async (id: string) => {
    if (!baseUrl || !token) return;
    setLoading(true);
    try {
      await revokeSession(baseUrl, token, id);
      setSessions((prev) => prev.filter((s) => s.id !== id));
      toast({ type: "success", title: "Session revoked" });
    } catch (err) {
      toast({ type: "error", title: "Failed to revoke session", description: String(err) });
    } finally {
      setLoading(false);
    }
  };

  const handleRevokeAll = async () => {
    if (!baseUrl || !token) return;
    setLoading(true);
    try {
      const result = await revokeAllSessions(baseUrl, token);
      loadSessions();
      toast({ type: "success", title: `Revoked ${result.revoked} session(s)` });
    } catch (err) {
      toast({ type: "error", title: "Failed to revoke sessions", description: String(err) });
    } finally {
      setLoading(false);
    }
  };

  const deviceIcon = (type: string) => {
    switch (type) {
      case "mobile":
        return <Smartphone size={14} className="text-tertiary" />;
      case "tablet":
        return <Monitor size={14} className="text-tertiary" />;
      default:
        return <Laptop size={14} className="text-tertiary" />;
    }
  };

  if (!connected) {
    return (
      <div className="space-y-3">
        <SectionHeader title="Active Sessions" />
        <p className="text-sm text-tertiary">Connect to a server to manage sessions.</p>
      </div>
    );
  }

  return (
    <div className="space-y-3">
      <div className="flex items-center justify-between">
        <SectionHeader title="Active Sessions" />
        {sessions.length > 1 && (
          <button
            onClick={handleRevokeAll}
            disabled={loading}
            className={cn(
              "text-xs text-destructive hover:text-destructive/80 transition-colors",
              loading && "opacity-50 cursor-not-allowed"
            )}
          >
            Revoke all other sessions
          </button>
        )}
      </div>

      {sessions.length === 0 && (
        <p className="text-sm text-tertiary">No active sessions found.</p>
      )}

      <div className="space-y-2">
        {sessions.map((session) => (
          <div
            key={session.id}
            className={cn(
              "flex items-center justify-between p-3 rounded-lg",
              "bg-elevated border border-border-subtle"
            )}
          >
            <div className="flex items-center gap-3">
              {deviceIcon(session.device_type)}
              <div>
                <p className="text-sm text-primary font-medium">
                  {session.device_name}
                  {session.current && (
                    <span className="ml-2 text-xs text-accent font-normal">(current)</span>
                  )}
                </p>
                <p className="text-xs text-tertiary">
                  {session.ip_address} - Last active{" "}
                  {new Date(session.last_active).toLocaleDateString()}
                </p>
              </div>
            </div>
            {!session.current && (
              <button
                onClick={() => handleRevoke(session.id)}
                disabled={loading}
                className="p-1.5 rounded-md text-tertiary hover:text-destructive hover:bg-destructive/10 transition-colors"
                title="Revoke session"
              >
                <Trash2 size={14} />
              </button>
            )}
          </div>
        ))}
      </div>
    </div>
  );
}

function NotificationSettingsPanel() {
  const [config, setConfig] = useState({
    enabled: true,
    sound_enabled: true,
    dnd_enabled: false,
    dnd_start: "22:00",
    dnd_end: "07:00",
  });

  useEffect(() => {
    getConfig()
      .then((appConfig) => {
        if (appConfig.notifications) {
          setConfig({
            enabled: appConfig.notifications.enabled,
            sound_enabled: appConfig.notifications.sound_enabled,
            dnd_enabled: appConfig.notifications.dnd_enabled,
            dnd_start: appConfig.notifications.dnd_start,
            dnd_end: appConfig.notifications.dnd_end,
          });
        }
      })
      .catch(() => {});
  }, []);

  const handleSave = async () => {
    try {
      const appConfig = await getConfig();
      appConfig.notifications = {
        ...appConfig.notifications,
        enabled: config.enabled,
        sound_enabled: config.sound_enabled,
        dnd_enabled: config.dnd_enabled,
        dnd_start: config.dnd_start,
        dnd_end: config.dnd_end,
      };
      await saveSettings(appConfig);
      toast({ type: "success", title: "Notification settings saved" });
    } catch (err) {
      toast({ type: "error", title: "Failed to save", description: String(err) });
    }
  };

  return (
    <div className="space-y-4">
      <SectionHeader title="Notifications" />

      <div className="flex items-center justify-between py-1">
        <span className="text-sm text-primary">Enable notifications</span>
        <input
          type="checkbox"
          checked={config.enabled}
          onChange={(e) => setConfig({ ...config, enabled: e.target.checked })}
          className="w-4 h-4 accent-accent"
        />
      </div>

      <div className="flex items-center justify-between py-1">
        <span className="text-sm text-primary">Notification sounds</span>
        <input
          type="checkbox"
          checked={config.sound_enabled}
          onChange={(e) => setConfig({ ...config, sound_enabled: e.target.checked })}
          className="w-4 h-4 accent-accent"
        />
      </div>

      <SectionHeader title="Do Not Disturb" />

      <div className="flex items-center justify-between py-1">
        <span className="text-sm text-primary">Enable DND schedule</span>
        <input
          type="checkbox"
          checked={config.dnd_enabled}
          onChange={(e) => setConfig({ ...config, dnd_enabled: e.target.checked })}
          className="w-4 h-4 accent-accent"
        />
      </div>

      {config.dnd_enabled && (
        <div className="grid grid-cols-2 gap-3">
          <FormField
            label="Start time"
            value={config.dnd_start}
            onChange={(v) => setConfig({ ...config, dnd_start: v })}
            placeholder="22:00"
          />
          <FormField
            label="End time"
            value={config.dnd_end}
            onChange={(v) => setConfig({ ...config, dnd_end: v })}
            placeholder="07:00"
          />
        </div>
      )}

      <div className="flex gap-2 pt-2">
        <button
          onClick={handleSave}
          className={cn(
            "flex-1 px-4 py-2 rounded-md text-sm font-medium",
            "bg-accent text-inverse hover:bg-accent-hover transition-colors"
          )}
        >
          Save
        </button>
      </div>
    </div>
  );
}

function CallSettingsPanel() {
  const { baseUrl, token, connected } = useServerStore();
  const [settings, setSettings] = useState<any>(null);
  const [saving, setSaving] = useState(false);
  const greetingInputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    if (!connected || !baseUrl || !token) return;
    fetch(`${baseUrl}/v1/call-settings`, { headers: { Authorization: `Bearer ${token}` } })
      .then((r) => r.ok ? r.json() : null)
      .then((data) => { if (data) setSettings(data); })
      .catch(() => {});
  }, [connected, baseUrl, token]);

  const save = async () => {
    if (!baseUrl || !token || !settings) return;
    setSaving(true);
    try {
      const res = await fetch(`${baseUrl}/v1/call-settings`, {
        method: "PUT",
        headers: { "Content-Type": "application/json", Authorization: `Bearer ${token}` },
        body: JSON.stringify(settings),
      });
      if (!res.ok) throw new Error("Failed");
      toast({ type: "success", title: "Call settings saved" });
    } catch {
      toast({ type: "error", title: "Failed to save" });
    }
    setSaving(false);
  };

  const addFollowMe = () => {
    setSettings({ ...settings, followme_numbers: [...(settings.followme_numbers || []), { number: "", ring_timeout: 15, label: "" }] });
  };

  const updateFollowMe = (idx: number, field: string, value: string | number) => {
    const nums = [...(settings.followme_numbers || [])];
    nums[idx] = { ...nums[idx], [field]: value };
    setSettings({ ...settings, followme_numbers: nums });
  };

  const removeFollowMe = (idx: number) => {
    setSettings({ ...settings, followme_numbers: (settings.followme_numbers || []).filter((_: any, i: number) => i !== idx) });
  };

  const uploadGreeting = async (file: File) => {
    if (!baseUrl || !token) return;
    try {
      const buffer = await file.arrayBuffer();
      const res = await fetch(`${baseUrl}/v1/files`, {
        method: "POST",
        headers: { "Content-Type": file.type || "audio/wav", Authorization: `Bearer ${token}`, "X-Pale-Filename": file.name },
        body: buffer,
      });
      if (!res.ok) throw new Error("Upload failed");
      const record = await res.json();
      setSettings({ ...settings, voicemail_greeting_file_id: record.id });
      toast({ type: "success", title: "Greeting uploaded" });
    } catch {
      toast({ type: "error", title: "Upload failed" });
    }
  };

  if (!connected) return <p className="text-sm text-tertiary py-8 text-center">Connect to server to manage call settings</p>;
  if (!settings) return <p className="text-sm text-tertiary py-8 text-center">Loading...</p>;

  return (
    <div className="space-y-5">
      <SectionHeader title="Calling Policy" />

      <div className="grid grid-cols-1 sm:grid-cols-2 gap-3">
        {[
          ["allow_private_calls", "Private calls"],
          ["allow_external_calls", "External calls"],
          ["allow_call_forwarding", "Call forwarding"],
          ["allow_call_recording", "Call recording"],
        ].map(([key, label]) => (
          <label key={key} className="flex items-center justify-between gap-3 rounded-md border border-border-subtle px-3 py-2">
            <span className="text-sm text-primary">{label}</span>
            <input
              type="checkbox"
              checked={settings[key] !== false}
              onChange={(event) => setSettings({ ...settings, [key]: event.target.checked })}
              className="w-4 h-4 accent-accent"
            />
          </label>
        ))}
      </div>

      <SectionHeader title="Voicemail" />

      <div className="flex items-center justify-between py-1">
        <div>
          <span className="text-sm text-primary">Enable voicemail</span>
          <p className="text-xs text-tertiary">Callers can leave a message when you don't answer</p>
        </div>
        <input type="checkbox" checked={settings.voicemail_enabled} onChange={(e) => setSettings({ ...settings, voicemail_enabled: e.target.checked })}
          className="w-4 h-4 accent-accent" />
      </div>

      {settings.voicemail_enabled && (
        <>
          <FormField label="Ring timeout (seconds before voicemail)" value={String(settings.voicemail_timeout)}
            onChange={(v) => setSettings({ ...settings, voicemail_timeout: parseInt(v) || 20 })} placeholder="20" />

          <div className="space-y-1.5">
            <label className="text-xs font-medium text-secondary">Voicemail Greeting</label>
            <div className="flex items-center gap-3">
              <input ref={greetingInputRef} type="file" accept="audio/*,.wav,.mp3" className="hidden"
                onChange={(e) => { const f = e.target.files?.[0]; if (f) uploadGreeting(f); }} />
              <button onClick={() => greetingInputRef.current?.click()} type="button"
                className={cn("px-3 py-2 rounded-md border border-border-default text-sm", "hover:bg-elevated")}>
                Upload Audio
              </button>
              {settings.voicemail_greeting_file_id && (
                <audio controls className="h-8" src={`${baseUrl}/v1/files/${settings.voicemail_greeting_file_id}`} />
              )}
            </div>
            <FormField label="Or use text-to-speech" value={settings.voicemail_greeting_text}
              onChange={(v) => setSettings({ ...settings, voicemail_greeting_text: v })} placeholder="Please leave a message after the tone." />
          </div>
        </>
      )}

      <SectionHeader title="Follow Me" />

      <div className="flex items-center justify-between py-1">
        <div>
          <span className="text-sm text-primary">Enable Follow-Me</span>
          <p className="text-xs text-tertiary">Ring multiple numbers in sequence before going to voicemail</p>
        </div>
        <input type="checkbox" checked={settings.followme_enabled} onChange={(e) => setSettings({ ...settings, followme_enabled: e.target.checked })}
          className="w-4 h-4 accent-accent" />
      </div>

      {settings.followme_enabled && (
        <div className="space-y-2">
          {(settings.followme_numbers || []).map((entry: any, idx: number) => (
            <div key={idx} className="flex items-end gap-2">
              <div className="flex items-center justify-center w-6 h-10 text-xs text-tertiary font-mono">{idx + 1}.</div>
              <FormField label={idx === 0 ? "Number / SIP URI" : ""} value={entry.number}
                onChange={(v) => updateFollowMe(idx, "number", v)} placeholder="sip:mobile@carrier.com" />
              <FormField label={idx === 0 ? "Label" : ""} value={entry.label}
                onChange={(v) => updateFollowMe(idx, "label", v)} placeholder="Mobile" />
              <FormField label={idx === 0 ? "Ring (sec)" : ""} value={String(entry.ring_timeout)}
                onChange={(v) => updateFollowMe(idx, "ring_timeout", parseInt(v) || 15)} placeholder="15" />
              <button onClick={() => removeFollowMe(idx)} className="h-10 px-2 text-tertiary hover:text-destructive text-xs">Remove</button>
            </div>
          ))}
          <button onClick={addFollowMe} className="text-xs text-accent hover:underline">+ Add number</button>

          <div className="space-y-1.5 pt-2">
            <label className="text-xs font-medium text-secondary">If nobody answers</label>
            <select value={settings.followme_final} onChange={(e) => setSettings({ ...settings, followme_final: e.target.value })}
              className={cn("w-full bg-surface border border-border-subtle rounded-md px-3 py-2 text-sm text-primary focus:outline-none focus:border-border-focus")}>
              <option value="voicemail">Go to voicemail</option>
              <option value="hangup">Hang up</option>
            </select>
          </div>
        </div>
      )}

      <SectionHeader title="Call Forwarding" />

      <FormField label="Always forward to (overrides everything)" value={settings.forward_always || ""}
        onChange={(v) => setSettings({ ...settings, forward_always: v || null })} placeholder="Leave empty to disable" />
      <FormField label="Forward when busy" value={settings.forward_busy || ""}
        onChange={(v) => setSettings({ ...settings, forward_busy: v || null })} placeholder="sip:backup@pale.local" />
      <FormField label="Forward when no answer" value={settings.forward_no_answer || ""}
        onChange={(v) => setSettings({ ...settings, forward_no_answer: v || null })} placeholder="sip:receptionist@pale.local" />

      <div className="flex gap-2 pt-3">
        <button onClick={save} disabled={saving}
          className={cn("flex-1 px-4 py-2 rounded-md text-sm font-medium", "bg-accent text-inverse hover:bg-accent-hover transition-colors", "disabled:opacity-60")}>
          {saving ? "Saving..." : "Save Call Settings"}
        </button>
      </div>
    </div>
  );
}

// ─── Call Analytics Panel ───

function CallAnalyticsPanel() {
  const { baseUrl, token, connected } = useServerStore();
  const [analytics, setAnalytics] = useState<any>(null);
  const [loading, setLoading] = useState(false);

  useEffect(() => {
    if (!connected || !baseUrl || !token) return;
    setLoading(true);
    // Get current user's analytics - we need the user id
    // Use the users list to find ourselves
    fetch(`${baseUrl}/v1/users`, { headers: { Authorization: `Bearer ${token}` } })
      .then((r) => r.ok ? r.json() : [])
      .then((users: any[]) => {
        // Find own user by checking who we are via account store
        const account = useAccountStore.getState().account;
        const me = users.find((u) => account?.sipUri && u.sip_uri === `sip:${account.sipUri}`);
        if (me) {
          return fetch(`${baseUrl}/v1/users/${me.id}/call-analytics`, {
            headers: { Authorization: `Bearer ${token}` },
          });
        }
        return null;
      })
      .then((r) => r?.ok ? r.json() : null)
      .then((data) => { if (data) setAnalytics(data); })
      .catch(() => {})
      .finally(() => setLoading(false));
  }, [connected, baseUrl, token]);

  if (!connected) {
    return (
      <div className="space-y-4">
        <SectionHeader title="Call Analytics" />
        <p className="text-sm text-tertiary">Connect to a server to view your call analytics.</p>
      </div>
    );
  }

  if (loading) {
    return (
      <div className="space-y-4">
        <SectionHeader title="Call Analytics" />
        <p className="text-sm text-tertiary">Loading...</p>
      </div>
    );
  }

  return (
    <div className="space-y-4">
      <SectionHeader title="Call Analytics" />
      {analytics ? (
        <div className="grid grid-cols-2 gap-3">
          <StatCard label="Total Calls" value={analytics.total_calls} />
          <StatCard label="Answered" value={analytics.answered_calls} />
          <StatCard label="Avg Duration" value={`${Math.round(analytics.avg_duration_secs)}s`} />
          <StatCard label="Total Duration" value={`${Math.round(analytics.total_duration_secs / 60)}m`} />
          <StatCard label="Avg MOS" value={analytics.avg_mos.toFixed(2)} />
          <StatCard label="Avg Packet Loss" value={`${analytics.avg_packet_loss.toFixed(2)}%`} />
          <StatCard label="Quality Reports" value={analytics.total_quality_reports} />
        </div>
      ) : (
        <p className="text-sm text-tertiary">No analytics data available.</p>
      )}
    </div>
  );
}

function StatCard({ label, value }: { label: string; value: string | number }) {
  return (
    <div className="p-3 rounded-lg border border-border-subtle bg-surface">
      <p className="text-[10px] text-tertiary uppercase tracking-wider">{label}</p>
      <p className="text-lg font-semibold text-primary mt-1">{String(value)}</p>
    </div>
  );
}

// ─── Personal Call Groups Panel ───

interface CallGroup {
  id: string;
  user_id: string;
  name: string;
  numbers: string[];
  ring_duration: number;
  enabled: boolean;
}

function CallGroupsPanel() {
  const { baseUrl, token, connected } = useServerStore();
  const [groups, setGroups] = useState<CallGroup[]>([]);
  const [name, setName] = useState("");
  const [numbers, setNumbers] = useState("");
  const [ringDuration, setRingDuration] = useState("20");

  useEffect(() => {
    if (!connected || !baseUrl || !token) return;
    fetch(`${baseUrl}/v1/call-groups`, { headers: { Authorization: `Bearer ${token}` } })
      .then((r) => r.ok ? r.json() : [])
      .then(setGroups)
      .catch(() => {});
  }, [connected, baseUrl, token]);

  const create = async () => {
    if (!baseUrl || !token || !name) return;
    try {
      const res = await fetch(`${baseUrl}/v1/call-groups`, {
        method: "POST",
        headers: { "Content-Type": "application/json", Authorization: `Bearer ${token}` },
        body: JSON.stringify({
          name,
          numbers: numbers.split(",").map((s) => s.trim()).filter(Boolean),
          ring_duration: parseInt(ringDuration) || 20,
          enabled: true,
        }),
      });
      if (!res.ok) throw new Error("Failed");
      const group = await res.json();
      setGroups([...groups, group]);
      setName("");
      setNumbers("");
      toast({ type: "success", title: "Call group created" });
    } catch {
      toast({ type: "error", title: "Failed to create call group" });
    }
  };

  const remove = async (id: string) => {
    if (!baseUrl || !token) return;
    try {
      await fetch(`${baseUrl}/v1/call-groups/${id}`, {
        method: "DELETE",
        headers: { Authorization: `Bearer ${token}` },
      });
      setGroups(groups.filter((g) => g.id !== id));
      toast({ type: "success", title: "Call group deleted" });
    } catch {
      toast({ type: "error", title: "Failed to delete" });
    }
  };

  if (!connected) {
    return (
      <div className="space-y-4">
        <SectionHeader title="Personal Call Groups" />
        <p className="text-sm text-tertiary">Connect to a server to manage call groups.</p>
      </div>
    );
  }

  return (
    <div className="space-y-4">
      <SectionHeader title="Personal Call Groups" />
      <p className="text-sm text-secondary">Ring multiple devices or numbers for incoming calls.</p>

      <FormField label="Group Name" value={name} onChange={setName} placeholder="My devices" />
      <FormField label="Numbers (comma-separated)" value={numbers} onChange={setNumbers} placeholder="+1234567890, sip:mobile@example.com" />
      <FormField label="Ring Duration (seconds)" value={ringDuration} onChange={setRingDuration} placeholder="20" />

      <button
        onClick={create}
        disabled={!name}
        className={cn(
          "px-4 py-2 rounded-md text-sm font-medium",
          "bg-accent text-inverse hover:bg-accent-hover transition-colors",
          "disabled:opacity-60"
        )}
      >
        Add Group
      </button>

      <div className="space-y-2">
        {groups.map((g) => (
          <div key={g.id} className="flex items-center justify-between p-3 rounded-lg border border-border-subtle bg-surface">
            <div>
              <p className="text-sm font-medium text-primary">{g.name}</p>
              <p className="text-[10px] text-tertiary">
                {g.numbers.join(", ")} &middot; Ring {g.ring_duration}s &middot; {g.enabled ? "Enabled" : "Disabled"}
              </p>
            </div>
            <button
              onClick={() => remove(g.id)}
              className="p-1 text-tertiary hover:text-destructive"
            >
              <Phone size={14} />
            </button>
          </div>
        ))}
        {groups.length === 0 && <p className="text-sm text-tertiary">No call groups configured.</p>}
      </div>
    </div>
  );
}

// ─── Delegation Panel (Boss-Secretary) ───

interface Delegation {
  id: string;
  owner_uri: string;
  delegate_uri: string;
  can_answer: boolean;
  can_make: boolean;
  can_view_history: boolean;
  created_at: string;
}

function DelegationPanel() {
  const { baseUrl, token, connected } = useServerStore();
  const account = useAccountStore((s) => s.account);
  const [delegations, setDelegations] = useState<Delegation[]>([]);
  const [delegateUri, setDelegateUri] = useState("");
  const [canAnswer, setCanAnswer] = useState(true);
  const [canMake, setCanMake] = useState(false);
  const [canViewHistory, setCanViewHistory] = useState(false);

  const ownerUri = account?.sipUri || "";

  useEffect(() => {
    if (!connected || !baseUrl || !token || !ownerUri) return;
    fetch(`${baseUrl}/v1/users/${encodeURIComponent(ownerUri)}/delegates`, {
      headers: { Authorization: `Bearer ${token}` },
    })
      .then((r) => (r.ok ? r.json() : []))
      .then(setDelegations)
      .catch(() => {});
  }, [connected, baseUrl, token, ownerUri]);

  const create = async () => {
    if (!baseUrl || !token || !delegateUri || !ownerUri) return;
    try {
      const res = await fetch(
        `${baseUrl}/v1/users/${encodeURIComponent(ownerUri)}/delegates`,
        {
          method: "POST",
          headers: { "Content-Type": "application/json", Authorization: `Bearer ${token}` },
          body: JSON.stringify({
            delegate_uri: delegateUri,
            can_answer: canAnswer,
            can_make: canMake,
            can_view_history: canViewHistory,
          }),
        }
      );
      if (!res.ok) throw new Error("Failed");
      const d = await res.json();
      setDelegations([...delegations, d]);
      setDelegateUri("");
      toast({ type: "success", title: "Delegate added" });
    } catch {
      toast({ type: "error", title: "Failed to add delegate" });
    }
  };

  const remove = async (id: string) => {
    if (!baseUrl || !token || !ownerUri) return;
    try {
      await fetch(
        `${baseUrl}/v1/users/${encodeURIComponent(ownerUri)}/delegates/${id}`,
        { method: "DELETE", headers: { Authorization: `Bearer ${token}` } }
      );
      setDelegations(delegations.filter((d) => d.id !== id));
      toast({ type: "success", title: "Delegate removed" });
    } catch {
      toast({ type: "error", title: "Failed to remove delegate" });
    }
  };

  if (!connected) {
    return (
      <div className="space-y-4">
        <SectionHeader title="Line Delegation" />
        <p className="text-sm text-tertiary">Connect to a server to manage delegates.</p>
      </div>
    );
  }

  return (
    <div className="space-y-4">
      <SectionHeader title="Line Delegation" />
      <p className="text-sm text-secondary">
        Allow another user to answer, make calls, or view call history on your behalf (boss-secretary).
      </p>

      <FormField
        label="Delegate SIP URI"
        value={delegateUri}
        onChange={setDelegateUri}
        placeholder="sip:assistant@example.com"
      />

      <div className="flex gap-4 text-sm">
        <label className="flex items-center gap-1.5">
          <input type="checkbox" checked={canAnswer} onChange={(e) => setCanAnswer(e.target.checked)} />
          Can answer
        </label>
        <label className="flex items-center gap-1.5">
          <input type="checkbox" checked={canMake} onChange={(e) => setCanMake(e.target.checked)} />
          Can make calls
        </label>
        <label className="flex items-center gap-1.5">
          <input type="checkbox" checked={canViewHistory} onChange={(e) => setCanViewHistory(e.target.checked)} />
          View history
        </label>
      </div>

      <button
        onClick={create}
        disabled={!delegateUri}
        className={cn(
          "px-4 py-2 rounded-md text-sm font-medium",
          "bg-accent text-inverse hover:bg-accent-hover transition-colors",
          "disabled:opacity-60"
        )}
      >
        Add Delegate
      </button>

      <div className="space-y-2">
        {delegations.map((d) => (
          <div
            key={d.id}
            className="flex items-center justify-between p-3 rounded-lg border border-border-subtle bg-surface"
          >
            <div>
              <p className="text-sm font-medium text-primary">{d.delegate_uri}</p>
              <p className="text-[10px] text-tertiary">
                {d.can_answer ? "Answer" : ""}
                {d.can_make ? " · Make calls" : ""}
                {d.can_view_history ? " · View history" : ""}
              </p>
            </div>
            <button
              onClick={() => remove(d.id)}
              className="p-1 text-tertiary hover:text-destructive"
            >
              <Trash2 size={14} />
            </button>
          </div>
        ))}
        {delegations.length === 0 && (
          <p className="text-sm text-tertiary">No delegates configured.</p>
        )}
      </div>
    </div>
  );
}

// ─── Appearance Panel (with chat density) ───

type ChatDensity = "compact" | "comfortable" | "spacious";

function AppearancePanel() {
  const { theme, setTheme } = useUiStore();
  const [chatDensity, setChatDensity] = useState<ChatDensity>(() => {
    return (localStorage.getItem("pale.chatDensity") as ChatDensity) || "comfortable";
  });

  const handleDensityChange = (density: ChatDensity) => {
    setChatDensity(density);
    localStorage.setItem("pale.chatDensity", density);
    // Apply the CSS class to the document for global effect
    document.documentElement.setAttribute("data-chat-density", density);
  };

  // Apply on mount
  useEffect(() => {
    document.documentElement.setAttribute("data-chat-density", chatDensity);
  }, [chatDensity]);

  return (
    <div className="space-y-4">
      <SectionHeader title="Appearance" />
      <p className="text-sm text-secondary">Choose your preferred color theme.</p>
      <div className="flex gap-2">
        <button
          onClick={() => setTheme("dark")}
          className={cn(
            "flex-1 flex items-center justify-center gap-2 px-4 py-3 rounded-lg text-sm font-medium transition-colors border",
            theme === "dark"
              ? "border-accent bg-accent-muted text-accent"
              : "border-border-subtle bg-surface text-secondary hover:bg-elevated"
          )}
        >
          <Moon size={16} />
          Dark
        </button>
        <button
          onClick={() => setTheme("light")}
          className={cn(
            "flex-1 flex items-center justify-center gap-2 px-4 py-3 rounded-lg text-sm font-medium transition-colors border",
            theme === "light"
              ? "border-accent bg-accent-muted text-accent"
              : "border-border-subtle bg-surface text-secondary hover:bg-elevated"
          )}
        >
          <Sun size={16} />
          Light
        </button>
      </div>

      <SectionHeader title="Chat Density" />
      <p className="text-sm text-secondary">Adjust message spacing and size in chat.</p>
      <div className="flex gap-2">
        {(["compact", "comfortable", "spacious"] as const).map((density) => (
          <button
            key={density}
            onClick={() => handleDensityChange(density)}
            className={cn(
              "flex-1 px-3 py-2.5 rounded-lg text-sm font-medium transition-colors border capitalize",
              chatDensity === density
                ? "border-accent bg-accent-muted text-accent"
                : "border-border-subtle bg-surface text-secondary hover:bg-elevated"
            )}
          >
            {density}
          </button>
        ))}
      </div>
      <p className="text-xs text-tertiary">
        {chatDensity === "compact" && "Smaller text and tighter spacing for maximum content."}
        {chatDensity === "comfortable" && "Balanced spacing and text size (default)."}
        {chatDensity === "spacious" && "Larger text and more padding for easy reading."}
      </p>
    </div>
  );
}

function LanguagePanel() {
  const [locale, setCurrentLocale] = useState(getLocale);
  const handleChange = (newLocale: Locale) => {
    setLocale(newLocale);
    setCurrentLocale(newLocale);
  };

  return (
    <div className="space-y-4">
      <SectionHeader title={t("settings.language")} />
      <p className="text-sm text-secondary">Choose your preferred language.</p>
      <div className="flex gap-2">
        {(Object.keys(LOCALE_LABELS) as Locale[]).map((loc) => (
          <button
            key={loc}
            onClick={() => handleChange(loc)}
            className={cn(
              "flex-1 px-4 py-3 rounded-lg text-sm font-medium transition-colors border",
              locale === loc
                ? "border-accent bg-accent-muted text-accent"
                : "border-border-subtle bg-surface text-secondary hover:bg-elevated"
            )}
          >
            {LOCALE_LABELS[loc]}
          </button>
        ))}
      </div>
    </div>
  );
}

function AboutPanel() {
  return (
    <div className="flex flex-col items-center justify-center py-12 gap-3">
      <div className="w-16 h-16 rounded-2xl bg-accent/10 flex items-center justify-center">
        <span className="text-2xl font-bold text-accent">P</span>
      </div>
      <h2 className="text-lg font-semibold text-primary">{t("app.name")}</h2>
      <p className="text-xs text-tertiary">{t("app.version")}</p>
      <p className="text-xs text-tertiary text-center px-8">
        {t("app.description")}
      </p>
    </div>
  );
}


function SectionHeader({ title }: { title: string }) {
  return (
    <h3 className="text-xs font-semibold text-tertiary uppercase tracking-wider">
      {title}
    </h3>
  );
}

// ── Out of Office Panel ──────────────────────────────────────────

function OutOfOfficePanel() {
  const baseUrl = useServerStore((s) => s.baseUrl);
  const token = useServerStore((s) => s.token);
  const [message, setMessage] = useState("");
  const [until, setUntil] = useState("");
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);

  const load = useCallback(async () => {
    if (!baseUrl || !token) return;
    try {
      const data = await paleServerApi<{ message: string | null; until: string | null }>(
        baseUrl, token, "/v1/users/out-of-office"
      );
      setMessage(data.message ?? "");
      setUntil(data.until ? data.until.slice(0, 16) : "");
    } catch { /* ignore */ }
    setLoading(false);
  }, [baseUrl, token]);

  useEffect(() => { load(); }, [load]);

  const save = async () => {
    if (!baseUrl || !token) return;
    setSaving(true);
    try {
      await paleServerApi(baseUrl, token, "/v1/users/out-of-office", {
        method: "PUT",
        body: {
          message: message.trim() || null,
          until: until ? new Date(until).toISOString() : null,
        },
      });
      toast({ type: "success", title: "Out of office settings saved" });
    } catch {
      toast({ type: "error", title: "Failed to save out of office settings" });
    }
    setSaving(false);
  };

  const clear = async () => {
    if (!baseUrl || !token) return;
    setSaving(true);
    try {
      await paleServerApi(baseUrl, token, "/v1/users/out-of-office", {
        method: "PUT",
        body: { message: null, until: null },
      });
      setMessage("");
      setUntil("");
      toast({ type: "success", title: "Out of office cleared" });
    } catch {
      toast({ type: "error", title: "Failed to clear out of office" });
    }
    setSaving(false);
  };

  if (loading) return <p className="text-sm text-secondary">Loading...</p>;

  return (
    <div className="space-y-4">
      <h2 className="text-sm font-semibold">Out of Office</h2>
      <p className="text-xs text-secondary">
        When enabled, an auto-reply will be sent to anyone who messages you.
      </p>
      <div className="space-y-3">
        <div>
          <label className="text-xs font-medium text-secondary mb-1 block">Auto-reply message</label>
          <textarea
            className={cn(
              "w-full bg-surface border border-border-subtle rounded-md",
              "px-3 py-2 text-sm text-primary min-h-[80px]",
              "placeholder:text-tertiary",
              "focus:outline-none focus:border-border-focus focus:ring-1 focus:ring-accent/30"
            )}
            placeholder="I'm currently out of the office..."
            value={message}
            onChange={(e) => setMessage(e.target.value)}
          />
        </div>
        <div>
          <label className="text-xs font-medium text-secondary mb-1 block">Return date/time (optional)</label>
          <input
            type="datetime-local"
            className={cn(
              "w-full bg-surface border border-border-subtle rounded-md",
              "px-3 py-2 text-sm text-primary",
              "focus:outline-none focus:border-border-focus focus:ring-1 focus:ring-accent/30"
            )}
            value={until}
            onChange={(e) => setUntil(e.target.value)}
          />
        </div>
        <div className="flex gap-2">
          <button
            onClick={save}
            disabled={saving}
            className={cn(
              "flex-1 px-4 py-2 rounded-md text-sm font-medium",
              "bg-accent text-white hover:bg-accent/90 transition-colors",
              "disabled:opacity-50"
            )}
          >
            {saving ? "Saving..." : "Save"}
          </button>
          <button
            onClick={clear}
            disabled={saving}
            className={cn(
              "px-4 py-2 rounded-md text-sm font-medium",
              "bg-hover text-secondary hover:text-primary transition-colors",
              "disabled:opacity-50"
            )}
          >
            Clear
          </button>
        </div>
        {message && (
          <div className="p-3 bg-amber-500/10 border border-amber-500/20 rounded-md">
            <div className="text-xs font-medium text-amber-600 mb-1">Out of office is active</div>
            <div className="text-xs text-amber-700">{message}</div>
            {until && (
              <div className="text-[10px] text-amber-600 mt-1">
                Until: {new Date(until).toLocaleString()}
              </div>
            )}
          </div>
        )}
      </div>
    </div>
  );
}

function FormField({
  label,
  value,
  onChange,
  placeholder,
  type = "text",
}: {
  label: string;
  value: string;
  onChange: (value: string) => void;
  placeholder: string;
  type?: string;
}) {
  return (
    <div className="space-y-1.5">
      <label className="text-xs font-medium text-secondary">{label}</label>
      <input
        type={type}
        value={value}
        onChange={(e) => onChange(e.target.value)}
        placeholder={placeholder}
        className={cn(
          "w-full bg-surface border border-border-subtle rounded-md",
          "px-3 py-2 text-sm text-primary",
          "placeholder:text-tertiary",
          "focus:outline-none focus:border-border-focus focus:ring-1 focus:ring-accent/30",
          "transition-colors"
        )}
      />
    </div>
  );
}

// ── Calendar Integration Panel ───────────────────────────────

interface CalendarIntegrationData {
  id: string;
  provider: string;
  calendar_id?: string;
  enabled: boolean;
  last_sync?: string;
}

function CalendarIntegrationPanel() {
  const serverBaseUrl = useServerStore((s) => s.baseUrl);
  const serverToken = useServerStore((s) => s.token);
  const [integrations, setIntegrations] = useState<CalendarIntegrationData[]>([]);
  const [creating, setCreating] = useState(false);
  const [provider, setProvider] = useState("google");
  const [accessToken, setAccessToken] = useState("");
  const [calendarId, setCalendarId] = useState("");
  const [loading, setLoading] = useState(true);

  const load = useCallback(async () => {
    if (!serverBaseUrl || !serverToken) return;
    setLoading(true);
    try {
      const data = await paleServerApi<CalendarIntegrationData[]>(serverBaseUrl, serverToken, "/v1/users/calendar-integration");
      setIntegrations(data);
    } catch { /* ignore */ }
    setLoading(false);
  }, [serverBaseUrl, serverToken]);

  useEffect(() => { load(); }, [load]);

  const create = async () => {
    if (!accessToken.trim() || !serverBaseUrl || !serverToken) return;
    try {
      await paleServerApi(serverBaseUrl, serverToken, "/v1/users/calendar-integration", {
        method: "POST",
        body: {
          provider,
          access_token: accessToken.trim(),
          calendar_id: calendarId.trim() || undefined,
        },
      });
      setCreating(false);
      setAccessToken("");
      setCalendarId("");
      load();
      toast({ type: "success", title: "Calendar connected" });
    } catch { toast({ type: "error", title: "Failed to connect calendar" }); }
  };

  const remove = async (id: string) => {
    if (!serverBaseUrl || !serverToken) return;
    try {
      await paleServerApi(serverBaseUrl, serverToken, `/v1/users/calendar-integration/${id}`, { method: "DELETE" });
      load();
      toast({ type: "success", title: "Calendar disconnected" });
    } catch { toast({ type: "error", title: "Failed to disconnect calendar" }); }
  };

  if (loading) return <p className="text-sm text-secondary">Loading...</p>;

  return (
    <div className="space-y-4">
      <h2 className="text-sm font-semibold">Calendar Integration</h2>
      <p className="text-xs text-secondary">
        Connect external calendars to sync meetings and availability.
      </p>

      <button
        onClick={() => setCreating(!creating)}
        className={cn(
          "flex items-center gap-1.5 text-xs px-3 py-1.5 rounded-md",
          "bg-accent text-white hover:bg-accent/90 transition-colors"
        )}
      >
        <Plus size={12} />
        {creating ? "Cancel" : "Add Calendar"}
      </button>

      {creating && (
        <div className="space-y-3 p-3 border border-border-subtle rounded-md">
          <div>
            <label className="text-xs font-medium text-secondary mb-1 block">Provider</label>
            <select
              className={cn("w-full bg-surface border border-border-subtle rounded-md px-3 py-2 text-sm")}
              value={provider}
              onChange={(e) => setProvider(e.target.value)}
            >
              <option value="google">Google Calendar</option>
              <option value="exchange">Microsoft Exchange</option>
              <option value="caldav">CalDAV</option>
            </select>
          </div>
          <div>
            <label className="text-xs font-medium text-secondary mb-1 block">Access Token</label>
            <input
              type="password"
              className={cn("w-full bg-surface border border-border-subtle rounded-md px-3 py-2 text-sm")}
              placeholder="OAuth access token"
              value={accessToken}
              onChange={(e) => setAccessToken(e.target.value)}
            />
          </div>
          <div>
            <label className="text-xs font-medium text-secondary mb-1 block">Calendar ID (optional)</label>
            <input
              className={cn("w-full bg-surface border border-border-subtle rounded-md px-3 py-2 text-sm")}
              placeholder="primary"
              value={calendarId}
              onChange={(e) => setCalendarId(e.target.value)}
            />
          </div>
          <button
            onClick={create}
            className={cn("w-full px-4 py-2 rounded-md text-sm font-medium bg-accent text-white hover:bg-accent/90")}
          >
            Connect Calendar
          </button>
        </div>
      )}

      {integrations.length === 0 ? (
        <p className="text-xs text-tertiary text-center py-3">No calendars connected</p>
      ) : (
        <div className="space-y-2">
          {integrations.map((i) => (
            <div key={i.id} className="flex items-center justify-between p-3 border border-border-subtle rounded-md">
              <div>
                <div className="text-sm font-medium capitalize">{i.provider}</div>
                {i.calendar_id && <div className="text-xs text-secondary">{i.calendar_id}</div>}
                {i.last_sync && <div className="text-[10px] text-tertiary">Last sync: {new Date(i.last_sync).toLocaleString()}</div>}
              </div>
              <button
                onClick={() => remove(i.id)}
                className="text-xs text-destructive hover:underline flex items-center gap-1"
              >
                <Trash2 size={12} />
                Disconnect
              </button>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}

// ── Contact Sync Panel ───────────────────────────────

interface ContactSyncData {
  id: string;
  provider: string;
  enabled: boolean;
  last_sync?: string;
}

function ContactSyncPanel() {
  const serverBaseUrl = useServerStore((s) => s.baseUrl);
  const serverToken = useServerStore((s) => s.token);
  const [configs, setConfigs] = useState<ContactSyncData[]>([]);
  const [creating, setCreating] = useState(false);
  const [provider, setProvider] = useState("google");
  const [accessToken, setAccessToken] = useState("");
  const [loading, setLoading] = useState(true);

  const load = useCallback(async () => {
    if (!serverBaseUrl || !serverToken) return;
    setLoading(true);
    try {
      const data = await paleServerApi<ContactSyncData[]>(serverBaseUrl, serverToken, "/v1/users/contact-sync");
      setConfigs(data);
    } catch { /* ignore */ }
    setLoading(false);
  }, [serverBaseUrl, serverToken]);

  useEffect(() => { load(); }, [load]);

  const create = async () => {
    if (!accessToken.trim() || !serverBaseUrl || !serverToken) return;
    try {
      await paleServerApi(serverBaseUrl, serverToken, "/v1/users/contact-sync", {
        method: "POST",
        body: { provider, access_token: accessToken.trim() },
      });
      setCreating(false);
      setAccessToken("");
      load();
      toast({ type: "success", title: "Contact sync connected" });
    } catch { toast({ type: "error", title: "Failed to connect contact sync" }); }
  };

  const remove = async (id: string) => {
    if (!serverBaseUrl || !serverToken) return;
    try {
      await paleServerApi(serverBaseUrl, serverToken, `/v1/users/contact-sync/${id}`, { method: "DELETE" });
      load();
      toast({ type: "success", title: "Contact sync disconnected" });
    } catch { toast({ type: "error", title: "Failed to disconnect" }); }
  };

  if (loading) return <p className="text-sm text-secondary">Loading...</p>;

  return (
    <div className="space-y-4">
      <h2 className="text-sm font-semibold">Contact Sync</h2>
      <p className="text-xs text-secondary">
        Sync your address book from external providers.
      </p>

      <button
        onClick={() => setCreating(!creating)}
        className={cn(
          "flex items-center gap-1.5 text-xs px-3 py-1.5 rounded-md",
          "bg-accent text-white hover:bg-accent/90 transition-colors"
        )}
      >
        <Plus size={12} />
        {creating ? "Cancel" : "Add Provider"}
      </button>

      {creating && (
        <div className="space-y-3 p-3 border border-border-subtle rounded-md">
          <div>
            <label className="text-xs font-medium text-secondary mb-1 block">Provider</label>
            <select
              className={cn("w-full bg-surface border border-border-subtle rounded-md px-3 py-2 text-sm")}
              value={provider}
              onChange={(e) => setProvider(e.target.value)}
            >
              <option value="google">Google Contacts</option>
              <option value="exchange">Microsoft Exchange</option>
              <option value="carddav">CardDAV</option>
              <option value="ldap">LDAP</option>
            </select>
          </div>
          <div>
            <label className="text-xs font-medium text-secondary mb-1 block">Access Token</label>
            <input
              type="password"
              className={cn("w-full bg-surface border border-border-subtle rounded-md px-3 py-2 text-sm")}
              placeholder="OAuth access token or API key"
              value={accessToken}
              onChange={(e) => setAccessToken(e.target.value)}
            />
          </div>
          <button
            onClick={create}
            className={cn("w-full px-4 py-2 rounded-md text-sm font-medium bg-accent text-white hover:bg-accent/90")}
          >
            Connect Provider
          </button>
        </div>
      )}

      {configs.length === 0 ? (
        <p className="text-xs text-tertiary text-center py-3">No contact sync configured</p>
      ) : (
        <div className="space-y-2">
          {configs.map((c) => (
            <div key={c.id} className="flex items-center justify-between p-3 border border-border-subtle rounded-md">
              <div>
                <div className="text-sm font-medium capitalize">{c.provider}</div>
                {c.last_sync && <div className="text-[10px] text-tertiary">Last sync: {new Date(c.last_sync).toLocaleString()}</div>}
              </div>
              <button
                onClick={() => remove(c.id)}
                className="text-xs text-destructive hover:underline flex items-center gap-1"
              >
                <Trash2 size={12} />
                Disconnect
              </button>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
