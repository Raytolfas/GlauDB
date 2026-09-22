import { useEffect, useState } from "react";
import { authorizedFetch } from "../api";

interface S3Config {
  enabled: boolean;
  bucket: string;
  region: string;
  endpoint: string;
  access_key: string;
  secret_key: string;
  force_path_style: boolean;
}

export default function Settings({ token }: { token: string }) {
  const [config, setConfig] = useState<S3Config>({
    enabled: false,
    bucket: "",
    region: "us-east-1",
    endpoint: "https://s3.amazonaws.com",
    access_key: "",
    secret_key: "",
    force_path_style: true,
  });

  const [saving, setSaving] = useState(false);
  const [msg, setMsg] = useState("");

  useEffect(() => {
    authorizedFetch(token, "/api/settings")
      .then((res) => res.json())
      .then((data) => setConfig(data))
      .catch((err) => console.error("Error loading settings:", err));
  }, [token]);

  const handleSave = async (e: React.FormEvent) => {
    e.preventDefault();
    setSaving(true);
    setMsg("");

    try {
      const res = await authorizedFetch(token, "/api/settings", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify(config),
      });

      if (res.ok) {
        setMsg("Settings saved successfully.");
      } else {
        setMsg("Failed to save settings.");
      }
    } catch (err) {
      setMsg("Error connecting to server.");
    } finally {
      setSaving(false);
    }
  };

  return (
    <div className="space-y-6">
      <div className="flex items-center justify-between border-b border-neutral-200 pb-4">
        <h1 className="text-xl font-bold tracking-tight text-neutral-900">Settings</h1>
      </div>

      <form onSubmit={handleSave} className="max-w-xl space-y-4 border border-neutral-200 bg-white p-6">
        <h2 className="text-sm font-bold uppercase tracking-wider text-neutral-900">S3 File Storage Configuration</h2>

        <div className="flex items-center space-x-2 pb-2">
          <input
            type="checkbox"
            id="enabled"
            checked={config.enabled}
            onChange={(e) => setConfig({ ...config, enabled: e.target.checked })}
            className="h-4 w-4 border-neutral-300 text-neutral-950 focus:ring-neutral-950 rounded-none"
          />
          <label htmlFor="enabled" className="text-sm font-bold text-neutral-700 uppercase">
            Enable S3 Storage
          </label>
        </div>

        <div className="space-y-3 pt-2">
          <div>
            <label className="block text-xs font-bold uppercase text-neutral-500 mb-1">Bucket Name</label>
            <input
              type="text"
              value={config.bucket}
              onChange={(e) => setConfig({ ...config, bucket: e.target.value })}
              className="w-full border border-neutral-300 px-3 py-1.5 text-sm font-mono focus:border-neutral-950 focus:outline-none rounded-none"
              required={config.enabled}
            />
          </div>

          <div>
            <label className="block text-xs font-bold uppercase text-neutral-500 mb-1">Region</label>
            <input
              type="text"
              value={config.region}
              onChange={(e) => setConfig({ ...config, region: e.target.value })}
              className="w-full border border-neutral-300 px-3 py-1.5 text-sm font-mono focus:border-neutral-950 focus:outline-none rounded-none"
              required={config.enabled}
            />
          </div>

          <div>
            <label className="block text-xs font-bold uppercase text-neutral-500 mb-1">Endpoint (R2/MinIO/S3)</label>
            <input
              type="text"
              value={config.endpoint}
              onChange={(e) => setConfig({ ...config, endpoint: e.target.value })}
              className="w-full border border-neutral-300 px-3 py-1.5 text-sm font-mono focus:border-neutral-950 focus:outline-none rounded-none"
              required={config.enabled}
            />
          </div>

          <div>
            <label className="block text-xs font-bold uppercase text-neutral-500 mb-1">Access Key</label>
            <input
              type="text"
              value={config.access_key}
              onChange={(e) => setConfig({ ...config, access_key: e.target.value })}
              className="w-full border border-neutral-300 px-3 py-1.5 text-sm font-mono focus:border-neutral-950 focus:outline-none rounded-none"
              required={config.enabled}
            />
          </div>

          <div>
            <label className="block text-xs font-bold uppercase text-neutral-500 mb-1">Secret Key</label>
            <input
              type="password"
              value={config.secret_key}
              onChange={(e) => setConfig({ ...config, secret_key: e.target.value })}
              className="w-full border border-neutral-300 px-3 py-1.5 text-sm font-mono focus:border-neutral-950 focus:outline-none rounded-none"
              required={config.enabled}
            />
          </div>

          <div className="flex items-center space-x-2 pt-2">
            <input
              type="checkbox"
              id="force_path_style"
              checked={config.force_path_style}
              onChange={(e) => setConfig({ ...config, force_path_style: e.target.checked })}
              className="h-4 w-4 border-neutral-300 text-neutral-950 focus:ring-neutral-950 rounded-none"
            />
            <label htmlFor="force_path_style" className="text-xs font-bold text-neutral-500 uppercase">
              Force Path Style (recommended for MinIO/R2)
            </label>
          </div>
        </div>

        <div className="pt-4 flex items-center justify-between">
          <button
            type="submit"
            disabled={saving}
            className="border border-neutral-950 bg-neutral-950 px-4 py-2 text-xs font-bold uppercase tracking-wider text-white hover:bg-neutral-800 disabled:opacity-50 rounded-none"
          >
            {saving ? "Saving..." : "Save Settings"}
          </button>
          {msg && <span className="text-xs font-mono text-neutral-600">{msg}</span>}
        </div>
      </form>
    </div>
  );
}
