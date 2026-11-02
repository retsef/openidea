/** Routes tool window: the Rails route table (Verb | Path | Controller#action | Helper), filtered, optionally grouped by file. */
import { useCallback, useEffect, useMemo, useRef } from "react";
import { create } from "zustand";
import clsx from "clsx";
import type { RailsRoute } from "@/bindings";
import { tx } from "@/platform/i18n";
import { basename, useProject } from "@/platform/project";
import { editorService, useOccurrenceNavigator } from "@/workbench/contributions";
import {
  EmptyState,
  Icon,
  IconButton,
  Input,
  Spinner,
  Tree,
  T,
  type MenuItem,
  type TreeHandle,
  type TreeRowState,
} from "@/ui";
import {
  diagnosticLines,
  filterRoutes,
  groupByFile,
  navigationTarget,
  relativeTo,
  routeKey,
  routeTarget,
  type RouteGroup,
} from "./model";
import { loadRoutes, selectRoute, useRailsRoutes } from "./store";
import styles from "./RoutesView.module.css";

export const ROUTES_WINDOW = "rails.routes";

const OPTIONS_KEY = "openidea.rails.routes.view.v1";

interface ViewOptions {
  grouped: boolean;
  /** Selecting a route opens it in the editor. */
  autoscroll: boolean;
}

function loadOptions(): Partial<ViewOptions> {
  try {
    return JSON.parse(globalThis.localStorage?.getItem(OPTIONS_KEY) ?? "{}") as Partial<ViewOptions>;
  } catch {
    return {};
  }
}

export const useRoutesViewOptions = create<ViewOptions>(() => ({
  grouped: false,
  autoscroll: false,
  ...loadOptions(),
}));

useRoutesViewOptions.subscribe((s) => {
  try {
    globalThis.localStorage?.setItem(OPTIONS_KEY, JSON.stringify(s));
  } catch {
    /* storage unavailable */
  }
});

interface ViewState {
  query: string;
  collapsed: Set<string>;
  showDiagnostics: boolean;
}

export const useRoutesView = create<ViewState>(() => ({ query: "", collapsed: new Set(), showDiagnostics: false }));

/** The tool window's ⋮ menu entries. */
export function useRoutesHeaderMenu(): MenuItem[] {
  const o = useRoutesViewOptions();
  const toggle = (key: keyof ViewOptions) => () => useRoutesViewOptions.setState({ [key]: !o[key] });
  return [
    { id: "group", label: "Group by File", checked: o.grouped, run: toggle("grouped") },
    { id: "autoscroll", label: "Autoscroll to Source", checked: o.autoscroll, run: toggle("autoscroll") },
  ];
}

type Node = { kind: "file"; key: string; group: RouteGroup } | { kind: "route"; key: string; route: RailsRoute };

export function openRoute(route: RailsRoute, focus: boolean) {
  const { path, line, col } = navigationTarget(route);
  void editorService().openFile(path, { selection: { line, col }, focus, preview: !focus });
}

/** The filtered routes, in table order. */
export function useVisibleRoutes(): RailsRoute[] {
  const table = useRailsRoutes((s) => s.table);
  const query = useRoutesView((s) => s.query);
  return useMemo(() => filterRoutes(table?.routes ?? [], query), [table, query]);
}

export function RoutesHeaderActions() {
  const query = useRoutesView((s) => s.query);
  const loading = useRailsRoutes((s) => s.loading);
  const diagnostics = useRailsRoutes((s) => s.table?.diagnostics.length ?? 0);
  const showDiagnostics = useRoutesView((s) => s.showDiagnostics);
  const grouped = useRoutesViewOptions((o) => o.grouped);
  return (
    <>
      <Input
        variant="bare"
        className={styles.filter}
        placeholder="Filter routes"
        aria-label="Filter routes"
        value={query}
        onChange={(e) => useRoutesView.setState({ query: e.target.value })}
      />
      <IconButton
        icon="folder"
        size="small"
        title="Group by File"
        pressed={grouped}
        onClick={() => useRoutesViewOptions.setState({ grouped: !grouped })}
      />
      {diagnostics > 0 && (
        <IconButton
          icon="warning"
          size="small"
          title={
            diagnostics === 1 ? tx`${diagnostics} route could not be read` : tx`${diagnostics} routes could not be read`
          }
          pressed={showDiagnostics}
          tone="danger"
          onClick={() => useRoutesView.setState({ showDiagnostics: !showDiagnostics })}
        />
      )}
      <IconButton icon="refresh" size="small" title="Refresh" disabled={loading} onClick={() => void loadRoutes()} />
    </>
  );
}

function verbClass(verb: RailsRoute["verb"]): string | undefined {
  switch (verb) {
    case "GET":
      return styles.get;
    case "POST":
      return styles.post;
    case "PUT":
    case "PATCH":
      return styles.put;
    case "DELETE":
      return styles.delete;
    default:
      return undefined;
  }
}

function RouteRow({ route, selected }: { route: RailsRoute; selected: boolean }) {
  return (
    <span className={clsx(styles.row, selected && styles.selected)} title={`${route.verb} ${route.path}`}>
      <span className={clsx(styles.verb, verbClass(route.verb))}>{route.verb}</span>
      <span className={styles.path}>{route.path}</span>
      <span className={styles.target}>{routeTarget(route)}</span>
      <span className={styles.helper}>{route.name ? `${route.name}_path` : ""}</span>
    </span>
  );
}

