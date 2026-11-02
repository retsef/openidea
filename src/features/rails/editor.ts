/**
 * `config/routes.rb` and `config/routes/*.rb` editors: a gutter icon on every
 * DSL line that produces routes (from the project's route table), a menu per
 * line with "Generate Request in HTTP Client" and "Show in Endpoints" for each
 * route, the same as a context action at the caret.
 */
import { RangeSet, StateEffect, StateField, type EditorState, type Extension } from "@codemirror/state";
import { EditorView, GutterMarker, ViewPlugin, gutter } from "@codemirror/view";
import { activeEditor, intentionProviders, type ContributedIntention, type EditorDocContext } from "@/editor/api";
import type { RailsRoute } from "@/bindings";
import { tx } from "@/platform/i18n";
import { isVirtualUri } from "@/platform/virtualDocs";
import { gutterIconMarkup, showContextMenu, type MenuItem } from "@/ui";
import { notifications, toolWindowService, type MenuGroupContext } from "@/workbench/contributions";
import { generateIntoScratch, renderRequestBlock } from "../http-client/generate";
import styles from "../http-client/HttpClient.module.css";
import { isRoutesFile, requestSpec, routeKey, routeLines, routeTarget, routesAt } from "./model";
import { ENDPOINTS_WINDOW } from "./EndpointsView";
import { selectRoute, useRailsRoutes } from "./store";

export const GENERATE_REQUEST = "http.generateRailsRequest";
export const SHOW_IN_ROUTES = "rails.showInRoutes";

/** Writes the requests of `routes` into the generated-requests scratch. */
export async function generateRequests(routes: RailsRoute[]): Promise<void> {
  if (!routes.length) return;
  try {
    await generateIntoScratch(routes.map((r) => renderRequestBlock(requestSpec(r))));
  } catch (err) {
    notifications().notify({
      level: "error",
      title: "Cannot generate the HTTP request",
      body: err instanceof Error ? err.message : String(err),
      key: "http-generate",
    });
  }
}

export function showInRoutes(route: RailsRoute) {
  selectRoute(routeKey(route));
  toolWindowService().show(ENDPOINTS_WINDOW, true);
}

/** Menu entries for the routes a DSL line produces: one submenu per route when there are several. */
export function routeMenuItems(routes: RailsRoute[]): MenuItem[] {
  const items = (r: RailsRoute): MenuItem[] => [
    { label: "Generate Request in HTTP Client", icon: "file-http", run: () => void generateRequests([r]) },
    { label: "Show in Endpoints", icon: "web", run: () => showInRoutes(r) },
  ];
  if (routes.length === 1) return items(routes[0]!);
  return [
    ...routes.map((r) => ({ label: `${r.verb} ${r.path}`, verbatim: true, icon: "web", submenu: items(r) })),
    { kind: "separator" as const },
    { label: "Generate Requests for All Routes Here", icon: "file-http", run: () => void generateRequests(routes) },
  ];
}

/** The routes of the DSL call at the caret of the active routes-file editor. */
export function routesAtCaret(): RailsRoute[] {
  const e = activeEditor();
  if (!e || e.ctx.language !== "ruby" || !isRoutesFile(e.ctx.path)) return [];
  const table = useRailsRoutes.getState().table;
  if (!table) return [];
  const line = e.view.state.doc.lineAt(e.view.state.selection.main.head).number;
  return routesAt(table.routes, e.ctx.path, line);
}

/** Editor context menu items for a routes file. */
export function menuGroupItems(ctx: MenuGroupContext): MenuItem[] {
  const path = ctx.paths[0];
  if (!path || ctx.text === undefined || ctx.line === undefined || !isRoutesFile(path)) return [];
  const e = activeEditor();
  if (!e || e.ctx.path !== path) return [];
  const routes = routesAtCaret();
  if (!routes.length) return [];
  if (routes.length === 1)
    return [
      { kind: "action", action: GENERATE_REQUEST, icon: "file-http" },
      { kind: "action", action: SHOW_IN_ROUTES, icon: "web" },
    ];
  return [{ label: "Rails Routes", icon: "web", submenu: routeMenuItems(routes) }];
}

// --- gutter ----------------------------------------------------------------------

interface Mark {
  /** Offset of the line start. */
  pos: number;
  /** 1-based line in the saved table. */
  line: number;
  title: string;
  routes: RailsRoute[];
}

const setMarks = StateEffect.define<Mark[]>();

