// Editable dashboard/page layout primitives.
import React from "react";
import type {
  EditableLayoutItem,
  EditableLayoutStore,
  EditableLayoutValue,
} from "./api-types";
import { Icon } from "./components";

export type LayoutStore = EditableLayoutStore;
export type { EditableLayoutValue };

const VERSION = 2;
const LEGACY_VERSION = 1;
const COLUMNS = 12;
const ROW_HEIGHT = 92;
const GRID_GAP = 14;

export interface EditableCardDefinition {
  id: string;
  label: string;
  description?: string;
  defaultSize: { w: number; h: number };
  minW?: number;
  maxW?: number;
  minH?: number;
  maxH?: number;
  defaultHidden?: boolean;
  content: React.ReactNode;
}

interface EditableGridProps {
  pageId: string;
  editMode: boolean;
  items: EditableCardDefinition[];
  layoutStore?: LayoutStore;
  onLayoutChange: (pageId: string, layout: EditableLayoutValue) => void;
}

function storageKey(pageId: string): string {
  return `sentinel.layout.${VERSION}.${pageId}`;
}

function legacyStorageKey(pageId: string): string {
  return `sentinel.layout.${LEGACY_VERSION}.${pageId}`;
}

function loadLocalLayout(pageId: string): EditableLayoutValue | undefined {
  try {
    const raw =
      window.localStorage.getItem(storageKey(pageId)) ??
      window.localStorage.getItem(legacyStorageKey(pageId));
    return raw ? JSON.parse(raw) : undefined;
  } catch {
    return undefined;
  }
}

function saveLocalLayout(pageId: string, layout: EditableLayoutValue): void {
  try {
    window.localStorage.setItem(storageKey(pageId), JSON.stringify(layout));
  } catch {
    /* server-side settings remain the source of truth when localStorage fails */
  }
}

function clamp(n: number, min: number, max: number): number {
  return Math.max(min, Math.min(max, n));
}

function hasGridPosition(item: Partial<EditableLayoutItem>): boolean {
  return Number.isFinite(item.x) && Number.isFinite(item.y);
}

function clampItem(item: EditableLayoutItem, def: EditableCardDefinition): EditableLayoutItem {
  const minW = def.minW ?? 2;
  const maxW = def.maxW ?? COLUMNS;
  const minH = def.minH ?? 1;
  const maxH = def.maxH ?? 12;
  const w = clamp(Math.round(item.w || def.defaultSize.w), minW, maxW);
  const h = clamp(Math.round(item.h || def.defaultSize.h), minH, maxH);
  return {
    id: def.id,
    x: clamp(Math.round(Number.isFinite(item.x) ? item.x : 0), 0, COLUMNS - w),
    y: Math.max(0, Math.round(Number.isFinite(item.y) ? item.y : 0)),
    w,
    h,
    hidden: !!item.hidden,
  };
}

function defaultItem(def: EditableCardDefinition): EditableLayoutItem {
  return clampItem(
    {
      id: def.id,
      x: 0,
      y: 0,
      w: def.defaultSize.w,
      h: def.defaultSize.h,
      hidden: !!def.defaultHidden,
    },
    def,
  );
}

function compactVisibleItems(items: EditableLayoutItem[]): EditableLayoutItem[] {
  const visible = items
    .filter((item) => !item.hidden)
    .sort((a, b) => a.y - b.y || a.x - b.x || a.id.localeCompare(b.id));
  const placed: EditableLayoutItem[] = [];
  const compactedById = new Map<string, EditableLayoutItem>();

  for (const item of visible) {
    let compacted = item;
    while (compacted.y > 0) {
      const candidate = { ...compacted, y: compacted.y - 1 };
      if (collides(candidate, placed)) break;
      compacted = candidate;
    }
    placed.push(compacted);
    compactedById.set(item.id, compacted);
  }

  return items.map((item) => compactedById.get(item.id) ?? item);
}

function overlaps(a: EditableLayoutItem, b: EditableLayoutItem): boolean {
  return (
    a.x < b.x + b.w &&
    a.x + a.w > b.x &&
    a.y < b.y + b.h &&
    a.y + a.h > b.y
  );
}

