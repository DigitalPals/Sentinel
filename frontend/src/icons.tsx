// Shared icon bridge. Product marks come from Simple Icons; UI glyphs come from Lucide.
import React from "react";
import {
  BatteryCharging,
  Bell,
  Camera,
  Check,
  ChevronDown,
  Database,
  Download,
  EthernetPort,
  GripVertical,
  Info,
  LayoutDashboard,
  Logs,
  Menu,
  MoreHorizontal,
  Network,
  PanelsTopLeft,
  Plus,
  Power,
  RefreshCw,
  RotateCcw,
  Router,
  Search,
  Server,
  Settings,
  Trash2,
  TriangleAlert,
  Upload,
  Wifi,
  X,
  type LucideIcon,
} from "lucide-react";
import { siProxmox, siUbiquiti, siUnraid, type SimpleIcon } from "simple-icons";

const UI_ICONS: Record<string, LucideIcon> = {
  dashboard: LayoutDashboard,
  network: Network,
  server: Server,
  storage: Database,
  alert: TriangleAlert,
  info: Info,
  logs: Logs,
  settings: Settings,
  search: Search,
  bell: Bell,
  refresh: RefreshCw,
  chevron: ChevronDown,
  download: Download,
  upload: Upload,
  ap: Wifi,
  switch: EthernetPort,
  gateway: Router,
  router: Router,
  camera: Camera,
  ups: BatteryCharging,
  more: MoreHorizontal,
  layout: PanelsTopLeft,
  check: Check,
  plus: Plus,
  trash: Trash2,
  grip: GripVertical,
  reset: RotateCcw,
  menu: Menu,
  power: Power,
  close: X,
};

const BRAND_ICONS: Record<string, SimpleIcon> = {
  proxmox: siProxmox,
  ubiquiti: siUbiquiti,
  unifi: siUbiquiti,
  unraid: siUnraid,
};

const BRAND_LABELS: Record<string, string> = {
  proxmox: "Proxmox",
  ubiquiti: "Ubiquiti",
  unifi: "UniFi",
  unraid: "Unraid",
};

const DEFAULT_SIZES: Record<string, number> = {
  chevron: 10,
  power: 12,
};

type IconProps = Omit<React.HTMLAttributes<HTMLSpanElement>, "children"> & {
  name: string;
  size?: number;
};

export function Icon({ name, size, title, className, style, ...rest }: IconProps) {
  const brand = BRAND_ICONS[name];
  const resolvedSize = size ?? DEFAULT_SIZES[name] ?? (brand ? 16 : 14);
  const classes = ["icon", brand ? "icon-brand" : "icon-ui", brand ? `icon-brand-${name}` : "", className]
    .filter(Boolean)
    .join(" ");

  if (brand) {
    const label = title || BRAND_LABELS[name] || brand.title;
    return (
      <span
        {...rest}
        className={classes}
        style={{ color: `#${brand.hex}`, ...style }}
        title={title}
        aria-hidden={title ? undefined : true}
      >
        <svg
          viewBox="0 0 24 24"
          width={resolvedSize}
          height={resolvedSize}
          fill="currentColor"
          role={title ? "img" : undefined}
          aria-label={title ? label : undefined}
        >
          {title ? <title>{label}</title> : null}
          <path d={brand.path} />
        </svg>
      </span>
    );
  }

  const Glyph = UI_ICONS[name];
  if (!Glyph) return null;

  return (
    <span {...rest} className={classes} style={style} title={title} aria-hidden={title ? undefined : true}>
      <Glyph size={resolvedSize} strokeWidth={1.8} absoluteStrokeWidth />
    </span>
  );
}
