import { mount } from "svelte";
import "../../src/index.css";
import Fixture from "./uiControls.fixture.svelte";
const target = document.getElementById("root");
if (target) mount(Fixture, { target });