function collides(
  item: EditableLayoutItem,
  items: EditableLayoutItem[],
  ignoreId?: string,
): boolean {
  if (item.hidden) return false;
  return items.some(
    (other) => !other.hidden && other.id !== ignoreId && overlaps(item, other),
  );
}

function layoutBottom(items: EditableLayoutItem[]): number {
  return items.reduce((max, item) => item.hidden ? max : Math.max(max, item.y + item.h), 0);
}

function firstFreeSpot(
  item: EditableLayoutItem,
  placed: EditableLayoutItem[],
): EditableLayoutItem {
  const scanBottom = layoutBottom(placed) + 12;
  for (let y = 0; y <= scanBottom; y++) {
    for (let x = 0; x <= COLUMNS - item.w; x++) {
      const candidate = { ...item, x, y };
      if (!collides(candidate, placed)) return candidate;
    }
  }
  return { ...item, x: 0, y: layoutBottom(placed) };
}

function layoutWithResolvedCollisions(
  layout: EditableLayoutValue,
  id: string,
  candidate: EditableLayoutItem,
): EditableLayoutValue {
  const replacements = new Map<string, EditableLayoutItem>([[id, candidate]]);
  const placed: EditableLayoutItem[] = candidate.hidden ? [] : [candidate];

  for (const item of layout.items) {
    if (item.id === id) continue;
    let next = item;
    if (!next.hidden) {
      if (collides(next, placed)) next = firstFreeSpot(next, placed);
      placed.push(next);
    }
    replacements.set(item.id, next);
  }

  return {
    version: VERSION,
    items: layout.items.map((item) => replacements.get(item.id) ?? item),
    updatedAt: new Date().toISOString(),
  };
}

function normalizeLayout(
  definitions: EditableCardDefinition[],
  saved?: EditableLayoutValue,
): EditableLayoutValue {
  const defsById = new Map(definitions.map((def) => [def.id, def]));
  const used = new Set<string>();
  const items: EditableLayoutItem[] = [];
  let shouldCompact = false;

  for (const item of saved?.items ?? []) {
    const def = defsById.get(item.id);
    if (!def || used.has(def.id)) continue;
    const hadPosition = hasGridPosition(item);
    let normalized = clampItem(item, def);
    if (!normalized.hidden && Math.round(item.h || def.defaultSize.h) > normalized.h) {
      shouldCompact = true;
    }
    if (!normalized.hidden && (!hadPosition || collides(normalized, items))) {
      normalized = firstFreeSpot(normalized, items);
    }
    items.push(normalized);
    used.add(def.id);
  }

  for (const def of definitions) {
    if (!used.has(def.id)) {
      const item = defaultItem(def);
      items.push(item.hidden ? item : firstFreeSpot(item, items));
    }
  }

  return {
    version: VERSION,
    items: shouldCompact ? compactVisibleItems(items) : items,
    updatedAt: saved?.updatedAt,
  };
}

function defaultLayout(definitions: EditableCardDefinition[]): EditableLayoutValue {
  return {
    version: VERSION,
    items: normalizeLayout(definitions).items,
    updatedAt: new Date().toISOString(),
  };
}

function updateItem(
  layout: EditableLayoutValue,
  id: string,
  update: (item: EditableLayoutItem) => EditableLayoutItem,
): EditableLayoutValue {
  return {
    version: VERSION,
    items: layout.items.map((item) => (item.id === id ? update(item) : item)),
    updatedAt: new Date().toISOString(),
  };
}

