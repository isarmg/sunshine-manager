import { StrictMode } from "react";
import { createRoot } from "react-dom/client";

import "@sarmg/design-tokens/tokens.css";
import "@sarmg/design-tokens/reset.css";
import "@sarmg/design-tokens/accessibility.css";

import App from "./App";

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <App />
  </StrictMode>,
);
