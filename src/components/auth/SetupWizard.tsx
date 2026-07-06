import { useState } from "react";
import { Phone, MessageSquare, Lock, ArrowRight, Check, Server } from "lucide-react";
import { Button } from "@/components/ui/Button";
import { registerAccount, storeSipPassword, getConfig, saveSettings, paleLogin } from "@/lib/tauri";
import { useServerStore } from "@/store/serverStore";
import { useAccountStore } from "@/store/accountStore";
import { toast } from "@/components/ui/Toast";

type WizardStep = "welcome" | "login" | "done";

export function SetupWizard({ onComplete }: { onComplete: () => void }) {
  const [step, setStep] = useState<WizardStep>("welcome");
  const [skipped, setSkipped] = useState(false);

  return (
    <div className="flex flex-col items-center justify-center h-full px-6 py-8">
      {step === "welcome" && <WelcomeStep onNext={() => setStep("login")} />}
      {step === "login" && (
        <UnifiedLoginStep
          onNext={() => setStep("done")}
          onSkip={() => {
            setSkipped(true);
            setStep("done");
          }}
        />
      )}
      {step === "done" && <DoneStep skipped={skipped} onComplete={onComplete} />}
    </div>
  );
}

function WelcomeStep({ onNext }: { onNext: () => void }) {
  return (
    <div className="text-center max-w-[300px]">
      <div className="w-16 h-16 rounded-2xl bg-accent/10 flex items-center justify-center mx-auto mb-4">
        <span className="text-3xl font-bold text-accent">P</span>
      </div>
      <h1 className="text-2xl font-bold text-primary mb-2">Welcome to Pale</h1>
      <p className="text-sm text-tertiary mb-8">
        Secure voice, video, chat, and file sharing — all in one app.
      </p>

      <div className="space-y-3 text-left mb-8">
        <FeatureRow icon={Phone} text="Voice & video calls via SIP" />
        <FeatureRow icon={MessageSquare} text="End-to-end encrypted messaging" />
        <FeatureRow icon={Lock} text="Encrypted file sharing" />
      </div>

      <Button className="w-full gap-2" onClick={onNext}>
        Get Started <ArrowRight size={16} />
      </Button>
    </div>
  );
}

function UnifiedLoginStep({ onNext, onSkip }: { onNext: () => void; onSkip?: () => void }) {
  const [serverUrl, setServerUrl] = useState("http://localhost:8080");
  const [sipUri, setSipUri] = useState("");
  const [password, setPassword] = useState("");
  const [loading, setLoading] = useState(false);
  const setServerConnection = useServerStore((s) => s.setConnection);
  const setAccount = useAccountStore((s) => s.setAccount);

  const handleLogin = async () => {
    if (!serverUrl || !sipUri || !password) return;
    setLoading(true);
    try {
      const response = await paleLogin(serverUrl, sipUri, password);

      // Store server connection
      sessionStorage.setItem("pale.admin.token", response.token);
      setServerConnection(serverUrl, response.token, response.expires_at, response.user.role, response.user.display_name);

      // Store credentials in OS keychain for auto-login on restart
      await storeSipPassword("pale-server-login", password).catch(() => {});

      // Persist server config
      const config = await getConfig().catch(() => null);
      if (config) {
        config.server = {
          url: serverUrl,
          username: sipUri,
          auto_connect: true,
          role: response.user.role,
          display_name: response.user.display_name,
        };
        await saveSettings(config).catch(() => {});
      }

      // Auto-register SIP if credentials were provisioned
      if (response.sip_credentials) {
        const creds = response.sip_credentials;
        await storeSipPassword(creds.sip_uri, creds.password).catch(() => {});

        setAccount({
          displayName: response.user.display_name,
          sipUri: creds.sip_uri,
          registrarUri: creds.registrar_uri,
          authUsername: creds.username,
          transport: (creds.transport as "udp" | "tcp" | "tls") || "tls",
        });

        await registerAccount({
          display_name: response.user.display_name,
          sip_uri: creds.sip_uri,
          registrar_uri: creds.registrar_uri,
          auth_username: creds.username,
          auth_password: creds.password,
          transport: (creds.transport as "udp" | "tcp" | "tls") || "tls",
        }).catch(() => {});

        // Persist account config
        if (config) {
          config.account = {
            display_name: response.user.display_name,
            sip_uri: creds.sip_uri,
            registrar_uri: creds.registrar_uri,
            auth_username: creds.username,
            transport: (creds.transport as "udp" | "tcp" | "tls") || "tls",
            reg_expiry: 3600,
          };
          await saveSettings(config).catch(() => {});
        }
      }

      toast({ type: "success", title: `Welcome, ${response.user.display_name}!` });
      onNext();
    } catch (err) {
      toast({ type: "error", title: "Login failed", description: String(err) });
    } finally {
      setLoading(false);
    }
  };

  return (
    <div className="w-full max-w-[300px]">
      <StepHeader
        icon={Server}
        title="Sign In"
        description="Connect to your organization's Pale server"
        step={1}
      />

      <div className="space-y-3 mb-6">
        <Input
          label="Server URL"
          value={serverUrl}
          onChange={setServerUrl}
          placeholder="http://localhost:8080"
          hint="Server default is port 8080; the docker-compose setup maps it to 8090 externally."
        />
        <Input label="SIP URI" value={sipUri} onChange={setSipUri} placeholder="sip:you@company.com" />
        <Input label="Password" value={password} onChange={setPassword} placeholder="password" type="password" />
      </div>

      <Button className="w-full gap-1" onClick={handleLogin} disabled={loading || !sipUri || !password}>
        {loading ? "Signing in..." : "Sign In"} {!loading && <ArrowRight size={14} />}
      </Button>

      {onSkip && (
        <Button variant="ghost" className="w-full mt-2" onClick={onSkip} disabled={loading}>
          Skip for now — use local SIP only
        </Button>
      )}
      <p className="text-[10px] text-tertiary text-center mt-2">
        You can connect to a Pale server later in Settings &gt; Server.
      </p>
    </div>
  );
}

