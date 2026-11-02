/**
 * Ruby on Rails: the application's routes, read statically from
 * `config/routes.rb` (`ide-rails`), as a provider of the Endpoints tool
 * window, and, in routes files, a gutter icon per route-producing line that
 * generates `.http` requests.
 */
import { registerEditorContribution } from "@/editor/api";
import { registerActions } from "@/platform/actions";
import { registerMenuGroup } from "@/workbench/contributions";
import { registerEndpointProvider } from "../endpoints";
import {
  GENERATE_REQUEST,
  SHOW_IN_ENDPOINTS,
  generateRequests,
  menuGroupItems,
  railsRoutesEditorExtension,
  routesAtCaret,
  showInEndpoints,
} from "./editor";
import { railsEndpointProvider } from "./provider";
import { installRailsRoutes } from "./store";

const ROUTES_FILE_WHEN = "editorLangId == ruby";

export function activate(): () => void {
  const disposers = [
    installRailsRoutes(),
    registerEndpointProvider(railsEndpointProvider()),
    registerEditorContribution({ id: "rails.routes", order: 58, extensions: railsRoutesEditorExtension }),
    registerMenuGroup({ id: "rails.routes", menus: ["editor"], order: 41, items: (_menu, ctx) => menuGroupItems(ctx) }),
    registerActions([
      {
        id: GENERATE_REQUEST,
        title: "Generate Request in HTTP Client",
        category: "Tools",
        icon: "file-http",
        keywords: ["rails", "routes", "rest", "endpoint", "http client"],
        when: ROUTES_FILE_WHEN,
        run: () => generateRequests(routesAtCaret()),
      },
      {
        id: SHOW_IN_ENDPOINTS,
        title: "Show in Endpoints",
        category: "Tools",
        icon: "toolwindow-endpoints",
        keywords: ["rails", "routes"],
        when: ROUTES_FILE_WHEN,
        run: () => {
          const route = routesAtCaret()[0];
          if (route) showInEndpoints(route);
        },
      },
    ]),
  ];
  return () => disposers.reverse().forEach((d) => d());
}
