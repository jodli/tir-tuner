import fs from "node:fs";
import { JSDOM } from "jsdom";

const dom = new JSDOM("<!doctype html><html><body></body></html>");
globalThis.window = dom.window;
globalThis.document = dom.window.document;
Object.defineProperty(globalThis, "navigator", { value: dom.window.navigator, configurable: true });
globalThis.Element = dom.window.Element;
globalThis.SVGElement = dom.window.SVGElement;

const mermaid = (await import("mermaid")).default;
const html = fs.readFileSync(process.argv[2], "utf8");
const blocks = [...html.matchAll(/<pre class="mermaid">([\s\S]*?)<\/pre>/g)].map((m) => m[1].trim());
mermaid.initialize({ startOnLoad: false });

let failed = false;
for (let i = 0; i < blocks.length; i++) {
  try {
    await mermaid.parse(blocks[i]);
    console.log(`diagram ${i + 1}: OK`);
  } catch (e) {
    failed = true;
    console.log(`diagram ${i + 1}: PARSE ERROR`);
    console.log(e.message.split("\n").slice(0, 8).join("\n"));
    console.log("---");
  }
}
process.exit(failed ? 1 : 0);