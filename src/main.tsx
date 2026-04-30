import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import { applyPalette, isPaletteKey, type PaletteMode } from "./theme/cinematic-palettes";
import "./styles/index.css";

const storedTheme = localStorage.getItem("fndr-theme") as PaletteMode | null;
const theme = storedTheme === "light" ? "light" : "dark";
const storedPalette = localStorage.getItem("fndr-palette");

document.documentElement.setAttribute("data-theme", theme);
applyPalette(isPaletteKey(storedPalette) ? storedPalette : "matrix", theme);

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
    <React.StrictMode>
        <App />
    </React.StrictMode>
);
