import '@testing-library/jest-dom';
import { setLang } from '../i18n';

// The app ships Chinese by default; these suites were written against the
// English copy, so pin the locale instead of rewriting 160+ text queries.
// `src/i18n/i18n.test.tsx` covers the Chinese rendering and the switch itself.
setLang('en');