function Diagnostics() {
  const table = useRailsRoutes((s) => s.table);
  const root = useProject((s) => s.project?.root ?? "");
  if (!table?.diagnostics.length) return null;
  const lines = diagnosticLines(table, root);
  return (
    <div className={styles.diagnostics} role="list" aria-label="Routes that could not be read">
      {table.diagnostics.map((d, i) => (
        <button
          key={`${d.file}:${d.line}:${i}`}
          type="button"
          role="listitem"
          className={styles.diagnostic}
          onClick={() =>
            void editorService().openFile(d.file, { selection: { line: Math.max(0, d.line - 1), col: 0 }, focus: true })
          }
        >
          <Icon name="warning" size={14} className={styles.diagnosticIcon} />
          <span>{lines[i]}</span>
        </button>
      ))}
    </div>
  );
}

export function RoutesView() {
  const { table, loading, loaded, error, selectedKey } = useRailsRoutes();
  const root = useProject((s) => s.project?.root ?? "");
  const routes = useVisibleRoutes();
  const grouped = useRoutesViewOptions((o) => o.grouped);
  const showDiagnostics = useRoutesView((s) => s.showDiagnostics);
  const collapsed = useRoutesView((s) => s.collapsed);
  const tree = useRef<TreeHandle>(null);

  useOccurrenceNavigator(ROUTES_WINDOW, (dir, focus) => {
    const step = tree.current?.selectOccurrence(dir);
    const n = step?.node as Node | undefined;
    if (!step || n?.kind !== "route") return null;
    openRoute(n.route, focus);
    return step;
  });

  const roots = useMemo<Node[]>(() => {
    const route = (r: RailsRoute): Node => ({ kind: "route", key: routeKey(r), route: r });
    if (!grouped) return routes.map(route);
    return groupByFile(routes).map((g) => ({ kind: "file", key: g.file, group: g }));
  }, [routes, grouped]);
  const getChildren = useCallback(
    (n: Node): Node[] | undefined =>
      n.kind === "file" ? n.group.routes.map((r) => ({ kind: "route", key: routeKey(r), route: r })) : undefined,
    [],
  );
  const expanded = useMemo(
    () => new Set(roots.filter((r) => r.kind === "file" && !collapsed.has(r.key)).map((r) => r.key)),
    [roots, collapsed],
  );

  // "Show in Routes": reveal the chosen route once it is in the tree.
  useEffect(() => {
    if (!selectedKey) return;
    const present = routes.some((r) => routeKey(r) === selectedKey);
    if (!present) return;
    const t = setTimeout(() => {
      tree.current?.reveal(selectedKey);
      selectRoute(null);
    }, 0);
    return () => clearTimeout(t);
  }, [selectedKey, routes, grouped]);

  const renderNode = (n: Node, state: TreeRowState) => {
    if (n.kind === "file") {
      const name = basename(n.group.file);
      return (
        <span className={styles.fileRow}>
          <Icon name="file-ruby" size={16} className={styles.fileIcon} />
          <span className={styles.fileName}>{name}</span>
          <span className={styles.muted}>{relativeTo(root, n.group.file)}</span>
          <span className={styles.count}>
            {n.group.routes.length === 1 ? tx`${n.group.routes.length} route` : tx`${n.group.routes.length} routes`}
          </span>
        </span>
      );
    }
    return <RouteRow route={n.route} selected={state.selected} />;
  };

  const empty = error ? (
    <EmptyState icon="web" title="Routes unavailable">
      {error}
    </EmptyState>
  ) : loading || !loaded ? (
    <div className={styles.loading}>
      <Spinner size={14} /> <T>Reading config/routes.rb…</T>
    </div>
  ) : !table ? (
    <EmptyState icon="web" title="Not a Rails project">
      <T>No config/routes.rb in this project.</T>
    </EmptyState>
  ) : table.routes.length ? (
    <EmptyState icon="web" title="No routes match the filter" />
  ) : (
    <EmptyState icon="web" title="No routes defined" />
  );

  return (
    <div className={styles.view}>
      <div className={styles.header} aria-hidden>
        <span className={clsx(styles.row, styles.headerRow)}>
          <span className={styles.verb}>
            <T>Verb</T>
          </span>
          <span className={styles.path}>
            <T>Path</T>
          </span>
          <span className={styles.target}>
            <T>Controller#Action</T>
          </span>
          <span className={styles.helper}>
            <T>Helper</T>
          </span>
        </span>
      </div>
      <div className={styles.body}>
        <Tree<Node>
          handleRef={tree}
          roots={roots}
          getKey={(n) => n.key}
          getLabel={(n) => (n.kind === "file" ? basename(n.group.file) : `${n.route.verb} ${n.route.path}`)}
          getChildren={getChildren}
          isExpandable={(n) => n.kind === "file"}
          expanded={expanded}
          onExpandedChange={(_next, node, isExpanded) =>
            useRoutesView.setState((s) => {
              const c = new Set(s.collapsed);
              if (isExpanded) c.delete(node.key);
              else c.add(node.key);
              return { collapsed: c };
            })
          }
          renderNode={renderNode}
          onActivate={(n) => {
            if (n.kind === "route") openRoute(n.route, true);
            else if (n.group.routes[0]) openRoute(n.group.routes[0], true);
          }}
          onSelectionChange={(_sel, node) => {
            if (node?.kind === "route" && useRoutesViewOptions.getState().autoscroll) openRoute(node.route, false);
          }}
          activateExpandable={false}
          multiSelect={false}
          speedSearch
          horizontalScroll
          aria-label="Rails routes"
          empty={empty}
        />
      </div>
      {showDiagnostics && <Diagnostics />}
    </div>
  );
}
