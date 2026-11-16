import ReactDOM from "react-dom/client";
import "./styles/index.css";
import "@/domains/notch/Notch.css";
import { NotchHud } from "@/domains/notch/NotchHud";
import { installPressFeedback } from "@/shared/motion/pressFeedback";

installPressFeedback();

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(<NotchHud />);
