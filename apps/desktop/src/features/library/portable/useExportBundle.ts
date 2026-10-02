import { useState } from 'react';
import { t } from '../../../i18n';
import { normalizeDesktopError } from '../transport';
import type { DesktopTransport } from '../transport';
import type { ExportReceipt } from '../types';

export function useExportBundle(transport: DesktopTransport) {
  // Export State
  const [includeApiKeys, setIncludeApiKeys] = useState(false);
  const [exportDir, setExportDir] = useState<string>('');
  const [isExporting, setIsExporting] = useState<boolean>(false);
  const [exportReceipt, setExportReceipt] = useState<ExportReceipt | null>(null);
  const [exportError, setExportError] = useState<string | null>(null);

  // Browse Directory for Export
  const handleBrowseExport = async () => {
    try {
      const picked = await transport.pickDirectory(t('Select Directory to Export Bundle'));
      if (picked !== null) {
        setExportDir(picked);
      }
    } catch (err) {
      setExportError(normalizeDesktopError(err).message);
    }
  };

  // Run Export
  const handleExecuteExport = async () => {
    if (!exportDir.trim()) {
      setExportError('Please specify a target destination directory.');
      return;
    }
    setIsExporting(true);
    setExportError(null);
    try {
      const receipt = await transport.portableExport(exportDir.trim(), includeApiKeys);
      setExportReceipt(receipt);
    } catch (err) {
      setExportError(normalizeDesktopError(err).message);
    } finally {
      setIsExporting(false);
    }
  };

  return {
    includeApiKeys, setIncludeApiKeys, exportDir, setExportDir, isExporting, exportReceipt,
    exportError, handleBrowseExport, handleExecuteExport
  };
}
