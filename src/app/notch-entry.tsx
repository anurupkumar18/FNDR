import ReactDOM from "react-dom/client";
import "./styles/index.css";
import "@/domains/notch/Notch.css";
import { NotchHud } from "@/domains/notch/NotchHud";

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(<NotchHud />);
