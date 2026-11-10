// Credits: who made Roundtable, and where to find them. Roundtable is free.
// Every link here is checked by scripts/check-links.mjs and opens in the default browser.

import type { App } from "./app";
import { h } from "./dom";

export const ROUNDTABLE_URL = "https://roundtable.sh";
export const TITANIUM_URL = "https://titaniumcomputing.com/?utm_source=roundtable&utm_medium=app&utm_campaign=about";
export const JASON_URL = "https://titaniumcomputing.com/jason-brashear/?utm_source=roundtable&utm_medium=app";
export const TITANIUM_EXPORT_URL = "https://titaniumcomputing.com/?utm_source=roundtable&utm_medium=export";

export const MADE_BY = "Roundtable is free. Made by Frontier Infra, built by Titanium Computing.";

/** The one line at the end of a Markdown export. */
export const EXPORT_CREDIT = `Made with Roundtable (${ROUNDTABLE_URL}), free from Titanium Computing (${TITANIUM_EXPORT_URL})`;

/** A verdict or session as saved or copied: the text, then the credit line unless it is turned off. */
export function withCredit(markdown: string, on: boolean | undefined): string {
  if (on === false || markdown.includes(EXPORT_CREDIT)) return markdown;
  return `${markdown.replace(/\s+$/, "")}\n\n---\n\n${EXPORT_CREDIT}\n`;
}

/** A link that opens in the default browser, never inside the app window. */
export function outLink(app: App, url: string, text: string, cls?: string): HTMLAnchorElement {
  return h("a", {
    href: url,
    text,
    class: cls,
    on: {
      click: (e) => {
        e.preventDefault();
        void app.api.openUrl(url);
      },
    },
  });
}

/** Settings, About. */
export function aboutSection(app: App): HTMLElement[] {
  return [
    h("p", { class: "about-version", text: `Roundtable ${app.state.version || "unknown"}` }),
    h("p", { text: MADE_BY }),
    h(
      "ul",
      { class: "credit-links" },
      h("li", null, "Need AI in your business? ", outLink(app, TITANIUM_URL, "Titanium Computing")),
      h("li", null, outLink(app, JASON_URL, "Built by Jason Brashear, CTO")),
    ),
  ];
}

/** The one-line footer of the main screen. */
export function mainFooter(app: App): HTMLElement {
  return h("footer", { class: "credit-footer small" }, "Free from ", outLink(app, TITANIUM_URL, "Titanium Computing"));
}

/** The last screen of setup. */
export function setupCredit(app: App): HTMLElement {
  return h("p", { class: "credit-line small" }, outLink(app, TITANIUM_URL, "Free from Titanium Computing"));
}
