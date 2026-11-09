import { mount } from "svelte";
import "./app.css";
import App from "./App.svelte";
import { installCommitBeforePress } from "./lib/commitfocus";

installCommitBeforePress();

export default mount(App, { target: document.getElementById("app")! });
