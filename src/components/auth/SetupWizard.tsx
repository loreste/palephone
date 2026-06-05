import { useState } from "react";
import { Phone, MessageSquare, Lock, ArrowRight, Check } from "lucide-react";
import { Button } from "@/components/ui/Button";
import { registerAccount, matrixLogin, storeSipPassword, getConfig, saveSettings } from "@/lib/tauri";
import { toast } from "@/components/ui/Toast";

type WizardStep = "welcome" | "sip" | "matrix" | "done";

export function SetupWizard({ onComplete }: { onComplete: () => void }) {
  const [step, setStep] = useState<WizardStep>("welcome");

  return (
    <div className="flex flex-col items-center justify-center h-full px-6 py-8">
      {step === "welcome" && <WelcomeStep onNext={() => setStep("sip")} />}
      {step === "sip" && (
        <SipSetupStep
          onNext={() => setStep("matrix")}
          onSkip={() => setStep("matrix")}
        />
      )}
      {step === "matrix" && (
        <MatrixSetupStep
          onNext={() => setStep("done")}
          onSkip={() => setStep("done")}
        />
      )}
      {step === "done" && <DoneStep onComplete={onComplete} />}
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

function SipSetupStep({ onNext, onSkip }: { onNext: () => void; onSkip: () => void }) {
  const [sipUri, setSipUri] = useState("");
  const [registrar, setRegistrar] = useState("");
  const [username, setUsername] = useState("");
  const [password, setPassword] = useState("");
  const [transport, setTransport] = useState<"udp" | "tcp" | "tls">("tls");
  const [loading, setLoading] = useState(false);

  const handleSave = async () => {
    setLoading(true);
    try {
      if (password) await storeSipPassword(sipUri, password).catch(() => {});
      await registerAccount({
        display_name: username,
        sip_uri: sipUri,
        registrar_uri: registrar,
        auth_username: username,
        auth_password: password,
        transport,
      });
      toast({ type: "success", title: "SIP account configured" });
      onNext();
    } catch (err) {
      toast({ type: "error", title: "SIP setup failed", description: String(err) });
    } finally {
      setLoading(false);
    }
  };

  return (
    <div className="w-full max-w-[300px]">
      <StepHeader
        icon={Phone}
        title="SIP Account"
        description="Configure your SIP account for voice and video calls"
        step={1}
      />

      <div className="space-y-3 mb-6">
        <Input label="SIP URI" value={sipUri} onChange={setSipUri} placeholder="user@sip.company.com" />
        <Input label="Registrar" value={registrar} onChange={setRegistrar} placeholder="sip.company.com" />
        <Input label="Username" value={username} onChange={setUsername} placeholder="username" />
        <Input label="Password" value={password} onChange={setPassword} placeholder="password" type="password" />
        <div className="space-y-1.5">
          <label className="text-xs font-medium text-secondary">Transport</label>
          <select
            value={transport}
            onChange={(e) => setTransport(e.target.value as any)}
            className="w-full bg-surface border border-border-subtle rounded-md px-3 py-2 text-sm text-primary focus:outline-none focus:border-border-focus"
          >
            <option value="tls">TLS (Recommended)</option>
            <option value="tcp">TCP</option>
            <option value="udp">UDP</option>
          </select>
        </div>
      </div>

      <div className="flex gap-2">
        <Button variant="ghost" className="flex-1" onClick={onSkip}>Skip</Button>
        <Button className="flex-1 gap-1" onClick={handleSave} disabled={loading || !sipUri}>
          {loading ? "Saving..." : "Next"} <ArrowRight size={14} />
        </Button>
      </div>
    </div>
  );
}

function MatrixSetupStep({ onNext, onSkip }: { onNext: () => void; onSkip: () => void }) {
  const [homeserver, setHomeserver] = useState("");
  const [username, setUsername] = useState("");
  const [password, setPassword] = useState("");
  const [loading, setLoading] = useState(false);

  const handleLogin = async () => {
    setLoading(true);
    try {
      await matrixLogin(homeserver, username, password);
      const config = await getConfig().catch(() => null);
      if (config) {
        config.matrix = { homeserver, username, user_id: null };
        await saveSettings(config).catch(() => {});
      }
      toast({ type: "success", title: "Matrix connected" });
      onNext();
    } catch (err) {
      toast({ type: "error", title: "Matrix login failed", description: String(err) });
    } finally {
      setLoading(false);
    }
  };

  return (
    <div className="w-full max-w-[300px]">
      <StepHeader
        icon={MessageSquare}
        title="Chat & Files"
        description="Connect to your Matrix homeserver for encrypted messaging"
        step={2}
      />

      <div className="space-y-3 mb-6">
        <Input label="Homeserver" value={homeserver} onChange={setHomeserver} placeholder="chat.yourcompany.com" />
        <Input label="Username" value={username} onChange={setUsername} placeholder="username" />
        <Input label="Password" value={password} onChange={setPassword} placeholder="password" type="password" />
      </div>

      <div className="flex gap-2">
        <Button variant="ghost" className="flex-1" onClick={onSkip}>Skip</Button>
        <Button className="flex-1 gap-1" onClick={handleLogin} disabled={loading || !homeserver || !username}>
          {loading ? "Connecting..." : "Next"} <ArrowRight size={14} />
        </Button>
      </div>
    </div>
  );
}

function DoneStep({ onComplete }: { onComplete: () => void }) {
  return (
    <div className="text-center max-w-[300px]">
      <div className="w-16 h-16 rounded-full bg-success/10 flex items-center justify-center mx-auto mb-4">
        <Check size={32} className="text-success" />
      </div>
      <h2 className="text-xl font-bold text-primary mb-2">You're all set!</h2>
      <p className="text-sm text-tertiary mb-6">
        Pale is ready. You can update your settings anytime.
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
}: {
  label: string;
  value: string;
  onChange: (v: string) => void;
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
        className="w-full bg-surface border border-border-subtle rounded-md px-3 py-2 text-sm text-primary placeholder:text-tertiary focus:outline-none focus:border-border-focus focus:ring-1 focus:ring-accent/30"
      />
    </div>
  );
}
