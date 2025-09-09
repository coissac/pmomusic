// Covers.tsx
import { useEffect, useState } from "react";

type CoverEntry = {
  pk: string;
  source_url: string;
  hits: number;
};

export function Covers() {
  const [covers, setCovers] = useState<CoverEntry[]>([]);

  useEffect(() => {
    fetch("/covers/stats")
      .then((res) => res.json())
      .then((data) => {
        setCovers(data.top_hits || []);
      })
      .catch((err) => console.error("Erreur stats covers:", err));
  }, []);

  return (
    <div className="p-4">
      <h2 className="text-xl font-bold mb-4 text-neutral-900 dark:text-neutral-100">
        Top Covers
      </h2>
      <div className="grid grid-cols-2 sm:grid-cols-3 md:grid-cols-4 lg:grid-cols-6 gap-6">
        {covers.map((cover) => (
          <div
            key={cover.pk}
            className="bg-white dark:bg-neutral-800 rounded-xl shadow p-3 flex flex-col items-center"
          >
            <img
              src={`/covers/${cover.pk}/256`}
              alt={cover.source_url}
              className="w-32 h-32 object-cover rounded-lg shadow"
            />
            <p className="mt-2 text-sm text-neutral-600 dark:text-neutral-300 truncate w-32 text-center">
              {cover.hits} hits
            </p>
          </div>
        ))}
      </div>
    </div>
  );
}
