// Cybex Sentinel — network topology tree + full-view modal.
import React from "react";
import type { TopoNode } from "./api";
import { Card, Chip, Icon } from "./components";

const DEVICE_GLYPH: Record<string, React.ReactElement> = {
  router: (
    <svg viewBox="0 0 16 16" width="13" height="13" fill="none" stroke="currentColor" strokeWidth="1.4">
      <rect x="2" y="5" width="12" height="6" rx="1.5" />
      <path d="M4.5 8h.01M11.5 8h.01" strokeWidth="1.8" strokeLinecap="round" />
      <path d="M5 4l-2-2M11 4l2-2M5 12l-2 2M11 12l2 2" strokeLinecap="round" />
    </svg>
  ),
  agg: (
    <svg viewBox="0 0 16 16" width="13" height="13" fill="none" stroke="currentColor" strokeWidth="1.4">
      <rect x="1.5" y="4" width="13" height="8" rx="1.2" />
      <path
        d="M3.5 7h.01M5 7h.01M6.5 7h.01M8 7h.01M9.5 7h.01M11 7h.01M12.5 7h.01M3.5 9.5h.01M5 9.5h.01M6.5 9.5h.01M8 9.5h.01M9.5 9.5h.01M11 9.5h.01M12.5 9.5h.01"
        strokeWidth="1.6"
        strokeLinecap="round"
      />
    </svg>
  ),
  sw: (
    <svg viewBox="0 0 16 16" width="13" height="13" fill="none" stroke="currentColor" strokeWidth="1.4">
      <rect x="2" y="5" width="12" height="6" rx="1" />
      <path d="M4 8h.01M5.5 8h.01M7 8h.01M8.5 8h.01M10 8h.01M11.5 8h.01" strokeWidth="1.8" strokeLinecap="round" />
    </svg>
  ),
  ap: (
    <svg viewBox="0 0 16 16" width="13" height="13" fill="none" stroke="currentColor" strokeWidth="1.4">
      <path d="M3 6a7 7 0 0 1 10 0M5 8a4.5 4.5 0 0 1 6 0M7 10a2 2 0 0 1 2 0" strokeLinecap="round" />
      <circle cx="8" cy="12.5" r="1" fill="currentColor" stroke="none" />
    </svg>
  ),
};

interface TopoCount {
  router: number;
  sw: number;
  ap: number;
  ok: number;
  warn: number;
  crit: number;
  total: number;
}

function countTree(root: TopoNode): TopoCount {
  const c: TopoCount = { router: 0, sw: 0, ap: 0, ok: 0, warn: 0, crit: 0, total: 0 };
  const walk = (n: TopoNode) => {
    if (n.kind === "router") c.router++;
    else if (n.kind === "ap") c.ap++;
    else c.sw++;
    if (n.status === "ok") c.ok++;
    else if (n.status === "warn") c.warn++;
    else c.crit++;
    c.total++;
    (n.children || []).forEach(walk);
  };
  if (root && root.id) walk(root);
  return c;
}

const matchesQuery = (n: TopoNode, q: string) =>
  !q || (n.name + " " + (n.ip || "") + " " + (n.model || "")).toLowerCase().includes(q.toLowerCase());

const subtreeMatches = (n: TopoNode, q: string): boolean =>
  matchesQuery(n, q) || (n.children || []).some((c) => subtreeMatches(c, q));

