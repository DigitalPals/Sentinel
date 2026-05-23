// Shared SVG charts and number formatters.
import React from "react";

export function Sparkline({
  data,
  width = 96,
  height = 28,
  color = "var(--accent)",
  area = true,
  strokeWidth = 1.4,
}: {
  data: number[];
  width?: number;
  height?: number;
  color?: string;
  area?: boolean;
  strokeWidth?: number;
}) {
  const uid = React.useId();
  if (!data || !data.length) return null;
  const min = Math.min(...data);
  const max = Math.max(...data);
  const range = max - min || 1;
  const pts = data.map((v, i) => {
    const x = data.length === 1 ? width / 2 : (i / (data.length - 1)) * (width - 2) + 1;
    const y = height - 1 - ((v - min) / range) * (height - 2);
    return [x, y] as [number, number];
  });
  const d = pts.map(([x, y], i) => (i ? "L" : "M") + x.toFixed(1) + " " + y.toFixed(1)).join(" ");
  const areaD = d + ` L${width - 1} ${height - 1} L1 ${height - 1} Z`;
  const id = "sg" + uid.replace(/[^a-zA-Z0-9]/g, "");
  return (
    <svg width={width} height={height} viewBox={`0 0 ${width} ${height}`}>
      {area && (
        <>
          <defs>
            <linearGradient id={id} x1="0" x2="0" y1="0" y2="1">
              <stop offset="0%" stopColor={color} stopOpacity="0.4" />
              <stop offset="100%" stopColor={color} stopOpacity="0" />
            </linearGradient>
          </defs>
          <path d={areaD} fill={`url(#${id})`} />
        </>
      )}
      <path d={d} fill="none" stroke={color} strokeWidth={strokeWidth} strokeLinecap="round" strokeLinejoin="round" />
    </svg>
  );
}

