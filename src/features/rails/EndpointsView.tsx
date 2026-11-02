/**
 * Endpoints tool window, laid out like IntelliJ's: Module / Type / Framework
 * filters and a details toggle, a search field, the endpoints grouped by
 * module (`/photos/:id [GET]` with the controller class at the right), and a
 * details pane with the HTTP Client request (Submit Request / Open in
 * Editor) and the route's documentation.
 */
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { create } from "zustand";
import clsx from "clsx";
import type { RailsRoute } from "@/bindings";
import { tx } from "@/platform/i18n";
import { basename, useProject } from "@/platform/project";
import { editorService, useOccurrenceNavigator } from "@/workbench/contributions";
import {
  Button,
  EmptyState,
  Icon,
  IconButton,
  Input,
  Menu,
  Spinner,
  SplitGroup,
  SplitHandle,
  SplitPanel,
  Tabs,
  Tree,
  T,
  type MenuItem,
  type TreeHandle,
  type TreeRowState,
} from "@/ui";
import { generateIntoScratch, renderRequestBlock } from "../http-client/generate";
import { runRequest } from "../http-client/store";
import {
  controllerClass,
  diagnosticLines,
  filterRoutes,
  groupByFile,
  navigationTarget,
  relativeTo,
  requestSpec,
  routeKey,
  routeTarget,
  type RouteGroup,
} from "./model";
import { loadRoutes, selectRoute, useRailsRoutes } from "./store";
import styles from "./EndpointsView.module.css";

export const ENDPOINTS_WINDOW = "endpoints";

/** The only type and framework the window lists so far. */
const TYPE_HTTP = "HTTP";
const FRAMEWORK_RAILS = "Rails";

const OPTIONS_KEY = "openidea.endpoints.view.v1";

interface ViewOptions {
  /** File nodes under the module. */
  groupByFile: boolean;
  /** Selecting an endpoint opens it in the editor. */
  autoscroll: boolean;
  /** The details pane (IntelliJ's eye toggle). */
  details: boolean;
}

function loadOptions(): Partial<ViewOptions> {
  try {
    return JSON.parse(globalThis.localStorage?.getItem(OPTIONS_KEY) ?? "{}") as Partial<ViewOptions>;
  } catch {
    return {};
  }
}

export const useEndpointsViewOptions = create<ViewOptions>(() => ({
  groupByFile: false,
  autoscroll: false,
  details: true,
  ...loadOptions(),
}));

useEndpointsViewOptions.subscribe((s) => {
  try {
    globalThis.localStorage?.setItem(OPTIONS_KEY, JSON.stringify(s));
  } catch {
    /* storage unavailable */
  }
});

type DetailsTab = "http" | "docs";

interface ViewState {
  query: string;
  /** Unchecked entries of the Module / Type / Framework filters. */
  hiddenModules: Set<string>;
  hiddenTypes: Set<string>;
  hiddenFrameworks: Set<string>;
  collapsed: Set<string>;
  showDiagnostics: boolean;
  tab: DetailsTab;
  /** The endpoint shown in the details pane. */
  current: RailsRoute | null;
}

export const useEndpointsView = create<ViewState>(() => ({
  query: "",
  hiddenModules: new Set(),
  hiddenTypes: new Set(),
  hiddenFrameworks: new Set(),
  collapsed: new Set(),
  showDiagnostics: false,
  tab: "http",
  current: null,
}));

/** The tool window's ⋮ menu entries. */
export function useEndpointsHeaderMenu(): MenuItem[] {
  const o = useEndpointsViewOptions();
  const toggle = (key: keyof ViewOptions) => () => useEndpointsViewOptions.setState({ [key]: !o[key] });
  return [
    { id: "group", label: "Group by File", checked: o.groupByFile, run: toggle("groupByFile") },
    { id: "details", label: "Show Details", checked: o.details, run: toggle("details") },
    { id: "autoscroll", label: "Autoscroll to Source", checked: o.autoscroll, run: toggle("autoscroll") },
  ];
}

type Node =
  | { kind: "module"; key: string; name: string; routes: RailsRoute[] }
  | { kind: "file"; key: string; group: RouteGroup }
  | { kind: "route"; key: string; route: RailsRoute };

export function openRoute(route: RailsRoute, focus: boolean) {
  const { path, line, col } = navigationTarget(route);
  void editorService().openFile(path, { selection: { line, col }, focus, preview: !focus });
}

