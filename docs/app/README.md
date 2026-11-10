# Roundtable app help

The help pages inside the Roundtable desktop app. One source: the app bundles these files, and roundtable.sh can import the same files.

## Files

- One Markdown file per page. The file name is the page's id (`getting-started.md` is `getting-started`).
- Front matter: `title`, `description`, `order` (the order in the app's list) and `online` (the page's twin on roundtable.sh).
- Pictures live in `img/`. Use real app screens only, about 1100 px wide.
- Link to another page by its file name: `[Connect your AIs](connect-your-ais.md)`. The app and Docusaurus both follow it.
- Plain Markdown only: headings, lists, paragraphs, bold, code, images and links. No tables, no HTML, no MDX.
- Plain consumer words. No em dashes.

## Adding a page

1. Add `<id>.md` with the front matter above.
2. To show it in the Help menu, add `("help:<id>", "<Title>")` to `HELP_PAGES` in `desktop/core/src/menu.rs`. A Rust test fails if a menu page has no file, and a UI test fails if a file has no title.
3. `node desktop/scripts/check-links.mjs` checks every `online` link.

The Deep Think lane adds `deep-think.md` and its menu line when Deep Think merges.

## How roundtable.sh imports these pages

The site is Docusaurus (`frontier-infra/roundtable.sh`). Add a second docs instance that reads this folder:

```ts
// docusaurus.config.ts, plugins
['@docusaurus/plugin-content-docs', {
  id: 'app',
  path: 'app-docs',          // this folder, checked out or synced at build time
  routeBasePath: 'docs/app',
  sidebarPath: false,
}],
```

A build step copies `docs/app` from this repo into `app-docs` (a git submodule or a `git archive` fetch). Docusaurus reads `title` and `description`; `order` maps to `sidebar_position`; `online` is ignored. Once the pages are live at `roundtable.sh/docs/app/<id>/`, point each page's `online` there.
