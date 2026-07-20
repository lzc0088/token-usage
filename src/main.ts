import "./app.css";
import App from "./App.svelte";
import { mount } from "svelte";

// Add Google Fonts for design consistency with wireframe
const link = document.createElement("link");
link.href = "https://fonts.googleapis.com/css2?family=Fraunces:opsz,wght@9..144,300;9..144,400;9..144,500;9..144,600&family=JetBrains+Mono:wght@400;500;600&family=Hanken+Grotesk:wght@300;400;500;600;700&display=swap";
link.rel = "stylesheet";
document.head.appendChild(link);

const app = mount(App, {
  target: document.getElementById("app")!,
});

export default app;
