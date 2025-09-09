// Logs.tsx
import ReactMarkdown from "react-markdown";
import rehypeRaw from "rehype-raw";

export type LogEntry = {
  id: string;
  level: "info" | "warn" | "error" | "debug";
  message: string;
};

type LogsProps = {
  logs: LogEntry[];
  theme: "light" | "dark";
};

export function Logs({ logs }: LogsProps) {
  const levelClass = (level: LogEntry["level"]) => {
    switch (level) {
      case "info":
        return "bg-blue-100 text-blue-800 dark:bg-blue-900 dark:text-blue-200";
      case "warn":
        return "bg-yellow-100 text-yellow-800 dark:bg-yellow-900 dark:text-yellow-200";
      case "error":
        return "bg-red-100 text-red-800 dark:bg-red-900 dark:text-red-200";
      case "debug":
        return "bg-gray-100 text-gray-800 dark:bg-gray-900 dark:text-gray-200";
      default:
        return "bg-gray-100 dark:bg-gray-800";
    }
  };

  const decodeMessage = (msg: string) => {
    const txt = document.createElement("textarea");
    txt.innerHTML = msg;
    return txt.value;
  };

  return (
    <div className="p-4 space-y-2">
      {logs.map((log) => (
        <div
          key={log.id}
          className={`rounded-lg shadow p-3 ${levelClass(log.level)}`}
        >
          <ReactMarkdown
            rehypePlugins={[rehypeRaw]}
            children={decodeMessage(log.message)}
            components={{
              code({ className, children, ...props }) {
                return (
                  <code
                    className={`bg-black/20 px-1 py-0.5 rounded ${className || ""}`}
                    {...props}
                  >
                    {children}
                  </code>
                );
              },
            }}
          />
        </div>
      ))}
    </div>
  );
}
