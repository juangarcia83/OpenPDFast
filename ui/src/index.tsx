// SPDX-License-Identifier: AGPL-3.0-or-later
import "@fontsource-variable/inter";
import "@fontsource-variable/jetbrains-mono";
import "./design/tokens.css";
import "./design/base.css";
import { render } from "solid-js/web";
import { App } from "./App";

const root = document.getElementById("root");
if (!root) throw new Error("#root element missing from index.html");
render(() => <App />, root);
