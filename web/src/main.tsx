import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import "./index.css";
import { App } from "./App";
import { RouterProvider } from "./lib/router";
import { AppStateProvider, applyTheme, readTheme } from "./lib/app-state";
import { TooltipProvider } from "./components/ui/tooltip";

// Apply the saved theme before the first paint to avoid a flash.
applyTheme(readTheme());

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <RouterProvider>
      <AppStateProvider>
        <TooltipProvider delayDuration={300}>
          <App />
        </TooltipProvider>
      </AppStateProvider>
    </RouterProvider>
  </StrictMode>,
);
