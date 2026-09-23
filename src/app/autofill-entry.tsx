import ReactDOM from "react-dom/client";
import "./styles/index.css";
import { AutofillOverlay } from "@/domains/workspace/AutofillOverlay";
import { installAuxiliaryAppearanceSync } from "./auxiliaryAppearance";

installAuxiliaryAppearanceSync();

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
    <AutofillOverlay />
);
