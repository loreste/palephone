import { useState, useCallback, useEffect } from "react";
import { Phone, Delete, Zap, X } from "lucide-react";
import { cn } from "@/lib/cn";
import { motion } from "framer-motion";
import { toast } from "@/components/ui/Toast";
import { makeCall as ipcMakeCall, paleServerApi } from "@/lib/tauri";
import { useServerStore } from "@/store/serverStore";
import { useAccountStore } from "@/store/accountStore";
import type { SipAccount } from "@/types";

const dialpadKeys = [
  { digit: "1", letters: "" },
  { digit: "2", letters: "ABC" },
  { digit: "3", letters: "DEF" },
  { digit: "4", letters: "GHI" },
  { digit: "5", letters: "JKL" },
  { digit: "6", letters: "MNO" },
  { digit: "7", letters: "PQRS" },
  { digit: "8", letters: "TUV" },
  { digit: "9", letters: "WXYZ" },
  { digit: "*", letters: "" },
  { digit: "0", letters: "+" },
  { digit: "#", letters: "" },
];

interface SpeedDial {
  code: string;
  destination: string;
  label: string;
}

/**
 * Derive the SIP domain to use for bare-number dialing from the registered
 * account: prefer the host part of the account's SIP URI, falling back to
 * the registrar URI (stripped of scheme/params).
 */
function accountDomain(account: SipAccount | null): string | null {
  if (!account) return null;
  const fromSipUri = account.sipUri?.split("@")[1]?.split(/[;>]/)[0]?.trim();
  if (fromSipUri) return fromSipUri;
  const fromRegistrar = account.registrarUri
    ?.replace(/^sips?:/, "")
    .split(/[;>]/)[0]
    ?.trim();
  return fromRegistrar || null;
}