export function BandwidthChart({
  down,
  up,
  windowLabel,
  height = 220,
}: {
  down: number[];
  up: number[];
  windowLabel?: string;
  height?: number;
}) {
  const uid = React.useId().replace(/[^a-zA-Z0-9]/g, "");
  const [hover, setHover] = React.useState<number | null>(null);
  const W = 760;
  const H = height;
  const padL = 40;
  const padR = 8;
  const padT = 12;
  const padB = 30;
  const n = down.length;
  const allMax = Math.max(...down, ...up, 1);
  const yMax = niceMax(allMax);
  const xAt = (i: number) => padL + (n <= 1 ? 0.5 : i / (n - 1)) * (W - padL - padR);
  const yAt = (v: number) => H - padB - (v / yMax) * (H - padT - padB);
  const pathFor = (arr: number[]) =>
    arr.map((v, i) => (i ? "L" : "M") + xAt(i).toFixed(1) + " " + yAt(v).toFixed(1)).join(" ");
  const areaFor = (arr: number[]) =>
    pathFor(arr) + ` L${xAt(n - 1).toFixed(1)} ${H - padB} L${padL} ${H - padB} Z`;
  const gridY = [0, 0.25, 0.5, 0.75, 1].map((p) => yMax - p * yMax);
  const xTicks = [0, 0.25, 0.5, 0.75, 1];
  const windowMin = parseWindowMinutes(windowLabel);

  const labelAt = (frac: number): string => {
    if (frac >= 0.999) return "now";
    if (windowMin == null) return frac <= 0.001 ? windowLabel ?? "" : "";
    const ago = windowMin * (1 - frac);
    if (ago < 60) return `-${Math.round(ago)}m`;
    const h = ago / 60;
    return `-${Number.isInteger(h) ? h : h.toFixed(1)}h`;
  };

  const onMove = (e: React.MouseEvent<SVGSVGElement>) => {
    const r = e.currentTarget.getBoundingClientRect();
    if (!r.width || n < 2) return;
    const frac = (((e.clientX - r.left) / r.width) * W - padL) / (W - padL - padR);
    setHover(Math.max(0, Math.min(n - 1, Math.round(frac * (n - 1)))));
  };

  const tipWhen = hover != null ? labelAt(hover / (n - 1)) : "";

  return (
    <div className="bw-chart">
      <svg
        viewBox={`0 0 ${W} ${H}`}
        preserveAspectRatio="none"
        onMouseMove={onMove}
        onMouseLeave={() => setHover(null)}
      >
        <defs>
          <linearGradient id={`bwd-${uid}`} x1="0" x2="0" y1="0" y2="1">
            <stop offset="0%" stopColor="var(--accent)" stopOpacity="0.45" />
            <stop offset="100%" stopColor="var(--accent)" stopOpacity="0" />
          </linearGradient>
          <linearGradient id={`bwu-${uid}`} x1="0" x2="0" y1="0" y2="1">
            <stop offset="0%" stopColor="var(--accent-2)" stopOpacity="0.38" />
            <stop offset="100%" stopColor="var(--accent-2)" stopOpacity="0" />
          </linearGradient>
        </defs>

        {gridY.map((v, i) => (
          <g key={"y" + i}>
            <line
              x1={padL}
              x2={W - padR}
              y1={yAt(v)}
              y2={yAt(v)}
              stroke="var(--line)"
              strokeWidth="1"
              strokeDasharray={i === 4 ? "0" : "2 4"}
            />
            <text x={padL - 6} y={yAt(v) + 3} fontSize="10" textAnchor="end" fill="var(--fg-3)" fontFamily="var(--mono)">
              {fmtAxis(v)}
            </text>
          </g>
        ))}

        {xTicks.map((f, i) => {
          const x = padL + f * (W - padL - padR);
          return (
            <g key={"x" + i}>
              <line
                x1={x}
                x2={x}
                y1={padT}
                y2={H - padB}
                stroke="var(--line)"
                strokeWidth="1"
                strokeDasharray="2 4"
                opacity="0.45"
              />
              <text
                x={x}
                y={H - padB + 16}
                fontSize="10"
                textAnchor={i === 0 ? "start" : i === xTicks.length - 1 ? "end" : "middle"}
                fill="var(--fg-3)"
                fontFamily="var(--mono)"
              >
                {labelAt(f)}
              </text>
            </g>
          );
        })}

        <path d={areaFor(down)} fill={`url(#bwd-${uid})`} />
        <path d={areaFor(up)} fill={`url(#bwu-${uid})`} />
        <path d={pathFor(down)} fill="none" stroke="var(--accent)" strokeWidth="1.6" strokeLinejoin="round" />
        <path d={pathFor(up)} fill="none" stroke="var(--accent-2)" strokeWidth="1.6" strokeLinejoin="round" />

        {hover == null ? (
          <>
            <line
              x1={xAt(n - 1)}
              x2={xAt(n - 1)}
              y1={padT}
              y2={H - padB}
              stroke="var(--accent)"
              strokeWidth="1"
              strokeDasharray="2 3"
              opacity="0.5"
            />
            <circle cx={xAt(n - 1)} cy={yAt(down[n - 1])} r="3.5" fill="var(--accent)" stroke="var(--bg)" strokeWidth="1.5" />
            <circle cx={xAt(n - 1)} cy={yAt(up[n - 1])} r="3.5" fill="var(--accent-2)" stroke="var(--bg)" strokeWidth="1.5" />
          </>
        ) : (
          <>
            <line x1={xAt(hover)} x2={xAt(hover)} y1={padT} y2={H - padB} stroke="var(--fg-2)" strokeWidth="1" />
            <circle cx={xAt(hover)} cy={yAt(down[hover])} r="4" fill="var(--accent)" stroke="var(--bg)" strokeWidth="1.5" />
            <circle cx={xAt(hover)} cy={yAt(up[hover])} r="4" fill="var(--accent-2)" stroke="var(--bg)" strokeWidth="1.5" />
          </>
        )}
      </svg>

      {hover != null && (
        <div
          className="bw-tip"
          style={{ left: `${Math.min(92, Math.max(8, (xAt(hover) / W) * 100))}%` }}
        >
          {tipWhen && <div className="bw-tip-when">{tipWhen}</div>}
          <div className="bw-tip-row">
            <span className="bw-tip-sw" style={{ background: "var(--accent)" }} />↓ {fmtMbps(down[hover])}
          </div>
          <div className="bw-tip-row">
            <span className="bw-tip-sw" style={{ background: "var(--accent-2)" }} />↑ {fmtMbps(up[hover])}
          </div>
        </div>
      )}
    </div>
  );
}

function niceMax(v: number): number {
  if (v <= 10) return Math.ceil(v / 2) * 2 || 2;
  if (v <= 100) return Math.ceil(v / 20) * 20;
  if (v <= 1000) return Math.ceil(v / 200) * 200;
  return Math.ceil(v / 500) * 500;
}

function fmtAxis(v: number): string {
  if (v >= 1000) return (v / 1000).toFixed(1) + "G";
  return String(Math.round(v));
}

function parseWindowMinutes(label?: string): number | null {
  if (!label) return null;
  const h = label.match(/(\d+)\s*h/);
  const m = label.match(/(\d+)\s*m/);
  if (!h && !m) return null;
  return (h ? parseInt(h[1], 10) * 60 : 0) + (m ? parseInt(m[1], 10) : 0);
}

export function fmtMbps(mbps: number | null | undefined): string {
  if (mbps == null) return "—";
  if (mbps >= 1000) return (mbps / 1000).toFixed(2) + " Gbps";
  if (mbps >= 10) return Math.round(mbps) + " Mbps";
  return mbps.toFixed(1) + " Mbps";
}
