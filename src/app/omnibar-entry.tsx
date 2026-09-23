import ReactDOM from "react-dom/client";
import "./styles/index.css";
import "@/domains/omnibar/Omnibar.css";
import { OmnibarApp } from "@/domains/omnibar/OmnibarApp";
import { installAuxiliaryAppearanceSync } from "./auxiliaryAppearance";

installAuxiliaryAppearanceSync();

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
    <OmnibarApp />
);
