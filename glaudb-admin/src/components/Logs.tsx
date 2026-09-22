import { useEffect, useRef, useState } from "react";
import { RefreshCw, Terminal } from "lucide-react";
import { authorizedFetch } from "../api";

interface Props {
  token: string;
}

export default function Logs({ token }: Props) {
  const [logs, setLogs] = useState<string[]>([]);
  const [autoRefresh, setAutoRefresh] = useState(true);
  const [filter, setFilter] = useState("");
  const bottomRef = useRef<HTMLDivElement>(null);

  const load = async () => {
    const res = await authorizedFetch(token, "/api/logs");
    if (res.ok) {
      const data: string[] = await res.json();
      setLogs(data);
    }
  };

  useEffect(() => {
    load();
  }, []);

  useEffect(() => {
    if (!autoRefresh) return;
    const interval = setInterval(load, 2000);
    return () => clearInterval(interval);
  }, [autoRefresh]);

  useEffect(() => {
    if (bottomRef.current) {
      bottomRef.current.scrollIntoView({ behavior: "smooth" });
    }
  }, [logs]);

  const filtered = filter
    ? logs.filter(l => l.toLowerCase().includes(filter.toLowerCase()))
    : logs;

  const levelColor = (line: string) => {
    if (line.includes("ERROR")) return "text-red-400";
    if (line.includes("WARN")) return "text-yellow-400";
    if (line.includes("INFO")) return "text-green-400";
    return "text-neutral-300";
  };

  return (
    <div className="space-y-4">
      <div className="flex items-center justify-between border-b border-neutral-200 pb-4">
        <h1 className="text-xl font-bold tracking-tight text-neutral-900">Server Logs</h1>
        <div className="flex items-center gap-3">
          <input
            value={filter}
            onChange={e => setFilter(e.target.value)}
            placeholder="Filter logs..."
            className="border border-neutral-300 rounded-md px-3 py-1.5 text-xs font-mono focus:border-neutral-950 focus:outline-none w-52"
          />
          <button
            onClick={() => setAutoRefresh(a => !a)}
            className={`flex items-center gap-1.5 px-3 py-1.5 text-xs font-bold uppercase tracking-wider rounded-md border ${
              autoRefresh
                ? "bg-neutral-950 text-white border-neutral-950 hover:bg-neutral-800"
                : "bg-white text-neutral-600 border-neutral-300 hover:bg-neutral-100"
            }`}
          >
            <RefreshCw className={`h-3.5 w-3.5 ${autoRefresh ? "animate-spin" : ""}`} />
            {autoRefresh ? "Live" : "Paused"}
          </button>
          <button onClick={load} className="p-1.5 border border-neutral-300 rounded-md hover:bg-neutral-100">
            <RefreshCw className="h-3.5 w-3.5 text-neutral-600" />
          </button>
        </div>
      </div>

      <div className="bg-neutral-950 rounded-lg overflow-hidden shadow-lg border border-neutral-800">
        <div className="flex items-center gap-2 px-4 py-2 border-b border-neutral-800 bg-neutral-900">
          <Terminal className="h-3.5 w-3.5 text-neutral-500" />
          <span className="text-xs font-mono text-neutral-500">glaudb-server — {filtered.length} entries</span>
        </div>
        <div className="h-[calc(100vh-280px)] overflow-y-auto p-4 space-y-0.5 font-mono text-xs">
          {filtered.length === 0 ? (
            <p className="text-neutral-600 text-center pt-8">No log entries</p>
          ) : (
            filtered.map((line, i) => (
              <div key={i} className={`leading-5 ${levelColor(line)}`}>
                {line}
              </div>
            ))
          )}
          <div ref={bottomRef} />
        </div>
      </div>
    </div>
  );
}