// ── Recursive tree row ───────────────────────────────────────
function TreeRow({
  node,
  depth = 0,
  expanded,
  onToggle,
  forceOpen = false,
  query = "",
  compact = false,
}: {
  node: TopoNode;
  depth?: number;
  expanded: Record<string, boolean>;
  onToggle: (id: string) => void;
  forceOpen?: boolean;
  query?: string;
  compact?: boolean;
}) {
  const kind = node.kind;
  const hasChildren = (node.children || []).length > 0;
  const open = forceOpen || expanded[node.id] !== false;

  if (query && !subtreeMatches(node, query)) return null;

  let summary: { sw: number; ap: number } | null = null;
  if (hasChildren) {
    let sw = 0;
    let ap = 0;
    const walk = (n: TopoNode) => {
      (n.children || []).forEach((c) => {
        if (c.kind === "ap") ap++;
        else sw++;
        walk(c);
      });
    };
    walk(node);
    summary = { sw, ap };
  }

  return (
    <>
      <div className={"tree-row tree-d-" + Math.min(depth, 5)}>
        <div className="tree-indent" style={{ width: depth * 22 }}>
          {Array.from({ length: depth }).map((_, i) => (
            <span key={i} className="tree-bar" />
          ))}
        </div>

        <button
          className="tree-twist"
          onClick={() => hasChildren && onToggle(node.id)}
          disabled={!hasChildren}
          aria-expanded={open}
        >
          {hasChildren ? (
            <svg
              viewBox="0 0 12 12"
              width="9"
              height="9"
              fill="none"
              stroke="currentColor"
              strokeWidth="2"
              style={{ transform: open ? "rotate(0deg)" : "rotate(-90deg)", transition: "transform .12s" }}
            >
              <path d="M3 4.5l3 3 3-3" strokeLinecap="round" strokeLinejoin="round" />
            </svg>
          ) : (
            <span className="tree-leaf-dot" />
          )}
        </button>

        <span className={"tree-glyph tree-glyph-" + kind}>{DEVICE_GLYPH[kind] || DEVICE_GLYPH.sw}</span>

        <div className="tree-name-block">
          <span className="tree-name">{node.name}</span>
          {!compact && <span className="tree-model">{node.model}</span>}
        </div>

        {!compact && <span className="tree-ip mono">{node.ip}</span>}

        {!compact && kind === "ap" && (
          <span className="tree-clients mono" title="Wireless clients">
            {node.clients > 0 ? node.clients : "—"} <small style={{ color: "var(--fg-3)" }}>clients</small>
          </span>
        )}
        {!compact && (kind === "sw" || kind === "agg") && node.ports && (
          <span className="tree-clients mono" title="Port activity">
            {node.ports}
          </span>
        )}
        {!compact && kind === "router" && node.wan && <span className="tree-clients mono">{node.wan}</span>}

        {summary && !open && (
          <span className="chip" style={{ marginLeft: 6 }}>
            {summary.sw > 0 && `${summary.sw} sw`}
            {summary.sw > 0 && summary.ap > 0 && " · "}
            {summary.ap > 0 && `${summary.ap} AP`}
          </span>
        )}

        <Chip tone={node.status} dot>
          {compact ? "" : node.status === "ok" ? "Online" : node.status === "warn" ? "Warn" : "Offline"}
        </Chip>
      </div>

      {open &&
        (node.children || []).map((c) => (
          <TreeRow
            key={c.id}
            node={c}
            depth={depth + 1}
            expanded={expanded}
            onToggle={onToggle}
            forceOpen={forceOpen}
            query={query}
            compact={compact}
          />
        ))}
    </>
  );
}

// ── Compact topology card (dashboard) ────────────────────────
export function TopologyCard({ topo, onOpenFull }: { topo: TopoNode; onOpenFull: () => void }) {
  const counts = countTree(topo);
  // Collapse every branch except the root so the card stays compact.
  const [expanded, setExpanded] = React.useState<Record<string, boolean>>(() => {
    const init: Record<string, boolean> = {};
    const walk = (n: TopoNode, depth: number) => {
      if ((n.children || []).length && depth > 0) init[n.id] = false;
      (n.children || []).forEach((c) => walk(c, depth + 1));
    };
    if (topo && topo.id) walk(topo, 0);
    return init;
  });
  const onToggle = (id: string) =>
    setExpanded((e) => ({ ...e, [id]: e[id] === false ? true : false }));

  if (!topo || !topo.id) {
    return (
      <Card title="Network Topology" sub="UniFi" tight>
        <div className="empty-row">Topology unavailable — UniFi source offline.</div>
      </Card>
    );
  }

  return (
    <Card
      title="Network Topology"
      sub={`${topo.name} → ${counts.sw} switches → ${counts.ap} APs`}
      actions={
        <button className="filter-pill" onClick={onOpenFull}>
          <Icon name="network" /> Full topology
        </button>
      }
      tight
    >
      <div className="tree-list">
        <TreeRow node={topo} depth={0} expanded={expanded} onToggle={onToggle} compact />
      </div>
      <div className="topo-foot">
        <span className="mono">{counts.total} devices</span>
        <span className="dot-sep" />
        <Chip dot tone="ok">
          {counts.ok} online
        </Chip>
        <Chip dot tone="warn">
          {counts.warn} warn
        </Chip>
        <Chip dot tone="crit">
          {counts.crit} offline
        </Chip>
        <span style={{ flex: 1 }} />
        <button className="filter-pill" onClick={onOpenFull}>
          View all {counts.total} devices →
        </button>
      </div>
    </Card>
  );
}

