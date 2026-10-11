import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { BaliseProvider } from "@balise-rs/react";
import { App } from "./App";

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <BaliseProvider>
      <App />
    </BaliseProvider>
  </StrictMode>,
);
