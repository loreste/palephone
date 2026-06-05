import { useState } from "react";
import { User, Volume2, Globe, Info } from "lucide-react";
import { cn } from "@/lib/cn";
import { useAccountStore } from "@/store/accountStore";
import { AudioSettings } from "./AudioSettings";
import { NetworkSettings } from "./NetworkSettings";
import { registerAccount, storeSipPassword, getConfig, saveSettings } from "@/lib/tauri";
import { toast } from "@/components/ui/Toast";
import type { SipAccount } from "@/types";

type SettingsTab = "account" | "audio" | "network" | "about";

const settingsTabs: { id: SettingsTab; label: string; icon: typeof User }[] = [
  { id: "account", label: "Account", icon: User },
  { id: "audio", label: "Audio", icon: Volume2 },
  { id: "network", label: "Network", icon: Globe },
  { id: "about", label: "About", icon: Info },
];

export function SettingsView() {
  const [activeTab, setActiveTab] = useState<SettingsTab>("account");

  return (
    <div className="flex flex-col h-full">
      {/* Settings header */}
      <div className="px-4 pt-4 pb-2">
        <h1 className="text-lg font-semibold text-primary">Settings</h1>
      </div>

      {/* Tab bar */}
      <div className="flex gap-1 px-4 pb-3">
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
        {activeTab === "network" && <NetworkSettings />}
        {activeTab === "about" && <AboutPanel />}
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

function AboutPanel() {
  return (
    <div className="flex flex-col items-center justify-center py-12 gap-3">
      <div className="w-16 h-16 rounded-2xl bg-accent/10 flex items-center justify-center">
        <span className="text-2xl font-bold text-accent">P</span>
      </div>
      <h2 className="text-lg font-semibold text-primary">Pale</h2>
      <p className="text-xs text-tertiary">Version 0.1.0</p>
      <p className="text-xs text-tertiary text-center px-8">
        Cross-platform SIP softphone
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
