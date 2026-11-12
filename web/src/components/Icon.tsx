// Inline SVG icons (stroke, 24-unit grid): no icon font, no external request.
const PATHS: Record<string, string> = {
  home: "M3 11l9-8 9 8v9a1 1 0 0 1-1 1h-5v-6H9v6H4a1 1 0 0 1-1-1z",
  list: "M8 6h13M8 12h13M8 18h13M3 6h.01M3 12h.01M3 18h.01",
  chart: "M3 3v18h18M7 15l4-5 3 3 5-7",
  bank: "M3 10l9-6 9 6M4 10v9M20 10v9M8 10v9M12 10v9M16 10v9M2 21h20",
  sync: "M21 12a9 9 0 0 1-15.5 6.2M3 12a9 9 0 0 1 15.5-6.2M21 4v5h-5M3 20v-5h5",
  check: "M5 12l5 5L20 7",
  x: "M6 6l12 12M18 6L6 18",
  alert: "M12 9v4M12 17h.01M10.3 3.9L2.3 18a2 2 0 0 0 1.7 3h16a2 2 0 0 0 1.7-3l-8-14.1a2 2 0 0 0-3.4 0z",
  info: "M12 16v-4M12 8h.01M12 22a10 10 0 1 0 0-20 10 10 0 0 0 0 20z",
  search: "M21 21l-4.3-4.3M11 18a7 7 0 1 0 0-14 7 7 0 0 0 0 14z",
  chevronL: "M15 18l-6-6 6-6",
  chevronR: "M9 18l6-6-6-6",
  plus: "M12 5v14M5 12h14",
  tag: "M20.6 13.4l-7.2 7.2a2 2 0 0 1-2.8 0l-8-8V3h9.6l8.4 8.4a2 2 0 0 1 0 2zM7 7h.01",
  external: "M18 13v6a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h6M15 3h6v6M10 14L21 3",
  edit: "M12 20h9M16.5 3.5a2.1 2.1 0 0 1 3 3L7 19l-4 1 1-4z",
  trash: "M3 6h18M8 6V4h8v2M19 6l-1 14H6L5 6M10 11v6M14 11v6",
  sun: "M12 17a5 5 0 1 0 0-10 5 5 0 0 0 0 10zM12 1v2M12 21v2M4.2 4.2l1.4 1.4M18.4 18.4l1.4 1.4M1 12h2M21 12h2M4.2 19.8l1.4-1.4M18.4 5.6l1.4-1.4",
  moon: "M21 12.8A9 9 0 1 1 11.2 3a7 7 0 0 0 9.8 9.8z",
  auto: "M12 22a10 10 0 1 0 0-20 10 10 0 0 0 0 20zM12 2v20",
  table: "M3 5h18v14H3zM3 10h18M3 15h18M9 5v14",
  filter: "M22 3H2l8 9.5V19l4 2v-8.5z",
  calendar: "M3 5h18v16H3zM16 3v4M8 3v4M3 10h18",
  clock: "M12 22a10 10 0 1 0 0-20 10 10 0 0 0 0 20zM12 6v6l4 2",
  spark: "M12 3l1.8 5.2L19 10l-5.2 1.8L12 17l-1.8-5.2L5 10l5.2-1.8z",
};

export function Icon({
  name,
  size = 20,
  className,
  strokeWidth = 2,
}: {
  name: keyof typeof PATHS | string;
  size?: number;
  className?: string;
  strokeWidth?: number;
}) {
  return (
    <svg
      className={className}
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={strokeWidth}
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
      focusable="false"
    >
      <path d={PATHS[name] ?? ""} />
    </svg>
  );
}
