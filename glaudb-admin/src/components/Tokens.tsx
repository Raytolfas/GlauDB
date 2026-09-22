import { useEffect, useState } from "react";
import { Plus, Trash, Copy, Check, Key } from "lucide-react";
import { authorizedFetch } from "../api";

interface ApiToken {
  id: string;
  name: string;
  role: string;
  created: string;
  expires: string | null;
  key_hash: string;
}

interface Props {
  token: string;
}

export default function Tokens({ token }: Props) {
  const [tokens, setTokens] = useState<ApiToken[]>([]);
  const [newToken, setNewToken] = useState<string | null>(null);
  const [copied, setCopied] = useState(false);
  const [showCreate, setShowCreate] = useState(false);
  const [form, setForm] = useState({ name: "", role: "read", expires_in_hours: "" });

  const load = async () => {
    const res = await authorizedFetch(token, "/api/tokens");
    if (res.ok) setTokens(await res.json());
  };

  useEffect(() => { load(); }, []);

  const create = async (e: React.FormEvent) => {
    e.preventDefault();
    const body: any = { name: form.name, role: form.role };
    if (form.expires_in_hours) body.expires_in_hours = parseInt(form.expires_in_hours);
    const res = await authorizedFetch(token, "/api/tokens", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(body),
    });
    if (res.ok) {
      const data = await res.json();
      setNewToken(data.token);
      setShowCreate(false);
      setForm({ name: "", role: "read", expires_in_hours: "" });
      load();
    }
  };

  const revoke = async (id: string) => {
    if (!confirm("Revoke this token?")) return;
    await authorizedFetch(token, `/api/tokens/${id}`, { method: "DELETE" });
    load();
  };

  const copy = () => {
    if (newToken) {
      navigator.clipboard.writeText(newToken);
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    }
  };

  const roleBadge = (role: string) => {
    const colors: Record<string, string> = {
      admin: "bg-red-100 text-red-700",
      write: "bg-yellow-100 text-yellow-700",
      read: "bg-green-100 text-green-700",
    };
    return (
      <span className={`inline-flex items-center px-2 py-0.5 rounded text-xs font-bold uppercase ${colors[role] ?? "bg-neutral-100 text-neutral-600"}`}>
        {role}
      </span>
    );
  };

  return (
    <div className="space-y-6">
      <div className="flex items-center justify-between border-b border-neutral-200 pb-4">
        <h1 className="text-xl font-bold tracking-tight text-neutral-900">API Tokens</h1>
        <button
          onClick={() => setShowCreate(true)}
          className="flex items-center gap-1.5 border border-neutral-950 bg-neutral-950 px-3 py-1.5 text-xs font-bold uppercase tracking-wider text-white hover:bg-neutral-800 rounded-md"
        >
          <Plus className="h-3.5 w-3.5" /> New Token
        </button>
      </div>

      {/* New Token reveal */}
      {newToken && (
        <div className="border border-green-300 bg-green-50 rounded-lg p-4">
          <p className="text-xs font-bold uppercase text-green-700 mb-2">Token created — copy it now, it won't be shown again</p>
          <div className="flex items-center gap-2">
            <code className="flex-1 bg-white border border-green-200 rounded px-3 py-2 text-sm font-mono text-green-900 break-all">
              {newToken}
            </code>
            <button onClick={copy} className="p-2 rounded-md border border-green-300 bg-white hover:bg-green-50">
              {copied ? <Check className="h-4 w-4 text-green-600" /> : <Copy className="h-4 w-4 text-green-600" />}
            </button>
          </div>
          <button onClick={() => setNewToken(null)} className="mt-2 text-xs text-green-600 underline">Dismiss</button>
        </div>
      )}

      {/* Create form */}
      {showCreate && (
        <div className="border border-neutral-200 bg-white rounded-lg p-5 shadow-sm">
          <h2 className="text-sm font-bold mb-4 uppercase tracking-wider">Create API Token</h2>
          <form onSubmit={create} className="space-y-3">
            <div>
              <label className="block text-xs font-bold uppercase text-neutral-500 mb-1">Name</label>
              <input
                required
                value={form.name}
                onChange={e => setForm(f => ({ ...f, name: e.target.value }))}
                placeholder="my-service-token"
                className="w-full border border-neutral-300 rounded-md px-3 py-2 text-sm font-mono focus:border-neutral-950 focus:outline-none"
              />
            </div>
            <div>
              <label className="block text-xs font-bold uppercase text-neutral-500 mb-1">Role</label>
              <select
                value={form.role}
                onChange={e => setForm(f => ({ ...f, role: e.target.value }))}
                className="w-full border border-neutral-300 rounded-md px-3 py-2 text-sm font-mono bg-white focus:border-neutral-950 focus:outline-none"
              >
                <option value="read">read</option>
                <option value="write">write</option>
                <option value="admin">admin</option>
              </select>
            </div>
            <div>
              <label className="block text-xs font-bold uppercase text-neutral-500 mb-1">Expires in (hours) — leave blank for never</label>
              <input
                type="number"
                value={form.expires_in_hours}
                onChange={e => setForm(f => ({ ...f, expires_in_hours: e.target.value }))}
                placeholder="720"
                className="w-full border border-neutral-300 rounded-md px-3 py-2 text-sm font-mono focus:border-neutral-950 focus:outline-none"
              />
            </div>
            <div className="flex gap-2 pt-1">
              <button type="submit" className="border border-neutral-950 bg-neutral-950 px-4 py-2 text-xs font-bold uppercase tracking-wider text-white hover:bg-neutral-800 rounded-md">
                Create
              </button>
              <button type="button" onClick={() => setShowCreate(false)} className="border border-neutral-300 px-4 py-2 text-xs font-bold uppercase tracking-wider text-neutral-600 hover:bg-neutral-100 rounded-md">
                Cancel
              </button>
            </div>
          </form>
        </div>
      )}

      {/* Token list */}
      <div className="border border-neutral-200 bg-white rounded-lg shadow-sm overflow-hidden">
        {tokens.length === 0 ? (
          <div className="flex flex-col items-center justify-center py-16 text-neutral-400">
            <Key className="h-8 w-8 mb-3 opacity-30" />
            <p className="text-sm">No API tokens yet</p>
          </div>
        ) : (
          <table className="w-full text-xs">
            <thead>
              <tr className="border-b border-neutral-200 bg-neutral-50 text-left">
                <th className="px-4 py-2.5 font-bold uppercase tracking-wider text-neutral-500">Name</th>
                <th className="px-4 py-2.5 font-bold uppercase tracking-wider text-neutral-500">Role</th>
                <th className="px-4 py-2.5 font-bold uppercase tracking-wider text-neutral-500">Created</th>
                <th className="px-4 py-2.5 font-bold uppercase tracking-wider text-neutral-500">Expires</th>
                <th className="px-4 py-2.5"></th>
              </tr>
            </thead>
            <tbody className="divide-y divide-neutral-100">
              {tokens.map(t => (
                <tr key={t.id} className="hover:bg-neutral-50">
                  <td className="px-4 py-3 font-mono font-bold text-neutral-900">{t.name}</td>
                  <td className="px-4 py-3">{roleBadge(t.role)}</td>
                  <td className="px-4 py-3 text-neutral-500 font-mono">{new Date(t.created).toLocaleDateString()}</td>
                  <td className="px-4 py-3 text-neutral-500 font-mono">{t.expires ? new Date(t.expires).toLocaleDateString() : "Never"}</td>
                  <td className="px-4 py-3">
                    <button onClick={() => revoke(t.id)} className="text-red-500 hover:text-red-700 p-1 rounded">
                      <Trash className="h-3.5 w-3.5" />
                    </button>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </div>
    </div>
  );
}
