import { useEffect, useLayoutEffect, useRef, useState } from 'react';

const KEY = 'assetmesh-view-scroll-v1';

function readPositions(): Record<string, number> {
  try {
    const saved = JSON.parse(localStorage.getItem(KEY) ?? '{}');
    return Object.fromEntries(Object.entries(saved).filter((entry): entry is [string, number] =>
      typeof entry[1] === 'number' && Number.isFinite(entry[1]) && entry[1] >= 0));
  } catch { return {}; }
}

/** Preserve position on read-back and restore each filtered page independently. */
export function useViewScroll(viewKey: string, ready: boolean) {
  const ref = useRef<HTMLDivElement>(null);
  const [initialPositions] = useState(readPositions);
  const positions = useRef(initialPositions);
  useLayoutEffect(() => {
    if (ready && ref.current) ref.current.scrollTop = positions.current[viewKey] ?? 0;
  }, [viewKey, ready]);
  const timer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  useEffect(() => {
    const flush = () => {
      clearTimeout(timer.current);
      try { localStorage.setItem(KEY, JSON.stringify(positions.current)); } catch { /* Scrolling remains available. */ }
    };
    window.addEventListener('pagehide', flush);
    return () => { flush(); window.removeEventListener('pagehide', flush); };
  }, [viewKey]);
  const onScroll = () => {
    if (!ref.current || !ready) return;
    delete positions.current[viewKey];
    positions.current[viewKey] = ref.current.scrollTop;
    const bounded = Object.fromEntries(Object.entries(positions.current).slice(-100));
    positions.current = bounded;
    clearTimeout(timer.current);
    timer.current = setTimeout(() => {
      try { localStorage.setItem(KEY, JSON.stringify(positions.current)); } catch { /* Scrolling remains available. */ }
    }, 300);
  };
  return { ref, onScroll };
}
