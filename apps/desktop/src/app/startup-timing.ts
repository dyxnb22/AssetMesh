import { isTauri } from '@tauri-apps/api/core';
import { useEffect } from 'react';
import { getTransport, type StartupStage } from '../features/library/transport';

const recorded = new Set<StartupStage>();

export function recordStartupStage(stage: StartupStage) {
  if (recorded.has(stage)) return;
  recorded.add(stage);
  performance.mark?.(`assetmesh:${stage}`);
  if (isTauri()) void getTransport().startupTiming(stage, performance.now()).catch(() => {});
}

/** Two frames include the first paint instead of just React's render time. */
export function useStartupStage(stage: StartupStage, ready = true) {
  useEffect(() => {
    if (!ready || recorded.has(stage)) return;
    let second: number | undefined;
    const first = requestAnimationFrame(() => {
      second = requestAnimationFrame(() => recordStartupStage(stage));
    });
    return () => {
      cancelAnimationFrame(first);
      if (second !== undefined) cancelAnimationFrame(second);
    };
  }, [stage, ready]);
}

export function FirstListTiming({ ready }: { ready: boolean }) {
  useStartupStage('first_list_ready', ready);
  return null;
}
