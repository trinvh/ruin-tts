import {
  createHashHistory,
  createRootRoute,
  createRoute,
  createRouter,
} from "@tanstack/react-router";
import { RootLayout } from "./routes/root";
import { StudioPage } from "./routes/studio";
import { FlowsHome } from "./routes/flowsHome";
import { FlowsEditor } from "./routes/flowsEditor";
import { RunsPage } from "./routes/runs";
import { SettingsPage } from "./routes/settings";
import { ApiPage } from "./routes/apiInfo";

const rootRoute = createRootRoute({ component: RootLayout });

const studioRoute = createRoute({ getParentRoute: () => rootRoute, path: "/", component: StudioPage });
const flowsRoute = createRoute({ getParentRoute: () => rootRoute, path: "/flows", component: FlowsHome });
const flowsEditorRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/flows/$id",
  component: FlowsEditor,
});
const runsRoute = createRoute({ getParentRoute: () => rootRoute, path: "/runs", component: RunsPage });
const settingsRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/settings",
  component: SettingsPage,
});
const apiRoute = createRoute({ getParentRoute: () => rootRoute, path: "/api", component: ApiPage });

const routeTree = rootRoute.addChildren([
  studioRoute,
  flowsRoute,
  flowsEditorRoute,
  runsRoute,
  settingsRoute,
  apiRoute,
]);

export const router = createRouter({
  routeTree,
  history: createHashHistory(),
  defaultPreload: "intent",
});

declare module "@tanstack/react-router" {
  interface Register {
    router: typeof router;
  }
}
