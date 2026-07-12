import { useState } from "react";
import { Phone, MessageSquare, Lock, ArrowRight, Check, Server, Shield } from "lucide-react";
import { Button } from "@/components/ui/Button";
import { registerAccount, storeSipPassword, getConfig, saveSettings, type UserLoginResponse } from "@/lib/tauri";
import { normalizeProvisionedSipAccount } from "@/lib/sipDefaults";
import { useServerStore } from "@/store/serverStore";
import { useAccountStore } from "@/store/accountStore";
import { toast } from "@/components/ui/Toast";
import {
  beginMfaEnrollment,
  completeMfaLogin,
  enrollAndValidateMfa,
  loginWithPossibleMfa,
  type MfaLoginPhase,
} from "@/lib/mfaLogin";
import type { MfaSetupResponse } from "@/lib/adminApi";

type WizardStep = "welcome" | "login" | "mfa" | "done";

export function SetupWizard({ onComplete }: { onComplete: () => void }) {
  const [step, setStep] = useState<WizardStep>("welcome");
  const [skipped, setSkipped] = useState(false);
  const [mfaChallenge, setMfaChallenge] = useState<Extract<MfaLoginPhase, { kind: "mfa_pending" }> | null>(
    null,
  );
  const [passwordForSip, setPasswordForSip] = useState("");

  return (
    <div className="flex flex-col items-center justify-center h-full px-6 py-8">
      {step === "welcome" && <WelcomeStep onNext={() => setStep("login")} />}
      {step === "login" && (
        <UnifiedLoginStep
          onComplete={(session, password, serverUrl) => {
            setPasswordForSip(password);
            void finishLogin(session, password, serverUrl).then(() => setStep("done"));
          }}
          onMfa={(challenge, password) => {
            setMfaChallenge(challenge);
            setPasswordForSip(password);
            setStep("mfa");
          }}
          onSkip={() => {
            setSkipped(true);
            setStep("done");
          }}
        />
      )}
      {step === "mfa" && mfaChallenge && (
        <MfaStep
          challenge={mfaChallenge}
          onComplete={(session) => {
            void finishLogin(session, passwordForSip, mfaChallenge.serverUrl).then(() =>
              setStep("done"),
            );
          }}
          onBack={() => setStep("login")}
        />
      )}
      {step === "done" && <DoneStep skipped={skipped} onComplete={onComplete} />}
    </div>
  );
}