export function EditableGrid({
  pageId,
  editMode,
  items,
  layoutStore,
  onLayoutChange,
}: EditableGridProps) {
  const definitions = React.useMemo(() => items, [items]);
  const defsById = React.useMemo(
    () => new Map(definitions.map((def) => [def.id, def])),
    [definitions],
  );
  const definitionKey = React.useMemo(
    () => definitions.map((def) => `${def.id}:${def.defaultSize.w}x${def.defaultSize.h}`).join("|"),
    [definitions],
  );
  const savedLayout = layoutStore?.[pageId] ?? loadLocalLayout(pageId);
  const savedKey = JSON.stringify(savedLayout ?? null);
  const [layout, setLayout] = React.useState(() => normalizeLayout(definitions, savedLayout));
  const layoutRef = React.useRef(layout);
  const gridRef = React.useRef<HTMLDivElement | null>(null);
  const [dragging, setDragging] = React.useState<string | null>(null);

  const setWorkingLayout = React.useCallback((next: EditableLayoutValue) => {
    layoutRef.current = next;
    setLayout(next);
  }, []);

  const persist = React.useCallback(
    (next: EditableLayoutValue) => {
      const normalized = {
        ...normalizeLayout(definitions, next),
        updatedAt: new Date().toISOString(),
      };
      setWorkingLayout(normalized);
      saveLocalLayout(pageId, normalized);
      onLayoutChange(pageId, normalized);
    },
    [definitions, onLayoutChange, pageId, setWorkingLayout],
  );

  React.useEffect(() => {
    const next = normalizeLayout(definitions, savedLayout);
    layoutRef.current = next;
    setLayout(next);
  }, [definitionKey, savedKey]);

  const visible = layout.items.filter((item) => !item.hidden && defsById.has(item.id));
  const hidden = layout.items.filter((item) => item.hidden && defsById.has(item.id));

  const hideCard = (id: string) => {
    persist(updateItem(layoutRef.current, id, (item) => ({ ...item, hidden: true })));
  };

  const showCard = (id: string) => {
    const items = layoutRef.current.items.filter((item) => item.id !== id);
    const current = layoutRef.current.items.find((x) => x.id === id);
    if (!current) return;
    let item: EditableLayoutItem = { ...current, hidden: false };
    if (collides(item, items)) item = firstFreeSpot(item, items);
    persist({
      version: VERSION,
      items: [...items, item],
      updatedAt: new Date().toISOString(),
    });
  };

  const reset = () => persist(defaultLayout(definitions));

  const gridMetrics = () => {
    const grid = gridRef.current;
    if (!grid) return null;
    const rect = grid.getBoundingClientRect();
    const colWidth = (rect.width - GRID_GAP * (COLUMNS - 1)) / COLUMNS;
    return {
      colStep: Math.max(1, colWidth + GRID_GAP),
      rowStep: ROW_HEIGHT + GRID_GAP,
    };
  };

  const startMove = (e: React.PointerEvent, id: string) => {
    const metrics = gridMetrics();
    const item = layoutRef.current.items.find((x) => x.id === id);
    if (!metrics || !item) return;
    e.preventDefault();
    e.stopPropagation();
    setDragging(id);
    const handle = e.currentTarget as HTMLElement;
    try {
      handle.setPointerCapture(e.pointerId);
    } catch {
      /* pointer capture is best-effort */
    }
    const startX = e.clientX;
    const startY = e.clientY;
    const startGridX = item.x;
    const startGridY = item.y;

    const onMove = (move: PointerEvent) => {
      const current = layoutRef.current.items.find((x) => x.id === id);
      if (!current) return;
      const x = clamp(
        startGridX + Math.round((move.clientX - startX) / metrics.colStep),
        0,
        COLUMNS - current.w,
      );
      const y = Math.max(
        0,
        startGridY + Math.round((move.clientY - startY) / metrics.rowStep),
      );
      const candidate = { ...current, x, y };
      if (collides(candidate, layoutRef.current.items, id)) return;
      setWorkingLayout(updateItem(layoutRef.current, id, () => candidate));
    };
    const onUp = () => {
      window.removeEventListener("pointermove", onMove);
      window.removeEventListener("pointerup", onUp);
      window.removeEventListener("pointercancel", onUp);
      persist(layoutRef.current);
      setDragging(null);
    };
    window.addEventListener("pointermove", onMove);
    window.addEventListener("pointerup", onUp);
    window.addEventListener("pointercancel", onUp);
  };

  const startResize = (e: React.PointerEvent, id: string) => {
    const def = defsById.get(id);
    const metrics = gridMetrics();
    const item = layoutRef.current.items.find((x) => x.id === id);
    if (!def || !metrics || !item) return;
    e.preventDefault();
    e.stopPropagation();
    setDragging(id);
    const handle = e.currentTarget as HTMLElement;
    try {
      handle.setPointerCapture(e.pointerId);
    } catch {
      /* pointer capture is best-effort */
    }
    const startX = e.clientX;
    const startY = e.clientY;
    const startW = item.w;
    const startH = item.h;
    const minW = def.minW ?? 2;
    const maxW = Math.min(def.maxW ?? COLUMNS, COLUMNS - item.x);
    const minH = def.minH ?? 1;
    const maxH = def.maxH ?? 12;

    const onMove = (move: PointerEvent) => {
      const current = layoutRef.current.items.find((x) => x.id === id);
      if (!current) return;
      const w = clamp(
        startW + Math.round((move.clientX - startX) / metrics.colStep),
        minW,
        maxW,
      );
      const h = clamp(
        startH + Math.round((move.clientY - startY) / metrics.rowStep),
        minH,
        maxH,
      );
      const candidate = { ...current, w, h, x: item.x, y: item.y };
      setWorkingLayout(layoutWithResolvedCollisions(layoutRef.current, id, candidate));
    };
    const onUp = () => {
      window.removeEventListener("pointermove", onMove);
      window.removeEventListener("pointerup", onUp);
      window.removeEventListener("pointercancel", onUp);
      persist(layoutRef.current);
      setDragging(null);
    };
    window.addEventListener("pointermove", onMove);
    window.addEventListener("pointerup", onUp);
    window.addEventListener("pointercancel", onUp);
  };

  return (
    <div className={"editable-page" + (editMode ? " is-editing" : "")}>
      {editMode && (
        <div className="editable-toolbar">
          <div className="editable-toolbar-main">
            <span className="editable-badge">
              <Icon name="layout" /> Layout editing
            </span>
            <span className="editable-toolbar-sub">{visible.length} active cards</span>
          </div>
          {hidden.length > 0 && (
            <div className="editable-add-list">
              {hidden.map((item) => {
                const def = defsById.get(item.id);
                if (!def) return null;
                return (
                  <button className="filter-pill" key={item.id} onClick={() => showCard(item.id)}>
                    <Icon name="plus" />
                    {def.label}
                  </button>
                );
              })}
            </div>
          )}
          <button className="filter-pill" onClick={reset}>
            <Icon name="reset" />
            Reset
          </button>
        </div>
      )}

      <div className="editable-grid" ref={gridRef}>
        {visible.length === 0 && (
          <div className="editable-empty">
            No cards are active on this page.
          </div>
        )}
        {visible.map((item) => {
          const def = defsById.get(item.id);
          if (!def) return null;
          const style = {
            gridColumn: `${item.x + 1} / span ${item.w}`,
            gridRow: `${item.y + 1} / span ${item.h}`,
            "--editable-item-min-h": `${item.h * ROW_HEIGHT + Math.max(0, item.h - 1) * GRID_GAP}px`,
          } as React.CSSProperties;
          return (
            <div
              className={
                "editable-grid-item" +
                (editMode ? " editing" : "") +
                (dragging === item.id ? " dragging" : "")
              }
              data-card-id={item.id}
              key={item.id}
              style={style}
            >
              {editMode && (
                <>
                  <button
                    className="editable-drag"
                    title={`Move ${def.label}`}
                    aria-label={`Move ${def.label}`}
                    onPointerDown={(e) => startMove(e, item.id)}
                  >
                    <Icon name="grip" />
                  </button>
                  <div className="editable-item-tools" aria-label={`${def.label} layout controls`}>
                    <button title="Remove card" onClick={() => hideCard(item.id)}>
                      <Icon name="trash" />
                    </button>
                  </div>
                  <button
                    className="editable-resize"
                    title={`Resize ${def.label}`}
                    aria-label={`Resize ${def.label}`}
                    onPointerDown={(e) => startResize(e, item.id)}
                  />
                </>
              )}
              {def.content}
            </div>
          );
        })}
      </div>
    </div>
  );
}
