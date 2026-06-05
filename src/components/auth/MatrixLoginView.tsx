import { useState, useEffect } from "react";
import { Lock, Globe, User, Eye, EyeOff } from "lucide-react";
import { cn } from "@/lib/cn";
import { matrixLogin, getConfig, saveSettings, storeSipPassword } from "@/lib/tauri";
import { toast } from "@/components/ui/Toast";
import { Button } from "@/components/ui/Button";

export function MatrixLoginView() {
  const [homeserver, setHomeserver] = useState("");
  const [username, setUsername] = useState("");
  const [password, setPassword] = useState("");
  const [showPassword, setShowPassword] = useState(false);
  const [loading, setLoading] = useState(false);

  // Load saved homeserver from config
  useEffect(() => {
    getConfig()
      .then((config) => {
        if (config.matrix?.homeserver) setHomeserver(config.matrix.homeserver);
        if (config.matrix?.username) setUsername(config.matrix.username);
      })
      .catch(() => {});
  }, []);

  const handleLogin = async () => {
    if (!homeserver || !username || !password) return;
    setLoading(true);
    try {
      const userId = await matrixLogin(homeserver, username, password);

      // Persist homeserver + username to config (password goes to keychain)
      const config = await getConfig().catch(() => null);
      if (config) {
        config.matrix = { homeserver, username, user_id: userId };
        await saveSettings(config).catch(() => {});
      }
      await storeSipPassword(`matrix:${username}`, password).catch(() => {});

      toast({ type: "success", title: "Connected", description: userId });
    } catch (err) {
      toast({ type: "error", title: "Login failed", description: String(err) });
    } finally {
      setLoading(false);
    }
  };

  return (
    <div className="flex flex-col items-center justify-center h-full px-6 py-8">
      <div className="w-12 h-12 rounded-2xl bg-accent/10 flex items-center justify-center mb-4">
        <Lock size={24} className="text-accent" />
      </div>

      <h2 className="text-lg font-semibold text-primary mb-1">Connect to Matrix</h2>
      <p className="text-xs text-tertiary text-center mb-6">
        Sign in to your Matrix homeserver for encrypted chat and file sharing
      </p>

      <div className="w-full max-w-[280px] space-y-3">
        <div className="space-y-1.5">
          <label className="text-xs font-medium text-secondary">Homeserver</label>
          <div className="relative">
            <Globe size={14} className="absolute left-3 top-1/2 -translate-y-1/2 text-tertiary" />
            <input
              type="text"
              value={homeserver}
              onChange={(e) => setHomeserver(e.target.value)}
              placeholder="chat.yourcompany.com"
              className={cn(
                "w-full bg-surface border border-border-subtle rounded-lg",
                "pl-8 pr-3 py-2.5 text-sm text-primary",
                "placeholder:text-tertiary",
                "focus:outline-none focus:border-border-focus focus:ring-1 focus:ring-accent/30"
              )}
            />
          </div>
        </div>

        <div className="space-y-1.5">
          <label className="text-xs font-medium text-secondary">Username</label>
          <div className="relative">
            <User size={14} className="absolute left-3 top-1/2 -translate-y-1/2 text-tertiary" />
            <input
              type="text"
              value={username}
              onChange={(e) => setUsername(e.target.value)}
              placeholder="username"
              className={cn(
                "w-full bg-surface border border-border-subtle rounded-lg",
                "pl-8 pr-3 py-2.5 text-sm text-primary",
                "placeholder:text-tertiary",
                "focus:outline-none focus:border-border-focus focus:ring-1 focus:ring-accent/30"
              )}
            />
          </div>
        </div>

        <div className="space-y-1.5">
          <label className="text-xs font-medium text-secondary">Password</label>
          <div className="relative">
            <Lock size={14} className="absolute left-3 top-1/2 -translate-y-1/2 text-tertiary" />
            <input
              type={showPassword ? "text" : "password"}
              value={password}
              onChange={(e) => setPassword(e.target.value)}
              placeholder="Password"
              onKeyDown={(e) => e.key === "Enter" && handleLogin()}
              className={cn(
                "w-full bg-surface border border-border-subtle rounded-lg",
                "pl-8 pr-9 py-2.5 text-sm text-primary",
                "placeholder:text-tertiary",
                "focus:outline-none focus:border-border-focus focus:ring-1 focus:ring-accent/30"
              )}
            />
            <button
              onClick={() => setShowPassword(!showPassword)}
              className="absolute right-3 top-1/2 -translate-y-1/2 text-tertiary hover:text-secondary"
            >
              {showPassword ? <EyeOff size={14} /> : <Eye size={14} />}
            </button>
          </div>
        </div>

        <Button
          variant="primary"
          className="w-full mt-4"
          onClick={handleLogin}
          disabled={loading || !homeserver || !username || !password}
        >
          {loading ? "Signing in..." : "Sign In"}
        </Button>

        <p className="text-[10px] text-tertiary text-center mt-3">
          Connects to your private homeserver. All messages are end-to-end encrypted.
        </p>
      </div>
    </div>
  );
}