// ── Full topology modal ──────────────────────────────────────
export function TopologyModal({ topo, onClose }: { topo: TopoNode; onClose: () => void }) {
  const counts = countTree(topo);
  const [expanded, setExpanded] = React.useState<Record<string, boolean>>({});
  const [query, setQuery] = React.useState("");
  const [statusFilter, setStatusFilter] = React.useState("all");

  const onToggle = (id: string) =>
    setExpanded((e) => ({ ...e, [id]: e[id] === false ? true : false }));

  const allIds = React.useMemo(() => {
    const ids: string[] = [];
    const walk = (n: TopoNode) => {
      if ((n.children || []).length) ids.push(n.id);
      (n.children || []).forEach(walk);
    };
    walk(topo);
    return ids;
  }, [topo]);

  const expandAll = () => setExpanded(Object.fromEntries(allIds.map((id) => [id, true])));
  const collapseAll = () => setExpanded(Object.fromEntries(allIds.map((id) => [id, false])));

  const filtered = React.useMemo(() => {
    if (statusFilter === "all") return topo;
    const prune = (n: TopoNode): TopoNode | null => {
      const children = (n.children || []).map(prune).filter(Boolean) as TopoNode[];
      if (children.length || n.status === statusFilter) return { ...n, children };
      return null;
    };
    return prune(topo) || topo;
  }, [statusFilter, topo]);

  React.useEffect(() => {
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && onClose();
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);

  React.useEffect(() => {
    const prev = document.body.style.overflow;
    document.body.style.overflow = "hidden";
    return () => {
      document.body.style.overflow = prev;
    };
  }, []);

  const exportJson = () => {
    const blob = new Blob([JSON.stringify(topo, null, 2)], { type: "application/json" });
    const url = URL.createObjectURL(blob);
    const a = document.createElement("a");
    a.href = url;
    a.download = "cybex-sentinel-topology.json";
    a.click();
    URL.revokeObjectURL(url);
  };

  return (
    <div className="modal-backdrop" onClick={onClose}>
      <div className="modal" onClick={(e) => e.stopPropagation()} role="dialog" aria-modal="true">
        <div className="modal-hd">
          <div>
            <div className="crumb">Network · UniFi</div>
            <h3 className="modal-title">Full Network Topology</h3>
          </div>
          <div style={{ display: "flex", gap: 8, alignItems: "center", marginLeft: "auto" }}>
            <Chip dot tone="ok">
              {counts.ok} online
            </Chip>
            <Chip dot tone="warn">
              {counts.warn} warn
            </Chip>
            <Chip dot tone="crit">
              {counts.crit} offline
            </Chip>
            <button className="icon-btn" onClick={onClose} title="Close (Esc)">
              <svg viewBox="0 0 16 16" width="14" height="14" fill="none" stroke="currentColor" strokeWidth="1.5">
                <path d="M4 4l8 8M12 4l-8 8" strokeLinecap="round" />
              </svg>
            </button>
          </div>
        </div>

        <div className="filters">
          {[
            ["all", "All", counts.total],
            ["ok", "Online", counts.ok],
            ["warn", "Warning", counts.warn],
            ["crit", "Offline", counts.crit],
          ].map(([key, label, count]) => (
            <button
              key={key as string}
              className={"filter-pill " + (statusFilter === key ? "active" : "")}
              onClick={() => setStatusFilter(key as string)}
            >
              {label} <span className="count">{count}</span>
            </button>
          ))}
          <div className="filters-spacer" />
          <button className="filter-pill" onClick={expandAll}>
            Expand all
          </button>
          <button className="filter-pill" onClick={collapseAll}>
            Collapse all
          </button>
          <div className="search-mini" style={{ width: 260 }}>
            <Icon name="search" />
            <input
              placeholder="Filter by name, model, IP"
              value={query}
              onChange={(e) => setQuery(e.target.value)}
            />
          </div>
        </div>

        <div className="modal-body">
          <div className="tree-list tree-list-modal">
            <TreeRow node={filtered} depth={0} expanded={expanded} onToggle={onToggle} query={query} />
          </div>
        </div>

        <div className="modal-foot">
          <span className="mono" style={{ color: "var(--fg-3)" }}>
            ⎯ Press Esc to close · click outside to dismiss
          </span>
          <span style={{ flex: 1 }} />
          <button className="filter-pill" onClick={exportJson}>
            <Icon name="download" /> Export as JSON
          </button>
        </div>
      </div>
    </div>
  );
}
