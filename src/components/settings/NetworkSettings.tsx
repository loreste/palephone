import { useState, useEffect } from "react";
import { cn } from "@/lib/cn";
import { Toggle } from "@/components/ui/Toggle";
import { toast } from "@/components/ui/Toast";
import { getConfig, saveSettings } from "@/lib/tauri";

interface NetworkConfig {
  stunServer: string;
  turnServer: string;
  turnUsername: string;
  turnPassword: string;
  enableIce: boolean;
  srtpMode: "disabled" | "optional" | "required";
  sipPort: number;
  rtpPortMin: number;
  rtpPortMax: number;
}

const defaultConfig: NetworkConfig = {
  stunServer: "stun:stun.l.google.com:19302",
  turnServer: "",
  turnUsername: "",
  turnPassword: "",
  enableIce: true,
  srtpMode: "optional",
  sipPort: 5060,
  rtpPortMin: 10000,
  rtpPortMax: 20000,
};

export function NetworkSettings() {
  const [config, setConfig] = useState<NetworkConfig>(defaultConfig);

  // Load persisted network settings
  useEffect(() => {
    getConfig()
      .then((appConfig) => {
        setConfig({
          stunServer: appConfig.network.stun_server || defaultConfig.stunServer,
          turnServer: appConfig.network.turn_server || defaultConfig.turnServer,
          turnUsername: appConfig.network.turn_username || defaultConfig.turnUsername,
          turnPassword: appConfig.network.turn_password || defaultConfig.turnPassword,
          enableIce: appConfig.network.enable_ice,
          srtpMode: appConfig.network.srtp_mode || defaultConfig.srtpMode,
          sipPort: appConfig.network.sip_port || defaultConfig.sipPort,
          rtpPortMin: appConfig.network.rtp_port_min || defaultConfig.rtpPortMin,
          rtpPortMax: appConfig.network.rtp_port_max || defaultConfig.rtpPortMax,
        });
      })
      .catch(() => {});
  }, []);

  const handleSave = async () => {
    try {
      if (config.sipPort < 1 || config.sipPort > 65534) {
        toast({ type: "error", title: "Invalid SIP port" });
        return;
      }
      if (config.rtpPortMin < 1024 || config.rtpPortMax <= config.rtpPortMin || config.rtpPortMax > 65535) {
        toast({ type: "error", title: "Invalid RTP port range" });
        return;
      }
      const appConfig = await getConfig();
      appConfig.network = {
        stun_server: config.stunServer,
        turn_server: config.turnServer,
        turn_username: config.turnUsername,
        turn_password: config.turnPassword,
        enable_ice: config.enableIce,
        srtp_mode: config.srtpMode,
        sip_port: config.sipPort,
        rtp_port_min: config.rtpPortMin,
        rtp_port_max: config.rtpPortMax,
      };
      await saveSettings(appConfig);
      toast({ type: "success", title: "Network settings saved" });
    } catch (err) {
      toast({ type: "error", title: "Failed to save", description: String(err) });
    }
  };

  return (
    <div className="space-y-5">
      <SectionHeader title="NAT Traversal" />

      <ToggleRow
        label="Enable ICE"
        checked={config.enableIce}
        onChange={(v) => setConfig({ ...config, enableIce: v })}
      />

      <FormField
        label="STUN Server"
        value={config.stunServer}
        onChange={(v) => setConfig({ ...config, stunServer: v })}
        placeholder="stun:stun.example.com:3478"
      />

      <SectionHeader title="TURN Relay" />

      <FormField
        label="TURN Server"
        value={config.turnServer}
        onChange={(v) => setConfig({ ...config, turnServer: v })}
        placeholder="turn:turn.example.com:3478"
      />
      <FormField
        label="Username"
        value={config.turnUsername}
        onChange={(v) => setConfig({ ...config, turnUsername: v })}
        placeholder="username"
      />
      <FormField
        label="Password"
        value={config.turnPassword}
        onChange={(v) => setConfig({ ...config, turnPassword: v })}
        placeholder="password"
        type="password"
      />

      <SectionHeader title="Media Security" />

      <div className="space-y-1.5">
        <label className="text-xs font-medium text-secondary">SRTP Mode</label>
        <select
          value={config.srtpMode}
          onChange={(e) =>
            setConfig({
              ...config,
              srtpMode: e.target.value as NetworkConfig["srtpMode"],
            })
          }
          className={cn(
            "w-full bg-surface border border-border-subtle rounded-md",
            "px-3 py-2 text-sm text-primary",
            "focus:outline-none focus:border-border-focus focus:ring-1 focus:ring-accent/30",
            "transition-colors"
          )}
        >
          <option value="optional">Optional</option>
          <option value="required">Required</option>
          <option value="disabled">Disabled</option>
        </select>
      </div>

      <SectionHeader title="Ports" />

      <div className="grid grid-cols-2 gap-3">
        <FormField
          label="SIP Port"
          value={String(config.sipPort)}
          onChange={(v) => setConfig({ ...config, sipPort: parseInt(v) || 5060 })}
          placeholder="5060"
        />
        <div />
        <FormField
          label="RTP Port Min"
          value={String(config.rtpPortMin)}
          onChange={(v) => setConfig({ ...config, rtpPortMin: parseInt(v) || 10000 })}
          placeholder="10000"
        />
        <FormField
          label="RTP Port Max"
          value={String(config.rtpPortMax)}
          onChange={(v) => setConfig({ ...config, rtpPortMax: parseInt(v) || 20000 })}
          placeholder="20000"
        />
      </div>

      <div className="flex gap-2 pt-2">
        <button
          onClick={() => setConfig(defaultConfig)}
          className={cn(
            "flex-1 px-4 py-2 rounded-md text-sm font-medium",
            "bg-elevated text-secondary hover:bg-overlay transition-colors"
          )}
        >
          Reset Defaults
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

function SectionHeader({ title }: { title: string }) {
  return (
    <h3 className="text-xs font-semibold text-tertiary uppercase tracking-wider pt-2">
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

function ToggleRow({
  label,
  checked,
  onChange,
}: {
  label: string;
  checked: boolean;
  onChange: (v: boolean) => void;
}) {
  return (
    <div className="flex items-center justify-between py-1">
      <span className="text-sm text-primary">{label}</span>
      <Toggle checked={checked} onChange={onChange} label={label} />
    </div>
  );
}