/** The endpoints after the search and the filters, in table order. */
export function useVisibleRoutes(): RailsRoute[] {
  const table = useRailsRoutes((s) => s.table);
  const query = useEndpointsView((s) => s.query);
  const hiddenModules = useEndpointsView((s) => s.hiddenModules);
  const hiddenTypes = useEndpointsView((s) => s.hiddenTypes);
  const hiddenFrameworks = useEndpointsView((s) => s.hiddenFrameworks);
  const module = useProject((s) => s.project?.name ?? "");
  return useMemo(() => {
    if (hiddenModules.has(module) || hiddenTypes.has(TYPE_HTTP) || hiddenFrameworks.has(FRAMEWORK_RAILS)) return [];
    return filterRoutes(table?.routes ?? [], query);
  }, [table, query, hiddenModules, hiddenTypes, hiddenFrameworks, module]);
}

export function EndpointsHeaderActions() {
  const loading = useRailsRoutes((s) => s.loading);
  const diagnostics = useRailsRoutes((s) => s.table?.diagnostics.length ?? 0);
  const showDiagnostics = useEndpointsView((s) => s.showDiagnostics);
  return (
    <>
      {diagnostics > 0 && (
        <IconButton
          icon="warning"
          size="small"
          title={
            diagnostics === 1 ? tx`${diagnostics} route could not be read` : tx`${diagnostics} routes could not be read`
          }
          pressed={showDiagnostics}
          tone="danger"
          onClick={() => useEndpointsView.setState({ showDiagnostics: !showDiagnostics })}
        />
      )}
      <IconButton icon="refresh" size="small" title="Refresh" disabled={loading} onClick={() => void loadRoutes()} />
    </>
  );
}

/** `Module ▾`: a dropdown of checkable entries. */
function FilterMenu({
  label,
  entries,
  hidden,
  onToggle,
}: {
  label: string;
  entries: string[];
  hidden: ReadonlySet<string>;
  onToggle: (entry: string) => void;
}) {
  const [anchor, setAnchor] = useState<HTMLElement | null>(null);
  const items: MenuItem[] = entries.map((e) => ({
    id: e,
    label: e,
    verbatim: true,
    checked: !hidden.has(e),
    run: () => onToggle(e),
  }));
  return (
    <>
      <button
        type="button"
        className={clsx(styles.filterButton, hidden.size > 0 && styles.filterActive)}
        aria-haspopup="menu"
        aria-expanded={!!anchor}
        onClick={(e) => setAnchor(anchor ? null : e.currentTarget)}
      >
        <T>{label}</T>
        <Icon name="chevron-down" size={16} className={styles.filterChevron} />
      </button>
      <Menu open={!!anchor} anchor={anchor} items={items} onClose={() => setAnchor(null)} aria-label={label} />
    </>
  );
}

function toggleIn(set: Set<string>, entry: string): Set<string> {
  const next = new Set(set);
  if (!next.delete(entry)) next.add(entry);
  return next;
}

function FilterBar() {
  const module = useProject((s) => s.project?.name ?? "");
  const { hiddenModules, hiddenTypes, hiddenFrameworks } = useEndpointsView();
  const details = useEndpointsViewOptions((o) => o.details);
  return (
    <div className={styles.filterBar}>
      <FilterMenu
        label="Module"
        entries={module ? [module] : []}
        hidden={hiddenModules}
        onToggle={(m) => useEndpointsView.setState((s) => ({ hiddenModules: toggleIn(s.hiddenModules, m) }))}
      />
      <FilterMenu
        label="Type"
        entries={[TYPE_HTTP]}
        hidden={hiddenTypes}
        onToggle={(t) => useEndpointsView.setState((s) => ({ hiddenTypes: toggleIn(s.hiddenTypes, t) }))}
      />
      <FilterMenu
        label="Framework"
        entries={[FRAMEWORK_RAILS]}
        hidden={hiddenFrameworks}
        onToggle={(f) => useEndpointsView.setState((s) => ({ hiddenFrameworks: toggleIn(s.hiddenFrameworks, f) }))}
      />
      <span className={styles.spacer} />
      <IconButton
        icon="eye"
        size="small"
        title="Show Details"
        pressed={details}
        onClick={() => useEndpointsViewOptions.setState({ details: !details })}
      />
    </div>
  );
}

