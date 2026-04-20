import React from "react";
import ReactDOM from "react-dom/client";

// Fonts: bundled locally for offline use.
import "@fontsource/inter/400.css";
import "@fontsource/inter/500.css";
import "@fontsource/inter/600.css";
import "@fontsource/inter/700.css";
import "@fontsource/lora/400.css";
import "@fontsource/lora/600.css";
import "@fontsource/jetbrains-mono/400.css";
import "@fontsource/jetbrains-mono/500.css";

// Design tokens + app CSS.
import "@/design/colors_and_type.css";
import "@/design/app.css";
import "@/styles.css";

import { App } from "@/App";

const root = document.getElementById("root");
if (!root) throw new Error("Root element not found");

ReactDOM.createRoot(root).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
