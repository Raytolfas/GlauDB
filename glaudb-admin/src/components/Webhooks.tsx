import { useEffect, useState } from "react";
import { Plus, Trash, Webhook } from "lucide-react";
import { authorizedFetch } from "../api";

interface WebhookItem {
  id: string;
  collection: string;
  event: string;
  url: string;
  active: boolean;
  created: string;
}

interface Props {
  token: string;
}

export default function Webhooks({ token }: Props) {
  const [hooks, setHooks] = useState<WebhookItem[]>([]);
  const [collections, setCollections] = useState<string[]>([]);
  const [showCreate, setShowCreate] = useState(false);
  const [form, setForm] = useState({ id: "", collection: "", event: "after_insert", url: "", active: true });

  const load = async () => {
    const [hRes, cRes] = await Promise.all([
      authorizedFetch(token, "/api/webhooks"),
      authorizedFetch(token, "/api/collections"),
    ]);
    if (hRes.ok) setHooks(await hRes.json());
    if (cRes.ok) {
      const cols = await cRes.json();
      setCollections(cols.map((c: any) => c.name));
      if (cols.length > 0 && !form.collection) {
        setForm(f => ({ ...f, collection: cols[0].name }));
      }
    }
  };

  useEffect(() => { load(); }, []);

  const create = async (e: React.FormEvent) => {
    e.preventDefault();
    const id = typeof crypto !== "undefined" && crypto.randomUUID ? crypto.randomUUID() : Math.random().toString(36).substring(2);
    const payload = {
      ...form,
      id,
      created: new Date().toISOString(),
    };
    const res = await authorizedFetch(token, "/api/webhooks", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(payload),
    });
    if (res.ok) {
      setShowCreate(false);
      setForm({ id: "", collection: "", event: "after_insert", url: "", active: true });
      load();
    }
  };

  const remove = async (id: string) => {
    if (!confirm("Delete this webhook?")) return;
    await authorizedFetch(token, `/api/webhooks/${id}`, { method: "DELETE" });
    load();
  };

  const eventBadge = (event: string) => {
    const colors: Record<string, string> = {
      after_insert: "bg-blue-100 text-blue-700",
      after_update: "bg-yellow-100 text-yellow-700",
      after_delete: "bg-red-100 text-red-700",
    };
    return (
      <span className={`inline-flex items-center px-2 py-0.5 rounded text-xs font-bold uppercase ${colors[event] ?? "bg-neutral-100 text-neutral-600"}`}>
        {event.replace("after_", "")}
      </span>
    );
  };

  return (
    <div className="space-y-6">
      <div className="flex items-center justify-between border-b border-neutral-200 pb-4">
        <h1 className="text-xl font-bold tracking-tight text-neutral-900">Webhooks</h1>
        <button
          onClick={() => setShowCreate(true)}
          className="flex items-center gap-1.5 border border-neutral-950 bg-neutral-950 px-3 py-1.5 text-xs font-bold uppercase tracking-wider text-white hover:bg-neutral-800 rounded-md"
        >
          <Plus className="h-3.5 w-3.5" /> Add Webhook
        </button>
      </div>

      {showCreate && (
        <div className="border border-neutral-200 bg-white rounded-lg p-5 shadow-sm">
          <h2 className="text-sm font-bold mb-4 uppercase tracking-wider">New Webhook</h2>
          <form onSubmit={create} className="space-y-3">
            <div className="grid grid-cols-2 gap-3">
              <div>
                <label className="block text-xs font-bold uppercase text-neutral-500 mb-1">Collection</label>
                <select
                  value={form.collection}
                  onChange={e => setForm(f => ({ ...f, collection: e.target.value }))}
                  className="w-full border border-neutral-300 rounded-md px-3 py-2 text-sm font-mono bg-white focus:border-neutral-950 focus:outline-none"
                >
                  {collections.map(c => <option key={c} value={c}>{c}</option>)}
                </select>
              </div>
              <div>
                <label className="block text-xs font-bold uppercase text-neutral-500 mb-1">Event</label>
                <select
                  value={form.event}
                  onChange={e => setForm(f => ({ ...f, event: e.target.value }))}
                  className="w-full border border-neutral-300 rounded-md px-3 py-2 text-sm font-mono bg-white focus:border-neutral-950 focus:outline-none"
                >
                  <option value="after_insert">After Insert</option>
                  <option value="after_update">After Update</option>
                  <option value="after_delete">After Delete</option>
                </select>
              </div>
            </div>
            <div>
              <label className="block text-xs font-bold uppercase text-neutral-500 mb-1">URL</label>
              <input
                required
                type="url"
                value={form.url}
                onChange={e => setForm(f => ({ ...f, url: e.target.value }))}
                placeholder="https://yourapp.com/hooks/glaudb"
                className="w-full border border-neutral-300 rounded-md px-3 py-2 text-sm font-mono focus:border-neutral-950 focus:outline-none"
              />
            </div>
            <div className="flex gap-2 pt-1">
              <button type="submit" className="border border-neutral-950 bg-neutral-950 px-4 py-2 text-xs font-bold uppercase tracking-wider text-white hover:bg-neutral-800 rounded-md">
                Save
              </button>
              <button type="button" onClick={() => setShowCreate(false)} className="border border-neutral-300 px-4 py-2 text-xs font-bold uppercase tracking-wider text-neutral-600 hover:bg-neutral-100 rounded-md">
                Cancel
              </button>
            </div>
          </form>
        </div>
      )}

      <div className="border border-neutral-200 bg-white rounded-lg shadow-sm overflow-hidden">
        {hooks.length === 0 ? (
          <div className="flex flex-col items-center justify-center py-16 text-neutral-400">
            <Webhook className="h-8 w-8 mb-3 opacity-30" />
            <p className="text-sm">No webhooks registered</p>
          </div>
        ) : (
          <table className="w-full text-xs">
            <thead>
              <tr className="border-b border-neutral-200 bg-neutral-50 text-left">
                <th className="px-4 py-2.5 font-bold uppercase tracking-wider text-neutral-500">Collection</th>
                <th className="px-4 py-2.5 font-bold uppercase tracking-wider text-neutral-500">Event</th>
                <th className="px-4 py-2.5 font-bold uppercase tracking-wider text-neutral-500">URL</th>
                <th className="px-4 py-2.5 font-bold uppercase tracking-wider text-neutral-500">Created</th>
                <th className="px-4 py-2.5"></th>
              </tr>
            </thead>
            <tbody className="divide-y divide-neutral-100">
              {hooks.map(h => (
                <tr key={h.id} className="hover:bg-neutral-50">
                  <td className="px-4 py-3 font-mono font-bold text-neutral-900">{h.collection}</td>
                  <td className="px-4 py-3">{eventBadge(h.event)}</td>
                  <td className="px-4 py-3 font-mono text-neutral-500 max-w-xs truncate">{h.url}</td>
                  <td className="px-4 py-3 font-mono text-neutral-400">{new Date(h.created).toLocaleDateString()}</td>
                  <td className="px-4 py-3">
                    <button onClick={() => remove(h.id)} className="text-red-500 hover:text-red-700 p-1 rounded">
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