function SearchBar() {
  const query = useEndpointsView((s) => s.query);
  return (
    <div className={styles.searchBar}>
      <Icon name="search" size={16} className={styles.searchIcon} />
      <Input
        variant="bare"
        className={styles.search}
        placeholder="Search endpoints"
        aria-label="Search endpoints"
        value={query}
        onChange={(e) => useEndpointsView.setState({ query: e.target.value })}
      />
    </div>
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

function EndpointRow({ route }: { route: RailsRoute }) {
  return (
    <span className={styles.endpoint} title={`${route.verb} ${route.path} → ${routeTarget(route)}`}>
      <Icon name="web" size={16} className={styles.endpointIcon} />
      <span className={styles.path}>{route.path}</span>
      <span className={clsx(styles.verb, verbClass(route.verb))}>[{route.verb}]</span>
      <span className={styles.spacer} />
      <span className={styles.controller}>{controllerClass(route)}</span>
    </span>
  );
}

// --- details pane ----------------------------------------------------------------

function HttpClientTab({ route }: { route: RailsRoute }) {
  const spec = requestSpec(route);
  const block = renderRequestBlock(spec);
  const lines = block.trimEnd().split("\n");
  const submit = () => void runRequest(null, block, 0);
  const openInEditor = () => void generateIntoScratch([block]);
  return (
    <div className={styles.httpTab}>
      <div className={styles.request} role="region" aria-label="HTTP request">
        {lines.map((line, i) => (
          <div key={i} className={clsx(styles.requestLine, i === 1 && styles.requestMain)}>
            <span className={styles.requestGutter}>
              {i === 0 && <IconButton icon="run" size="small" title="Submit Request" onClick={submit} />}
            </span>
            <span className={styles.requestText}>
              {i === 1 ? (
                <>
                  <span className={clsx(styles.verb, verbClass(route.verb))}>{spec.method}</span> {spec.url}
                </>
              ) : (
                line
              )}
            </span>
          </div>
        ))}
      </div>
      <div className={styles.detailButtons}>
        <Button onClick={submit}>Submit Request</Button>
        <Button onClick={openInEditor}>Open in Editor</Button>
      </div>
    </div>
  );
}

function DocumentationTab({ route }: { route: RailsRoute }) {
  const root = useProject((s) => s.project?.root ?? "");
  const rows: [string, string][] = [
    ["Path", route.path],
    ["Method", route.verb],
    ["Controller", controllerClass(route)],
    ["Action", route.action || "—"],
    ["Helper", route.name ? `${route.name}_path / ${route.name}_url` : "—"],
  ];
  return (
    <div className={styles.docs}>
      <dl className={styles.docList}>
        {rows.map(([k, v]) => (
          <div key={k} className={styles.docRow}>
            <dt>
              <T>{k}</T>
            </dt>
            <dd>{v}</dd>
          </div>
        ))}
        <div className={styles.docRow}>
          <dt>
            <T>Defined in</T>
          </dt>
          <dd>
            <button type="button" className={styles.link} onClick={() => openRoute(route, true)}>
              {relativeTo(root, route.file)}:{route.line}
            </button>
          </dd>
        </div>
      </dl>
    </div>
  );
}

function DetailsPane({ route }: { route: RailsRoute | null }) {
  const tab = useEndpointsView((s) => s.tab);
  return (
    <div className={styles.details}>
      <Tabs
        variant="compact"
        aria-label="Endpoint details"
        className={styles.detailTabs}
        activeId={tab}
        onSelect={(id) => useEndpointsView.setState({ tab: id as DetailsTab })}
        items={[
          { id: "http", label: "HTTP Client" },
          { id: "docs", label: "Documentation" },
        ]}
      />
      {!route ? (
        <div className={styles.detailEmpty}>
          <T>Select an endpoint</T>
        </div>
      ) : tab === "http" ? (
        <HttpClientTab route={route} />
      ) : (
        <DocumentationTab route={route} />
      )}
    </div>
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

// --- the window ------------------------------------------------------------------

export function EndpointsView() {
  const { table, loading, loaded, error, selectedKey } = useRailsRoutes();
  const module = useProject((s) => s.project?.name ?? "");
  const routes = useVisibleRoutes();
  const { groupByFile: grouped, details } = useEndpointsViewOptions();
  const showDiagnostics = useEndpointsView((s) => s.showDiagnostics);
  const collapsed = useEndpointsView((s) => s.collapsed);
  const current = useEndpointsView((s) => s.current);
  const tree = useRef<TreeHandle>(null);

  useOccurrenceNavigator(ENDPOINTS_WINDOW, (dir, focus) => {
    const step = tree.current?.selectOccurrence(dir);
    const n = step?.node as Node | undefined;
    if (!step || n?.kind !== "route") return null;
    openRoute(n.route, focus);
    return step;
  });

  const moduleKey = `module:${module}`;
  const roots = useMemo<Node[]>(
    () => (routes.length ? [{ kind: "module", key: moduleKey, name: module, routes }] : []),
    [routes, module, moduleKey],
  );
  const getChildren = useCallback(
    (n: Node): Node[] | undefined => {
      const leaf = (r: RailsRoute): Node => ({ kind: "route", key: routeKey(r), route: r });
      if (n.kind === "module")
        return grouped
          ? groupByFile(n.routes).map((g) => ({ kind: "file", key: `file:${g.file}`, group: g }))
          : n.routes.map(leaf);
      if (n.kind === "file") return n.group.routes.map(leaf);
      return undefined;
    },
    [grouped],
  );
  const expanded = useMemo(() => {
    const keys = new Set<string>();
    if (routes.length && !collapsed.has(moduleKey)) keys.add(moduleKey);
    if (grouped) for (const g of groupByFile(routes)) if (!collapsed.has(`file:${g.file}`)) keys.add(`file:${g.file}`);
    return keys;
  }, [routes, collapsed, grouped, moduleKey]);

  // "Show in Endpoints": reveal the chosen route once it is in the tree.
  useEffect(() => {
    if (!selectedKey) return;
    const route = routes.find((r) => routeKey(r) === selectedKey);
    if (!route) return;
    const t = setTimeout(() => {
      tree.current?.reveal(selectedKey);
      useEndpointsView.setState({ current: route });
      selectRoute(null);
    }, 0);
    return () => clearTimeout(t);
  }, [selectedKey, routes, grouped]);

  // The details pane follows the table: a route that disappeared is no longer current.
  useEffect(() => {
    if (current && !routes.some((r) => routeKey(r) === routeKey(current))) useEndpointsView.setState({ current: null });
  }, [routes, current]);

  const renderNode = (n: Node, _state: TreeRowState) => {
    if (n.kind === "module")
      return (
        <span className={styles.groupRow}>
          <Icon name="module" size={16} className={styles.groupIcon} />
          <span className={styles.groupName}>{n.name}</span>
          <span className={styles.count}>
            {n.routes.length === 1 ? tx`${n.routes.length} route` : tx`${n.routes.length} routes`}
          </span>
        </span>
      );
    if (n.kind === "file")
      return (
        <span className={styles.groupRow}>
          <Icon name="file-ruby" size={16} className={styles.groupIcon} />
          <span className={styles.groupName}>{basename(n.group.file)}</span>
          <span className={styles.count}>
            {n.group.routes.length === 1 ? tx`${n.group.routes.length} route` : tx`${n.group.routes.length} routes`}
          </span>
        </span>
      );
    return <EndpointRow route={n.route} />;
  };

  const empty = error ? (
    <EmptyState icon="toolwindow-endpoints" title="Endpoints unavailable">
      {error}
    </EmptyState>
  ) : loading || !loaded ? (
    <div className={styles.loading}>
      <Spinner size={14} /> <T>Reading config/routes.rb…</T>
    </div>
  ) : !table ? (
    <EmptyState icon="toolwindow-endpoints" title="Not a Rails project">
      <T>No config/routes.rb in this project.</T>
    </EmptyState>
  ) : table.routes.length ? (
    <EmptyState icon="toolwindow-endpoints" title="No endpoints match the filters" />
  ) : (
    <EmptyState icon="toolwindow-endpoints" title="No endpoints found" />
  );

  return (
    <div className={styles.view}>
      <FilterBar />
      <SearchBar />
      <div className={styles.body}>
        <SplitGroup orientation="vertical" id="endpoints-details-split">
          <SplitPanel id="endpoints" minSize={120} className={styles.panel}>
            <Tree<Node>
              handleRef={tree}
              roots={roots}
              getKey={(n) => n.key}
              getLabel={(n) =>
                n.kind === "module"
                  ? n.name
                  : n.kind === "file"
                    ? basename(n.group.file)
                    : `${n.route.path} ${n.route.verb}`
              }
              getChildren={getChildren}
              isExpandable={(n) => n.kind !== "route"}
              expanded={expanded}
              onExpandedChange={(_next, node, isExpanded) =>
                useEndpointsView.setState((s) => {
                  const c = new Set(s.collapsed);
                  if (isExpanded) c.delete(node.key);
                  else c.add(node.key);
                  return { collapsed: c };
                })
              }
              renderNode={renderNode}
              onActivate={(n) => {
                if (n.kind === "route") openRoute(n.route, true);
              }}
              onSelectionChange={(_sel, node) => {
                if (node?.kind !== "route") return;
                useEndpointsView.setState({ current: node.route });
                if (useEndpointsViewOptions.getState().autoscroll) openRoute(node.route, false);
              }}
              activateExpandable={false}
              multiSelect={false}
              aria-label="Endpoints"
              empty={empty}
            />
          </SplitPanel>
          {details && (
            <>
              <SplitHandle />
              <SplitPanel id="details" minSize={120} defaultSize="40%" className={styles.panel}>
                <DetailsPane route={current} />
              </SplitPanel>
            </>
          )}
        </SplitGroup>
      </div>
      {showDiagnostics && <Diagnostics />}
    </div>
  );
}
