import { useEffect, useState } from "react";

interface DataFreshnessProps {
  dataUpdatedAt: number; // Unix ms from React Query's dataUpdatedAt
}

export function DataFreshness({ dataUpdatedAt }: DataFreshnessProps) {
  const [now, setNow] = useState(() => Date.now());

  useEffect(() => {
    const id = setInterval(() => setNow(Date.now()), 30_000);
    return () => clearInterval(id);
  }, []);

  useEffect(() => {
    setNow(Date.now());
  }, [dataUpdatedAt]);

  if (dataUpdatedAt === 0) return null;

  const diffSec = Math.max(0, Math.floor((now - dataUpdatedAt) / 1000));
  let label: string;
  if (diffSec < 60) {
    label = `${diffSec}s ago`;
  } else if (diffSec < 3600) {
    label = `${Math.floor(diffSec / 60)}m ago`;
  } else {
    label = `${Math.floor(diffSec / 3600)}h ago`;
  }

  return <span className="text-xs text-[var(--muted)]">Updated {label}</span>;
}