async function finishLogin(response: UserLoginResponse, password: string, serverUrl: string) {
  const setServerConnection = useServerStore.getState().setConnection;
  const setAccount = useAccountStore.getState().setAccount;

  sessionStorage.setItem("pale.admin.token", response.token);
  setServerConnection(
    serverUrl,
    response.token,
    response.expires_at,
    response.user.role,
    response.user.display_name,
  );

  await storeSipPassword("pale-server-login", password).catch(() => {});

  const config = await getConfig().catch(() => null);
  if (config) {
    config.server = {
      url: serverUrl || config.server?.url || "",
      username: response.user.sip_uri,
      auto_connect: true,
      role: response.user.role,
      display_name: response.user.display_name,
    };
    await saveSettings(config).catch(() => {});
  }

  if (response.sip_credentials) {
    const creds = response.sip_credentials;
    const sipPassword = creds.password || password;
    await storeSipPassword(creds.sip_uri, sipPassword).catch(() => {});

    if (creds.registrar_uri) {
      const account = normalizeProvisionedSipAccount({
        displayName: response.user.display_name,
        sipUri: creds.sip_uri,
        registrarUri: creds.registrar_uri,
        authUsername: creds.username,
        transport: creds.transport,
      });
      setAccount(account);

      await registerAccount({
        display_name: account.displayName,
        sip_uri: account.sipUri,
        registrar_uri: account.registrarUri,
        auth_username: account.authUsername,
        auth_password: sipPassword,
        transport: account.transport,
      }).catch((e) => {
        console.warn("SIP auto-registration failed:", e);
      });
    }

    if (config) {
      const account = normalizeProvisionedSipAccount({
        displayName: response.user.display_name,
        sipUri: creds.sip_uri,
        registrarUri: creds.registrar_uri ?? "",
        authUsername: creds.username,
        transport: creds.transport,
      });
      config.account = {
        display_name: account.displayName,
        sip_uri: account.sipUri,
        registrar_uri: account.registrarUri,
        auth_username: account.authUsername,
        transport: account.transport,
        reg_expiry: 3600,
      };
      await saveSettings(config).catch(() => {});
    }
  }

  toast({ type: "success", title: `Welcome, ${response.user.display_name}!` });
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

function UnifiedLoginStep({
  onComplete,
  onMfa,
  onSkip,
}: {
  onComplete: (session: UserLoginResponse, password: string, serverUrl: string) => void;
  onMfa: (challenge: Extract<MfaLoginPhase, { kind: "mfa_pending" }>, password: string) => void;
  onSkip?: () => void;
}) {
  const [serverUrl, setServerUrl] = useState("https://drcpbx.com");
  const [sipUri, setSipUri] = useState("");
  const [password, setPassword] = useState("");
  const [loading, setLoading] = useState(false);

  const handleLogin = async () => {
    if (!serverUrl || !sipUri || !password) return;
    setLoading(true);
    try {
      const phase = await loginWithPossibleMfa(serverUrl, sipUri, password);
      if (phase.kind === "mfa_pending") {
        onMfa(phase, password);
        return;
      }
      onComplete(phase.session, password, serverUrl);
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
          placeholder="https://drcpbx.com"
          hint="Use HTTPS for Pale Server. Voice and video registration uses SIP over TLS on port 5061."
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

function MfaStep({
  challenge,
  onComplete,
  onBack,
}: {
  challenge: Extract<MfaLoginPhase, { kind: "mfa_pending" }>;
  onComplete: (session: UserLoginResponse) => void;
  onBack: () => void;
}) {
  const [code, setCode] = useState("");
  const [loading, setLoading] = useState(false);
  const [enroll, setEnroll] = useState(false);
  const [setup, setSetup] = useState<MfaSetupResponse | null>(null);

  const startEnroll = async () => {
    setLoading(true);
    try {
      const res = await beginMfaEnrollment(challenge.serverUrl, challenge.pendingToken);
      setSetup(res);
      setEnroll(true);
      toast({ type: "info", title: "Scan the secret in your authenticator app" });
    } catch (err) {
      toast({ type: "error", title: "MFA setup failed", description: String(err) });
    } finally {
      setLoading(false);
    }
  };

  const submit = async () => {
    if (!code.trim()) return;
    setLoading(true);
    try {
      let session: UserLoginResponse;
      if (enroll) {
        session = await enrollAndValidateMfa(challenge.serverUrl, challenge.pendingToken, code.trim());
      } else {
        try {
          session = await completeMfaLogin(challenge.serverUrl, challenge.pendingToken, code.trim());
        } catch (err) {
          const msg = String(err);
          if (msg.includes("not enabled") || msg.includes("not configured") || msg.includes("not set up")) {
            await startEnroll();
            toast({
              type: "info",
              title: "Authenticator required",
              description: "Set up MFA, then enter a new code from the app.",
            });
            return;
          }
          throw err;
        }
      }
      onComplete(session);
    } catch (err) {
      toast({ type: "error", title: "MFA failed", description: String(err) });
    } finally {
      setLoading(false);
    }
  };

  return (
    <div className="w-full max-w-[300px]">
      <StepHeader
        icon={Shield}
        title="Two-factor authentication"
        description={
          enroll
            ? "Add this account in your authenticator, then enter a 6-digit code."
            : "Enter the code from your authenticator app to continue."
        }
        step={2}
      />

      {enroll && setup && (
        <div className="mb-4 p-3 rounded-md bg-surface border border-border-subtle space-y-2">
          <p className="text-xs text-secondary break-all">
            <span className="font-medium">Secret: </span>
            {setup.secret_base32}
          </p>
          <p className="text-[10px] text-tertiary">
            Save backup codes somewhere safe. They can unlock your account once.
          </p>
          <ul className="text-[10px] font-mono text-tertiary grid grid-cols-2 gap-1">
            {setup.backup_codes.map((c) => (
              <li key={c}>{c}</li>
            ))}
          </ul>
        </div>
      )}

      <div className="space-y-3 mb-6">
        <Input
          label="Authentication code"
          value={code}
          onChange={setCode}
          placeholder="123456"
        />
      </div>

      <Button className="w-full gap-1" onClick={submit} disabled={loading || !code.trim()}>
        {loading ? "Verifying..." : enroll ? "Enable MFA & sign in" : "Verify"}
      </Button>

      {!enroll && (
        <Button variant="ghost" className="w-full mt-2" onClick={startEnroll} disabled={loading}>
          Set up authenticator first
        </Button>
      )}
      <Button variant="ghost" className="w-full mt-1" onClick={onBack} disabled={loading}>
        Back
      </Button>
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
