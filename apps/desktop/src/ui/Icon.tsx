import React from 'react';

const paths = {
  play: <path d="m8 5 11 7-11 7V5Z" />,
  stop: <rect x="5" y="5" width="14" height="14" rx="1" />,
  refresh: <><path d="M20 7v5h-5M4 17v-5h5" /><path d="M6 7a7 7 0 0 1 12-1l2 3M18 17a7 7 0 0 1-12 1l-2-3" /></>,
  copy: <><rect x="8" y="8" width="12" height="13" rx="2" /><path d="M16 8V3H3v13h5" /></>,
  edit: <><path d="m14 4 6 6M4 20l1-6L16 3l5 5-11 11-6 1Z" /></>,
  externalLink: <><path d="M13 4h7v7M20 4 9 15M9 4H4v16h16v-5" /></>,
  sidebar: <><rect x="3" y="4" width="18" height="16" rx="3" /><path d="M9 4v16M6 8v4" /></>,
  inspector: <><rect x="3" y="4" width="18" height="16" rx="3" /><path d="M15 4v16M18 8v4" /></>,
  search: <><circle cx="10.5" cy="10.5" r="6.5" /><path d="m16 16 5 5" /></>,
  plus: <path d="M12 4v16M4 12h16" />,
  close: <path d="m6 6 12 12M6 18 18 6" />,
  more: <><circle cx="5" cy="12" r=".8" /><circle cx="12" cy="12" r=".8" /><circle cx="19" cy="12" r=".8" /></>,
  filter: <path d="M4 6h16M7 12h10M10 18h4" />,
  check: <path d="m5 12 4 4L19 6" />,
  left: <path d="m14 5-7 7 7 7" />,
  right: <path d="m10 5 7 7-7 7" />,
  library: <><path d="m3 7 9-4 9 4-9 4-9-4ZM5 9v10h14V9M9 14h6" /></>,
  media: <><rect x="3" y="4" width="18" height="16" rx="2" /><path d="M7 4v16M17 4v16M3 9h4M3 15h4M17 9h4M17 15h4" /></>,
  game: <><path d="M7 7h10c2 0 3 2 3.5 4l1 6c.4 3-2.5 4-4 1l-2-2h-7l-2 2c-1.5 3-4.4 2-4-1l1-6C4 9 5 7 7 7Z" /><path d="M7 10v5M4.5 12.5h5M16 11h.01M18 14h.01" /></>,
  software: <><rect x="4" y="5" width="16" height="15" rx="3" /><path d="M4 10h16M9 3v4M15 3v4m-5 7-2 2 2 2m4-4 2 2-2 2" /></>,
  services: <><rect x="3" y="5" width="18" height="6" rx="2" /><rect x="3" y="14" width="18" height="6" rx="2" /><path d="M7 8h.01M7 17h.01M11 8h6M11 17h6" /></>,
  subscriptions: <><rect x="5" y="5" width="14" height="16" rx="2" /><path d="M8 3v4M16 3v4M5 10h14M9 14h6M9 17h4" /></>,
  info: <><rect x="5" y="6" width="14" height="15" rx="2" /><path d="M9 6V3h6v3M9 11h6M9 15h6" /></>,
  tools: <><path d="m14 5 5-2a6 6 0 0 1-7 8l-7 9-3-3 9-7a6 6 0 0 1 3-8l-1 4 3 2 3-3" /></>,
  settings: <><path d="m10 3 4 0 .6 3 2.5 1.5 3-.8 2 3.6-2.4 2 .1 2.9 2.1 2-2 3.4-2.9-.9-2.4 1.3-.7 3h-4l-.6-3-2.5-1.4-2.9.9-2-3.4 2.2-2v-2.9l-2.3-2 2-3.5 3 .8L9.4 6 10 3Z" transform="translate(1 -1) scale(.9)" /><circle cx="12" cy="12" r="3" /></>,
  star: <path d="m12 3 2.8 5.8 6.4.9-4.6 4.5 1.1 6.3-5.7-3-5.7 3 1.1-6.3-4.6-4.5 6.4-.9L12 3Z" />,
} satisfies Record<string, React.ReactNode>;

export type IconName = keyof typeof paths;

export function Icon({ name, size = 18 }: { name: IconName; size?: number }) {
  return <svg aria-hidden="true" focusable="false" width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round">{paths[name]}</svg>;
}
