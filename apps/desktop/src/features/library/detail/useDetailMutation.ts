import { useEffect, useRef, useState } from 'react';
import { normalizeDesktopError } from '../transport';
import type { DesktopTransport } from '../transport';
import type { AssetDetailDto, DesktopError, MutationReceiptDto } from '../types';

type Notice = string | ((receipt: MutationReceiptDto) => string);

/** A save completes only after canonical read-back; failed saves keep their draft. */
export function useDetailMutation(
  assetId: string,
  transport: DesktopTransport,
  onSaved: (receipt: MutationReceiptDto, fresh: AssetDetailDto) => void,
) {
  const busy = useRef(false);
  const mounted = useRef(true);
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<DesktopError | null>(null);
  const [notice, setNotice] = useState<string | null>(null);

  useEffect(() => {
    mounted.current = true;
    return () => { mounted.current = false; };
  }, []);

  const clear = () => { setError(null); setNotice(null); };
  const run = async (command: () => Promise<MutationReceiptDto>, success: Notice) => {
    if (busy.current) throw { category: 'unavailable', message: 'Saving...' };
    busy.current = true;
    setPending(true);
    clear();
    try {
      const receipt = await command();
      const fresh = await transport.getAsset(assetId);
      // The library must still refresh if the detail was closed during the save.
      onSaved(receipt, fresh);
      if (mounted.current) setNotice(typeof success === 'function' ? success(receipt) : success);
    } catch (reason: unknown) {
      if (mounted.current) setError(normalizeDesktopError(reason));
      throw reason;
    } finally {
      busy.current = false;
      if (mounted.current) setPending(false);
    }
  };

  return { pending, error, notice, run, clear, fail: setError };
}

export type DetailMutation = ReturnType<typeof useDetailMutation>;