export function DialpadView() {
  const [input, setInput] = useState("");
  const [speedDials, setSpeedDials] = useState<SpeedDial[]>([]);
  const baseUrl = useServerStore((s) => s.baseUrl);
  const token = useServerStore((s) => s.token);

  useEffect(() => {
    if (baseUrl && token) {
      paleServerApi<SpeedDial[]>(baseUrl, token, "/v1/speed-dials")
        .then(setSpeedDials)
        .catch(() => {});
    }
  }, [baseUrl, token]);

  const handleDigit = useCallback((digit: string) => {
    setInput((prev) => prev + digit);
  }, []);

  const handleBackspace = useCallback(() => {
    setInput((prev) => prev.slice(0, -1));
  }, []);

  const handleClear = useCallback(() => {
    setInput("");
  }, []);

  const account = useAccountStore((s) => s.account);
  const regState = useAccountStore((s) => s.regState);
  const defaultDomain = accountDomain(account);

  // Bare numbers (no "@") need a registered account to supply the domain;
  // full SIP URIs can be dialed directly.
  const isBareNumber = input.trim().length > 0 && !input.includes("@");
  const canCall =
    input.trim().length > 0 &&
    (!isBareNumber || (regState === "registered" && !!defaultDomain));

  const handleCall = useCallback(async () => {
    const trimmed = input.trim();
    if (!trimmed) return;
    if (!trimmed.includes("@") && (regState !== "registered" || !defaultDomain)) return;

    const uri = trimmed.includes("@")
      ? (trimmed.startsWith("sip:") ? trimmed : `sip:${trimmed}`)
      : `sip:${trimmed}@${defaultDomain}`;
    const name = trimmed.includes("@") ? trimmed.split("@")[0]?.replace("sip:", "") : trimmed;

    toast({ type: "info", title: `Calling ${name}...` });

    try {
      // Real PJSIP call via Tauri IPC — the backend emits CallState events
      // that useSipEvents picks up to create the call session.
      await ipcMakeCall(uri);
    } catch (err) {
      toast({ type: "error", title: "Call failed", description: String(err) });
    }
  }, [input, regState, defaultDomain]);

  return (
    <div className="flex flex-col items-center justify-between h-full px-6 py-4">
      {/* Input field */}
      <div className="w-full relative">
        <input
          type="text"
          value={input}
          onChange={(e) => setInput(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter" && canCall) {
              e.preventDefault();
              handleCall();
            }
          }}
          placeholder="Enter number or SIP URI"
          className={cn(
            "w-full bg-surface border border-border-subtle rounded-lg",
            "px-4 py-3 text-lg text-center text-primary tabular-nums",
            "placeholder:text-tertiary",
            "focus:outline-none focus:border-border-focus focus:ring-1 focus:ring-accent/30",
            "transition-colors"
          )}
        />
        {input && (
          <button
            onClick={handleClear}
            className="absolute right-3 top-1/2 -translate-y-1/2 text-tertiary hover:text-secondary"
            aria-label="Clear input"
          >
            <X size={16} />
          </button>
        )}
        {isBareNumber && !canCall && (
          <p className="mt-1.5 text-xs text-tertiary text-center">
            Register a SIP account to dial numbers, or enter a full SIP URI (user@domain)
          </p>
        )}
      </div>

      {/* Speed dials */}
      {speedDials.length > 0 && (
        <div className="flex gap-2 w-full overflow-x-auto pb-1 px-1">
          {speedDials.map((sd) => (
            <button
              key={sd.code}
              onClick={() => {
                setInput(sd.destination);
                toast({ type: "info", title: `Calling ${sd.label}...` });
                ipcMakeCall(sd.destination.startsWith("sip:") ? sd.destination : `sip:${sd.destination}`).catch((err) =>
                  toast({ type: "error", title: "Call failed", description: String(err) })
                );
              }}
              className={cn(
                "flex items-center gap-1.5 px-3 py-1.5 rounded-full shrink-0",
                "bg-surface border border-border-subtle text-sm",
                "hover:bg-elevated transition-colors"
              )}
            >
              <Zap size={12} className="text-accent" />
              <span className="text-primary font-medium">{sd.label || sd.code}</span>
            </button>
          ))}
        </div>
      )}

      {/* Dialpad grid */}
      <div className="grid grid-cols-3 gap-3 w-full max-w-[264px] py-4">
        {dialpadKeys.map(({ digit, letters }) => (
          <motion.button
            key={digit}
            whileTap={{ scale: 0.93 }}
            transition={{ type: "spring", stiffness: 500, damping: 30 }}
            onClick={() => handleDigit(digit)}
            className={cn(
              "flex flex-col items-center justify-center",
              "w-full aspect-square max-h-16 rounded-xl",
              "bg-surface border border-border-subtle",
              "hover:bg-elevated active:bg-overlay",
              "transition-colors cursor-pointer"
            )}
          >
            <span className="text-2xl font-medium text-primary leading-none">
              {digit}
            </span>
            {letters && (
              <span className="text-[9px] font-medium text-tertiary tracking-widest mt-0.5">
                {letters}
              </span>
            )}
          </motion.button>
        ))}
      </div>

      {/* Bottom row: backspace, call button, SIP URI toggle */}
      <div className="flex items-center justify-center gap-6 w-full pb-2">
        <button
          onClick={handleBackspace}
          className="p-3 text-tertiary hover:text-secondary transition-colors"
          aria-label="Backspace"
        >
          <Delete size={22} />
        </button>

        <motion.button
          whileTap={{ scale: 0.9 }}
          transition={{ type: "spring", stiffness: 400, damping: 20 }}
          onClick={handleCall}
          disabled={!canCall}
          className={cn(
            "flex items-center justify-center gap-2",
            "w-[72px] h-[56px] rounded-full",
            "bg-success text-inverse font-semibold",
            "hover:brightness-110 active:brightness-90",
            "disabled:opacity-40 disabled:cursor-not-allowed",
            "transition-all shadow-md",
            canCall && "shadow-glow-success"
          )}
          aria-label="Make call"
          style={{
            boxShadow: canCall
              ? "0 0 16px rgba(34, 197, 94, 0.25)"
              : undefined,
          }}
        >
          <Phone size={22} fill="currentColor" />
        </motion.button>

        <div className="w-[46px]" /> {/* Spacer to balance layout */}
      </div>
    </div>
  );
}