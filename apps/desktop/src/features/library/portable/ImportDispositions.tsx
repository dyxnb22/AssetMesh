import { t } from '../../../i18n';
import type { ImportReport } from '../types';

export function ImportDispositions({ report }: { report: ImportReport }) {
  const rows = [
    ['Assets', report.assets_created, report.assets_updated],
    ['Media', report.media_created, report.media_updated],
    ['Software', report.software_created, report.software_updated],
    ['Services', report.services_created, report.services_updated],
    ['Relations', report.relations_created, report.relations_updated],
    ['Information', report.info_created, report.info_updated],
  ] as const;
  return <table data-testid="import-dispositions-table" className="portable-dispositions">
    <thead><tr><th>{t('Entity Type')}</th><th>{t('To Be Created')}</th><th>{t('To Be Updated')}</th></tr></thead>
    <tbody>{rows.map(([label, created, updated]) => <tr key={label}>
      <td>{t(label)}</td><td>+{created}</td><td>{updated}</td>
    </tr>)}</tbody>
  </table>;
}
