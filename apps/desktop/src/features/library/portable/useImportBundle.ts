import { useState } from 'react';
import { t } from '../../../i18n';
import { normalizeDesktopError } from '../transport';
import type { DesktopTransport } from '../transport';
import type { ImportPreview, ImportReceipt } from '../types';

export function useImportBundle(transport: DesktopTransport) {
  // Import State
  const [importDir, setImportDir] = useState<string>('');
  const [isPreviewing, setIsPreviewing] = useState<boolean>(false);
  const [importPreview, setImportPreview] = useState<ImportPreview | null>(null);
  // What the last preview asked the backend to inspect. Tracked separately from
  // `importPreview.source_dir` — that is the backend's own report and may
  // legitimately differ (it resolves and normalizes the path), so judging
  // staleness against the request rather than the report avoids a false
  // "stale" on a path the backend merely cleaned up.
  const [previewedRequest, setPreviewedRequest] = useState<string | null>(null);
  const [importError, setImportError] = useState<string | null>(null);
  const [isApplying, setIsApplying] = useState<boolean>(false);
  const [importReceipt, setImportReceipt] = useState<ImportReceipt | null>(null);

  // Browse Directory for Import
  const handleBrowseImport = async () => {
    try {
      const picked = await transport.pickDirectory(t('Select Portable Bundle Directory to Import'));
      if (picked !== null) {
        setImportDir(picked);
        // Reset previous preview / receipt
        setImportPreview(null);
        setPreviewedRequest(null);
        setImportReceipt(null);
        setImportError(null);
        await handlePreviewImport(picked);
      }
    } catch (err) {
      setImportError(normalizeDesktopError(err).message);
    }
  };

  // Run Import Preflight Preview
  const handlePreviewImport = async (directory = importDir) => {
    const source = directory.trim();
    if (!source) {
      setImportError('Please specify the source bundle directory.');
      return;
    }
    setIsPreviewing(true);
    setImportError(null);
    setImportReceipt(null);
    try {
      const preview = await transport.portableImportPreview(source);
      setImportPreview(preview);
      setPreviewedRequest(source);
      if (!preview.valid && preview.errors.length > 0) {
        setImportError(preview.errors.join('; '));
      }
    } catch (err) {
      setImportPreview(null);
      setPreviewedRequest(null);
      setImportError(normalizeDesktopError(err).message);
    } finally {
      setIsPreviewing(false);
    }
  };

  // A preview describes one directory. If the field no longer matches the
  // request that produced it, the confirmation on screen refers to a bundle
  // nobody inspected, so the apply must be refused rather than run against
  // whatever happens to be in the input.
  const previewIsStale =
    importPreview !== null && previewedRequest !== importDir.trim();

  // Run Explicit Apply
  const handleApplyImport = async () => {
    if (!importPreview || !importPreview.valid || isApplying || isPreviewing || previewIsStale) {
      return;
    }
    setIsApplying(true);
    setImportError(null);
    try {
      // Apply the directory the backend inspected, not whatever the field says
      // now — `source_dir` is the backend's own report.
      const receipt = await transport.portableImportApply(importPreview.source_dir, importPreview.fingerprint);
      setImportReceipt(receipt);
    } catch (err) {
      setImportError(normalizeDesktopError(err).message);
    } finally {
      setIsApplying(false);
    }
  };

  const changeDirectory = (value: string) => {
    setImportDir(value);
    // A receipt belongs to the already-applied source, not the new draft path.
    setImportReceipt(null);
  };
  return {
    importDir, changeDirectory, isPreviewing, importPreview, importError, isApplying,
    importReceipt, previewedRequest, previewIsStale, handleBrowseImport, handlePreviewImport, handleApplyImport
  };
}
