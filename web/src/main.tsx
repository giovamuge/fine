import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import "./styles.css";
import { App } from "./App";
import { RouterProvider } from "./lib/router";
import { ToastProvider } from "./lib/toast";
import { AppStateProvider, applyTheme, readTheme } from "./lib/app-state";

// Apply the saved theme before the first paint to avoid a flash.
applyTheme(readTheme());

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <RouterProvider>
      <ToastProvider>
        <AppStateProvider>
          <App />
        </AppStateProvider>
      </ToastProvider>
    </RouterProvider>
  </StrictMode>,
);
