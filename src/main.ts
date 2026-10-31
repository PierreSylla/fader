import { mount } from "svelte";
import "../docs/design/fader-tokens.css";
import App from "./App.svelte";

const target = document.getElementById("app");
if (!target) throw new Error("#app introuvable");

mount(App, { target });