const marksField = StateField.define<Mark[]>({
  create: () => [],
  update(value, tr) {
    for (const e of tr.effects) if (e.is(setMarks)) return e.value;
    if (!tr.docChanged || !value.length) return value;
    // Unsaved edits move the icons with their lines; the table itself follows the next save.
    return value.map((m) => ({ ...m, pos: tr.state.doc.lineAt(tr.changes.mapPos(m.pos, -1)).from }));
  },
});

/** Marks for `path` from the current table. */
export function computeMarks(state: EditorState, path: string, routes: readonly RailsRoute[]): Mark[] {
  const doc = state.doc;
  return routeLines(routes, path)
    .filter((line) => line >= 1 && line <= doc.lines)
    .map((line) => {
      const here = routesAt(routes, path, line);
      const title =
        here.length === 1 ? `${here[0]!.verb} ${here[0]!.path} → ${routeTarget(here[0]!)}` : tx`${here.length} routes`;
      return { pos: doc.line(line).from, line, title, routes: here };
    });
}

class RouteMarker extends GutterMarker {
  constructor(readonly mark: Mark) {
    super();
  }
  override eq(other: GutterMarker): boolean {
    return (
      other instanceof RouteMarker &&
      other.mark.title === this.mark.title &&
      other.mark.routes.length === this.mark.routes.length
    );
  }
  override toDOM(): Node {
    const el = document.createElement("div");
    el.className = styles.runMarker ?? "";
    el.title = this.mark.title;
    el.setAttribute("role", "button");
    el.setAttribute("aria-label", this.mark.title);
    el.innerHTML = gutterIconMarkup("web") ?? "";
    return el;
  }
}

function sameMarks(a: Mark[], b: Mark[]): boolean {
  return (
    a.length === b.length &&
    a.every((m, i) => m.pos === b[i]!.pos && m.title === b[i]!.title && m.routes.length === b[i]!.routes.length)
  );
}

export function railsRoutesEditorExtension(ctx: EditorDocContext): Extension {
  if (ctx.language !== "ruby" || ctx.virtual || isVirtualUri(ctx.path) || !isRoutesFile(ctx.path)) return [];
  const path = ctx.path;
  const follower = ViewPlugin.fromClass(
    class {
      unsubscribe: () => void;
      constructor(readonly view: EditorView) {
        this.unsubscribe = useRailsRoutes.subscribe((s, prev) => {
          if (s.table !== prev.table) this.refresh();
        });
        // The table may already be there (the editor opened after the project).
        setTimeout(() => this.refresh(), 0);
      }
      refresh() {
        const routes = useRailsRoutes.getState().table?.routes ?? [];
        const found = computeMarks(this.view.state, path, routes);
        if (!sameMarks(this.view.state.field(marksField), found)) this.view.dispatch({ effects: setMarks.of(found) });
      }
      destroy() {
        this.unsubscribe();
      }
    },
  );
  const intentions = (view: EditorView): ContributedIntention[] => {
    const marks = view.state.field(marksField, false);
    if (!marks?.length) return [];
    const line = view.state.doc.lineAt(view.state.selection.main.head).from;
    const mark = marks.find((m) => m.pos === line);
    if (!mark) return [];
    return [
      {
        id: GENERATE_REQUEST,
        label: "Generate Request in HTTP Client",
        icon: "file-http",
        run: () => generateRequests(mark.routes),
      },
      {
        id: SHOW_IN_ROUTES,
        label: "Show in Endpoints",
        icon: "web",
        run: () => showInRoutes(mark.routes[0]!),
      },
    ];
  };
  return [
    marksField,
    follower,
    intentionProviders.of(intentions),
    gutter({
      class: `${styles.runGutter} cm-openidea-gutter-rails`,
      markers: (view) =>
        RangeSet.of(
          view.state.field(marksField).map((m) => new RouteMarker(m).range(m.pos)),
          true,
        ),
      domEventHandlers: {
        mousedown: (view, block, event) => {
          const mark = view.state.field(marksField).find((m) => m.pos === block.from);
          if (!mark || (event as MouseEvent).button !== 0) return false;
          const el = (event.target as HTMLElement).closest?.(`.${styles.runMarker}`) as HTMLElement | null;
          const rect = (el ?? (event.target as HTMLElement)).getBoundingClientRect();
          event.preventDefault();
          showContextMenu({ x: rect.left, y: rect.bottom + 2 }, routeMenuItems(mark.routes));
          return true;
        },
      },
    }),
  ];
}
