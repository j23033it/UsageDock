import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { App } from "./App";
import "./styles.css";

const root = document.getElementById("root");

if (!root) {
  throw new Error("アプリの描画先が見つかりません。");
}

document.body.dataset.view = new URLSearchParams(window.location.search).get("view") === "settings" ? "settings" : "widget";

createRoot(root).render(
  <StrictMode>
    <App />
  </StrictMode>,
);

