// App.tsx
import { useEffect, useState } from "react";
import { Logs, type LogEntry } from "./Logs";
import { Covers } from "./Covers";
import "./App.css";

export default function App() {
  const [theme, setTheme] = useState<"light" | "dark">("dark");
  const [logs, setLogs] = useState<LogEntry[]>([]);
  const [view, setView] = useState<"logs" | "covers">("logs");

  const toggleTheme = () =>
    setTheme((prev) => (prev === "dark" ? "light" : "dark"));

  const mapSSEToLogEntry = (sseData: any, index: number): LogEntry => ({
    id: sseData.time || `log-${index}`,
    level: sseData.level || "info",
    message: sseData.content || "",
  });

  useEffect(() => {
    const evtSource = new EventSource("/log-sse");
    let idx = 0;

    evtSource.addEventListener("message", (e: MessageEvent) => {
      try {
        const data = JSON.parse(e.data);
        const newLog = mapSSEToLogEntry(data, idx++);
        setLogs((prev) => [...prev, newLog]);
      } catch (err) {
        console.error("Erreur parsing SSE:", err);
      }
    });

    return () => {
      evtSource.close();
    };
  }, []);

  return (
    <div className={theme === "dark" ? "dark bg-neutral-900 text-white" : "bg-neutral-50 text-black"}>
      <header className="flex items-center justify-between p-4 bg-neutral-800 text-white">
        <h1 className="text-xl font-bold">PMO-Music</h1>
        <div className="flex gap-2">
          <button
            className="px-4 py-2 bg-blue-600 rounded hover:bg-blue-500"
            onClick={toggleTheme}
          >
            Theme: {theme}
          </button>
          <button
            className={`px-4 py-2 rounded ${
              view === "logs" ? "bg-neutral-700" : "bg-neutral-600"
            }`}
            onClick={() => setView("logs")}
          >
            Logs
          </button>
          <button
            className={`px-4 py-2 rounded ${
              view === "covers" ? "bg-neutral-700" : "bg-neutral-600"
            }`}
            onClick={() => setView("covers")}
          >
            Covers
          </button>
        </div>
      </header>
      {view === "logs" && <Logs logs={logs} theme={theme} />}
      {view === "covers" && <Covers />}
    </div>
  );
}
