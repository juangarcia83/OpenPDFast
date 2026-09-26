// SPDX-License-Identifier: AGPL-3.0-or-later
// Small stroke icons drawn on a 24x24 grid. Decorative by default: the
// control that contains an icon carries the accessible label.

import type { JSX } from "solid-js";

function Icon(props: { children: JSX.Element; size?: number }) {
  return (
    <svg
      class="icon"
      width={props.size ?? 18}
      height={props.size ?? 18}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      stroke-width="1.8"
      stroke-linecap="round"
      stroke-linejoin="round"
      aria-hidden="true"
    >
      {props.children}
    </svg>
  );
}

type P = { size?: number };

export const IconOpen = (p: P) => (
  <Icon size={p.size}>
    <path d="M3 7.5V18a1.5 1.5 0 0 0 1.5 1.5h15A1.5 1.5 0 0 0 21 18V9a1.5 1.5 0 0 0-1.5-1.5H12L10 5H4.5A1.5 1.5 0 0 0 3 6.5z" />
  </Icon>
);

export const IconFile = (p: P) => (
  <Icon size={p.size}>
    <path d="M14 3H7a1.5 1.5 0 0 0-1.5 1.5v15A1.5 1.5 0 0 0 7 21h10a1.5 1.5 0 0 0 1.5-1.5V7.5z" />
    <path d="M14 3v4.5h4.5M9 12.5h6M9 16h6" />
  </Icon>
);

export const IconPrev = (p: P) => (
  <Icon size={p.size}>
    <path d="M15 6l-6 6 6 6" />
  </Icon>
);

export const IconNext = (p: P) => (
  <Icon size={p.size}>
    <path d="M9 6l6 6-6 6" />
  </Icon>
);

export const IconMinus = (p: P) => (
  <Icon size={p.size}>
    <path d="M6 12h12" />
  </Icon>
);

export const IconPlus = (p: P) => (
  <Icon size={p.size}>
    <path d="M12 6v12M6 12h12" />
  </Icon>
);

export const IconFitWidth = (p: P) => (
  <Icon size={p.size}>
    <rect x="3" y="5" width="18" height="14" rx="1.5" />
    <path d="M7 12h10M9.5 9.5L7 12l2.5 2.5M14.5 9.5L17 12l-2.5 2.5" />
  </Icon>
);

export const IconFitPage = (p: P) => (
  <Icon size={p.size}>
    <rect x="6" y="3" width="12" height="18" rx="1.5" />
    <path d="M12 7v10M9.5 9.5L12 7l2.5 2.5M9.5 14.5L12 17l2.5-2.5" />
  </Icon>
);

export const IconSun = (p: P) => (
  <Icon size={p.size}>
    <circle cx="12" cy="12" r="4" />
    <path d="M12 2.5v2M12 19.5v2M4.6 4.6L6 6M18 18l1.4 1.4M2.5 12h2M19.5 12h2M4.6 19.4L6 18M18 6l1.4-1.4" />
  </Icon>
);

export const IconMoon = (p: P) => (
  <Icon size={p.size}>
    <path d="M20 14.5A8 8 0 0 1 9.5 4a8 8 0 1 0 10.5 10.5z" />
  </Icon>
);

export const IconSystem = (p: P) => (
  <Icon size={p.size}>
    <rect x="3" y="4" width="18" height="12" rx="1.5" />
    <path d="M9 20h6M12 16v4" />
  </Icon>
);

export const IconKeyboard = (p: P) => (
  <Icon size={p.size}>
    <rect x="2.5" y="6" width="19" height="12" rx="1.5" />
    <path d="M6.5 10h.01M10 10h.01M13.5 10h.01M17 10h.01M7.5 14h9" />
  </Icon>
);

export const IconInfo = (p: P) => (
  <Icon size={p.size}>
    <circle cx="12" cy="12" r="9" />
    <path d="M12 11v5.5M12 7.5h.01" />
  </Icon>
);

export const IconClose = (p: P) => (
  <Icon size={p.size}>
    <path d="M6 6l12 12M18 6L6 18" />
  </Icon>
);

export const IconAlert = (p: P) => (
  <Icon size={p.size}>
    <path d="M12 3.5L2.5 20h19z" />
    <path d="M12 10v4.5M12 17.5h.01" />
  </Icon>
);

export const IconExternal = (p: P) => (
  <Icon size={p.size}>
    <path d="M14 4h6v6M20 4l-9 9M18 14v4.5a1.5 1.5 0 0 1-1.5 1.5h-11A1.5 1.5 0 0 1 4 18.5v-11A1.5 1.5 0 0 1 5.5 6H10" />
  </Icon>
);
