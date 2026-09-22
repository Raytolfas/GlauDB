import { useState } from "react";
import type { FormEvent } from "react";
import Dashboard from "./components/Dashboard";
import Collections from "./components/Collections";
import Settings from "./components/Settings";
import Tokens from "./components/Tokens";
import Logs from "./components/Logs";
import Webhooks from "./components/Webhooks";
import { Database, Sliders, LayoutDashboard, LogOut, Key, Terminal, Webhook } from "lucide-react";
import { apiUrl } from "./api";

type Tab = "dashboard" | "collections" | "tokens" | "webhooks" | "logs" | "settings";

interface NavItem {
  id: Tab;
  label: string;
  icon: React.ReactNode;
}

const NAV: NavItem[] = [
  { id: "dashboard", label: "Dashboard", icon: <LayoutDashboard className="h-4 w-4" /> },
  { id: "collections", label: "Collections", icon: <Database className="h-4 w-4" /> },
  { id: "tokens", label: "Tokens", icon: <Key className="h-4 w-4" /> },
  { id: "webhooks", label: "Webhooks", icon: <Webhook className="h-4 w-4" /> },
  { id: "logs", label: "Logs", icon: <Terminal className="h-4 w-4" /> },
  { id: "settings", label: "Settings", icon: <Sliders className="h-4 w-4" /> },
];

export default function App() {
  const [activeTab, setActiveTab] = useState<Tab>("dashboard");
  const [token, setToken] = useState(() => localStorage.getItem("glaudb_admin_token"));

  if (!token) {
    return <LoginScreen onLogin={setToken} />;
  }

  const logout = () => {
    localStorage.removeItem("glaudb_admin_token");
    setToken(null);
  };

  return (
    <div className="flex h-screen w-screen overflow-hidden bg-neutral-50">
      {/* Sidebar Navigation */}
      <aside className="w-56 border-r border-neutral-200 bg-white flex flex-col justify-between">
        <div>
          {/* Logo / Title */}
          <div className="px-6 py-5 border-b border-neutral-200">
            <h1 className="text-sm font-black tracking-[0.2em] text-neutral-950 uppercase font-mono">
              GLAUDB
            </h1>
          </div>

          {/* Navigation Links */}
          <nav className="p-3 space-y-1">
            {NAV.map(item => (
              <button
                key={item.id}
                onClick={() => setActiveTab(item.id)}
                className={`w-full flex items-center space-x-2.5 px-3 py-2 text-xs font-bold uppercase tracking-wider transition-colors rounded-md ${
                  activeTab === item.id
                    ? "bg-neutral-950 text-white"
                    : "text-neutral-600 hover:bg-neutral-100 hover:text-neutral-900"
                }`}
              >
                {item.icon}
                <span>{item.label}</span>
              </button>
            ))}
          </nav>
        </div>

        {/* Footer */}
        <div className="p-3 border-t border-neutral-200">
          <button onClick={logout} className="flex w-full items-center gap-2 rounded-md px-2 py-2 text-xs font-bold uppercase tracking-wider text-neutral-500 hover:bg-neutral-100 hover:text-neutral-950">
            <LogOut className="h-3.5 w-3.5" />
            Sign out
          </button>
        </div>
      </aside>

      {/* Main Workspaces */}
      <main className="flex-1 overflow-auto p-8">
        {activeTab === "dashboard" && <Dashboard />}
        {activeTab === "collections" && <Collections token={token} />}
        {activeTab === "tokens" && <Tokens token={token} />}
        {activeTab === "webhooks" && <Webhooks token={token} />}
        {activeTab === "logs" && <Logs token={token} />}
        {activeTab === "settings" && <Settings token={token} />}
      </main>
    </div>
  );
}

function LoginScreen({ onLogin }: { onLogin: (token: string) => void }) {
  const [password, setPassword] = useState("");
  const [error, setError] = useState("");
  const [loading, setLoading] = useState(false);

  const submit = async (event: FormEvent) => {
    event.preventDefault();
    setLoading(true);
    setError("");
    try {
      const response = await fetch(apiUrl("/api/login"), {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ password }),
      });
      const body = await response.json();
      if (!response.ok || !body.token) {
        setError(body.error || "Unable to sign in");
        return;
      }
      localStorage.setItem("glaudb_admin_token", body.token);
      onLogin(body.token);
    } catch {
      setError("Cannot connect to GlauDB server");
    } finally {
      setLoading(false);
    }
  };

  return (
    <main className="flex min-h-screen items-center justify-center bg-neutral-50 p-6">
      <form onSubmit={submit} className="w-full max-w-sm rounded-lg border border-neutral-200 bg-white p-7 shadow-sm">
        <p className="font-mono text-xs font-black tracking-[0.2em] text-neutral-950">GLAUDB</p>
        <h1 className="mt-6 text-xl font-bold text-neutral-950">Sign in to the admin panel</h1>
        <label className="mt-6 block text-xs font-bold uppercase tracking-wider text-neutral-600">
          Admin password
          <input autoFocus required type="password" value={password} onChange={(event) => setPassword(event.target.value)} className="mt-2 w-full rounded-md border border-neutral-300 px-3 py-2 text-sm font-mono outline-none focus:border-neutral-950" />
        </label>
        {error && <p className="mt-3 text-sm text-red-600">{error}</p>}
        <button disabled={loading} className="mt-6 w-full rounded-md bg-neutral-950 px-4 py-2 text-xs font-bold uppercase tracking-wider text-white hover:bg-neutral-800 disabled:opacity-50">
          {loading ? "Signing in..." : "Sign in"}
        </button>
      </form>
    </main>
  );
}