function DoneStep({ skipped, onComplete }: { skipped: boolean; onComplete: () => void }) {
  return (
    <div className="text-center max-w-[300px]">
      <div className="w-16 h-16 rounded-full bg-success/10 flex items-center justify-center mx-auto mb-4">
        <Check size={32} className="text-success" />
      </div>
      <h2 className="text-xl font-bold text-primary mb-2">You're all set!</h2>
      <p className="text-sm text-tertiary mb-6">
        {skipped
          ? "Pale is running in local SIP-only mode. Add your SIP account in Settings > Account, and connect to a Pale server anytime in Settings > Server."
          : "Pale is ready. You can update your settings anytime."}
      </p>
      <Button className="w-full" onClick={onComplete}>
        Start Using Pale
      </Button>
    </div>
  );
}

function StepHeader({
  icon: Icon,
  title,
  description,
  step,
}: {
  icon: typeof Phone;
  title: string;
  description: string;
  step: number;
}) {
  return (
    <div className="mb-5">
      <div className="flex items-center gap-2 mb-2">
        <span className="w-5 h-5 rounded-full bg-accent text-white text-[10px] font-bold flex items-center justify-center">
          {step}
        </span>
        <Icon size={16} className="text-accent" />
        <h2 className="text-base font-semibold text-primary">{title}</h2>
      </div>
      <p className="text-xs text-tertiary">{description}</p>
    </div>
  );
}

function FeatureRow({ icon: Icon, text }: { icon: typeof Phone; text: string }) {
  return (
    <div className="flex items-center gap-3">
      <div className="w-8 h-8 rounded-lg bg-accent/10 flex items-center justify-center shrink-0">
        <Icon size={16} className="text-accent" />
      </div>
      <span className="text-sm text-primary">{text}</span>
    </div>
  );
}

function Input({
  label,
  value,
  onChange,
  placeholder,
  type = "text",
  hint,
}: {
  label: string;
  value: string;
  onChange: (v: string) => void;
  placeholder: string;
  type?: string;
  hint?: string;
}) {
  return (
    <div className="space-y-1.5">
      <label className="text-xs font-medium text-secondary">{label}</label>
      <input
        type={type}
        value={value}
        onChange={(e) => onChange(e.target.value)}
        placeholder={placeholder}
        className="w-full bg-surface border border-border-subtle rounded-md px-3 py-2 text-sm text-primary placeholder:text-tertiary focus:outline-none focus:border-border-focus focus:ring-1 focus:ring-accent/30"
      />
      {hint && <p className="text-[10px] text-tertiary">{hint}</p>}
    </div>
  );
}
