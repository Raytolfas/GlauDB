import { useEffect, useState } from "react";
import { Server, Database, Activity, Cpu } from "lucide-react";

interface Metrics {
  cpu_usage: number;
  memory_used: number;
  memory_total: number;
  db_size: number;
  collections_count: number;
  active_tcp_connections: number;
}

export default function Dashboard() {
  const [metrics, setMetrics] = useState<Metrics>({
    cpu_usage: 0,
    memory_used: 0,
    memory_total: 0,
    db_size: 0,
    collections_count: 0,
    active_tcp_connections: 0,
  });

  const [cpuHistory, setCpuHistory] = useState<number[]>(new Array(30).fill(0));
  const [memHistory, setMemHistory] = useState<number[]>(new Array(30).fill(0));

  useEffect(() => {
    const wsUrl =
      window.location.port === "5173"
        ? "ws://localhost:8080/api/metrics/ws"
        : `ws://${window.location.host}/api/metrics/ws`;

    const socket = new WebSocket(wsUrl);

    socket.onmessage = (event) => {
      try {
        const data: Metrics = JSON.parse(event.data);
        setMetrics(data);
        
        setCpuHistory((prev) => {
          const next = [...prev.slice(1), data.cpu_usage];
          return next;
        });

        const memPercent = data.memory_total > 0 ? (data.memory_used / data.memory_total) * 100 : 0;
        setMemHistory((prev) => {
          const next = [...prev.slice(1), memPercent];
          return next;
        });
      } catch (e) {
        console.error("Error parsing metrics WS data:", e);
      }
    };

    return () => {
      socket.close();
    };
  }, []);

  const formatBytes = (bytes: number) => {
    if (bytes === 0) return "0 B";
    const k = 1024;
    const sizes = ["B", "KB", "MB", "GB"];
    const i = Math.floor(Math.log(bytes) / Math.log(k));
    return parseFloat((bytes / Math.pow(k, i)).toFixed(2)) + " " + sizes[i];
  };

  const renderSparkline = (data: number[], color: string, maxVal: number = 100) => {
    const width = 300;
    const height = 60;
    const padding = 2;
    const step = width / (data.length - 1);
    
    const points = data
      .map((val, index) => {
        const x = index * step;
        const ratio = val / maxVal;
        const y = height - ratio * (height - padding * 2) - padding;
        return `${x},${y}`;
      })
      .join(" ");

    return (
      <svg width="100%" height={height} viewBox={`0 0 ${width} ${height}`} className="overflow-visible">
        <polyline
          fill="none"
          stroke={color}
          strokeWidth="1.5"
          points={points}
        />
      </svg>
    );
  };

  const formatMemGb = (bytes: number) => {
    return (bytes / (1024 * 1024 * 1024)).toFixed(2);
  };

  return (
    <div className="space-y-6">
      <div className="flex items-center justify-between border-b border-neutral-200 pb-4">
        <h1 className="text-xl font-bold tracking-tight text-neutral-900">Dashboard</h1>
      </div>

      <div className="grid grid-cols-1 gap-4 md:grid-cols-4">
        {/* Metric Card 1 */}
        <div className="border border-neutral-200 bg-white p-4 rounded-lg shadow-sm">
          <div className="flex items-center justify-between">
            <span className="text-xs uppercase tracking-wider text-neutral-500 font-semibold">Database Size</span>
            <Database className="h-4 w-4 text-neutral-400" />
          </div>
          <p className="mt-2 text-2xl font-bold font-mono tracking-tight text-neutral-900">
            {formatBytes(metrics.db_size)}
          </p>
        </div>

        {/* Metric Card 2 */}
        <div className="border border-neutral-200 bg-white p-4 rounded-lg shadow-sm">
          <div className="flex items-center justify-between">
            <span className="text-xs uppercase tracking-wider text-neutral-500 font-semibold">Active TCP Conns</span>
            <Activity className="h-4 w-4 text-neutral-400" />
          </div>
          <p className="mt-2 text-2xl font-bold font-mono tracking-tight text-neutral-900">
            {metrics.active_tcp_connections}
          </p>
        </div>

        {/* Metric Card 3 */}
        <div className="border border-neutral-200 bg-white p-4 rounded-lg shadow-sm">
          <div className="flex items-center justify-between">
            <span className="text-xs uppercase tracking-wider text-neutral-500 font-semibold">CPU Usage</span>
            <Cpu className="h-4 w-4 text-neutral-400" />
          </div>
          <p className="mt-2 text-2xl font-bold font-mono tracking-tight text-neutral-900">
            {metrics.cpu_usage.toFixed(1)}%
          </p>
        </div>

        {/* Metric Card 4 */}
        <div className="border border-neutral-200 bg-white p-4 rounded-lg shadow-sm">
          <div className="flex items-center justify-between">
            <span className="text-xs uppercase tracking-wider text-neutral-500 font-semibold">Collections</span>
            <Server className="h-4 w-4 text-neutral-400" />
          </div>
          <p className="mt-2 text-2xl font-bold font-mono tracking-tight text-neutral-900">
            {metrics.collections_count}
          </p>
        </div>
      </div>

      <div className="grid grid-cols-1 gap-4 md:grid-cols-2">
        {/* CPU Chart */}
        <div className="border border-neutral-200 bg-white p-4 rounded-lg shadow-sm">
          <div className="mb-2 flex items-center justify-between">
            <h3 className="text-sm font-bold text-neutral-900 uppercase">CPU Usage History</h3>
            <span className="text-xs font-mono text-neutral-500">{metrics.cpu_usage.toFixed(0)}%</span>
          </div>
          <div className="bg-neutral-50 p-2 border border-neutral-100 rounded-md">
            {renderSparkline(cpuHistory, "#000000")}
          </div>
        </div>

        {/* Memory Chart */}
        <div className="border border-neutral-200 bg-white p-4 rounded-lg shadow-sm">
          <div className="mb-2 flex items-center justify-between">
            <h3 className="text-sm font-bold text-neutral-900 uppercase">Memory Utilization</h3>
            <span className="text-xs font-mono text-neutral-500">
              {formatMemGb(metrics.memory_used)} / {formatMemGb(metrics.memory_total)} GB
            </span>
          </div>
          <div className="bg-neutral-50 p-2 border border-neutral-100 rounded-md">
            {renderSparkline(memHistory, "#000000")}
          </div>
        </div>
      </div>
    </div>
  );
}
