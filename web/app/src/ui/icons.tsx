// The design's icons: 24-unit strokes drawn in the current colour, hidden from screen readers
// because the text beside each one says what it means.

import type { ReactNode } from "react";

interface IconProps {
  size?: number;
  stroke?: number;
  className?: string;
}

function icon(paths: ReactNode, defaultStroke = 2) {
  return function Icon({ size = 14, stroke = defaultStroke, className }: IconProps) {
    return (
      <svg
        aria-hidden="true"
        className={className}
        width={size}
        height={size}
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        strokeWidth={stroke}
        strokeLinecap="round"
        strokeLinejoin="round"
      >
        {paths}
      </svg>
    );
  };
}

export const ChevronDown = icon(<path d="M6 9l6 6 6-6" />);
export const ChevronLeft = icon(<path d="M15 6l-6 6 6 6" />);
export const ChevronRight = icon(<path d="M9 6l6 6-6 6" />);
export const Check = icon(<path d="M5 12.5l4.5 4.5L19 7.5" />);
export const CheckCircle = icon(
  <>
    <circle cx="12" cy="12" r="9" />
    <path d="M8 12.5l2.5 2.5L16 9.5" />
  </>,
);
export const Warning = icon(
  <>
    <path d="M12 4l9 16H3z" />
    <path d="M12 10v4" />
    <path d="M12 17h.01" />
  </>,
);
export const ErrorCircle = icon(
  <>
    <circle cx="12" cy="12" r="9" />
    <path d="M12 7.5v5.5" />
    <path d="M12 16.5h.01" />
  </>,
  1.75,
);
export const Clock = icon(
  <>
    <circle cx="12" cy="12" r="9" />
    <path d="M12 7v5l3 2" />
  </>,
);
export const Lock = icon(
  <>
    <rect x="5" y="11" width="14" height="9" rx="2" />
    <path d="M8 11V8a4 4 0 0 1 8 0v3" />
  </>,
  1.75,
);
export const Download = icon(
  <>
    <path d="M12 4v11" />
    <path d="M7 10l5 5 5-5" />
    <path d="M5 20h14" />
  </>,
  1.75,
);
export const Folder = icon(
  <path d="M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z" />,
  1.75,
);
export const EmptyFolder = icon(
  <>
    <path d="M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z" />
    <path d="M10 13h4" />
  </>,
  1.5,
);
export const Copy = icon(
  <>
    <rect x="8" y="8" width="12" height="12" rx="2" />
    <path d="M16 8V6a2 2 0 0 0-2-2H6a2 2 0 0 0-2 2v8a2 2 0 0 0 2 2h2" />
  </>,
  1.75,
);
export const TableIcon = icon(
  <>
    <rect x="3" y="4" width="18" height="16" rx="2" />
    <path d="M3 10h18M9 10v10" />
  </>,
  1.75,
);
export const ChartIcon = icon(
  <>
    <path d="M4 20V10M10 20V4M16 20v-7M22 20H2" />
  </>,
  1.75,
);
export const Search = icon(
  <>
    <circle cx="11" cy="11" r="6" />
    <path d="M20 20l-4.5-4.5" />
  </>,
);
export const Plus = icon(<path d="M12 5v14M5 12h14" />);
export const Close = icon(<path d="M6 6l12 12M18 6L6 18" />);
/** Three quarters of a circle, which turns while something is under way. */
export const Spinner = icon(<path d="M12 3a9 9 0 1 0 9 9" />, 2.5);

/** The desktop sidebar's icons for the sections, drawn at 16 units with a 1.75 stroke. */
export const OverviewIcon = icon(
  <>
    <rect x="3" y="3" width="7" height="7" rx="1.5" />
    <rect x="14" y="3" width="7" height="7" rx="1.5" />
    <rect x="3" y="14" width="7" height="7" rx="1.5" />
    <rect x="14" y="14" width="7" height="7" rx="1.5" />
  </>,
  1.75,
);
export const SessionsIcon = icon(
  <>
    <path d="M8 6h13M8 12h13M8 18h13" />
    <path d="M3.5 6h.01M3.5 12h.01M3.5 18h.01" />
  </>,
  1.75,
);
export const ProjectsIcon = icon(
  <path d="M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z" />,
  1.75,
);
export const ModelsIcon = icon(
  <>
    <path d="M12 3l8 4.5v9L12 21l-8-4.5v-9z" />
    <path d="M12 12l8-4.5M12 12L4 7.5M12 12v9" />
  </>,
  1.75,
);
export const LimitsIcon = icon(
  <>
    <path d="M4 16a8 8 0 1 1 16 0" />
    <path d="M12 16l4-5" />
  </>,
  1.75,
);
export const SettingsIcon = icon(
  <>
    <path d="M4 6h9M17 6h3M4 12h3M11 12h9M4 18h11M19 18h1" />
    <circle cx="15" cy="6" r="2" />
    <circle cx="9" cy="12" r="2" />
    <circle cx="17" cy="18" r="2" />
  </>,
  1.75,
);
