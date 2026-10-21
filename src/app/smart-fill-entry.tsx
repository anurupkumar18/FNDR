import ReactDOM from "react-dom/client";
import "./styles/index.css";
import { SmartFillGlow } from "@/domains/smart-fill/SmartFillGlow";
import { installAuxiliaryAppearanceSync } from "./auxiliaryAppearance";

installAuxiliaryAppearanceSync();

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(<SmartFillGlow />);
