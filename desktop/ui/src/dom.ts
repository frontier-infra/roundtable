// A tiny element builder. Text always goes in as text, never as HTML.

type Child = Node | string | number | null | undefined | false | Child[];

export interface Props {
  class?: string;
  text?: string;
  title?: string;
  id?: string;
  type?: string;
  value?: string;
  placeholder?: string;
  disabled?: boolean;
  checked?: boolean;
  href?: string;
  role?: string;
  rows?: number;
  for?: string;
  attrs?: Record<string, string>;
  on?: Partial<{ [K in keyof HTMLElementEventMap]: (e: HTMLElementEventMap[K]) => void }>;
}

export function h<K extends keyof HTMLElementTagNameMap>(tag: K, props?: Props | null, ...children: Child[]): HTMLElementTagNameMap[K] {
  const el = document.createElement(tag);
  if (props) {
    if (props.class) el.className = props.class;
    if (props.text !== undefined) el.textContent = props.text;
    if (props.title) el.title = props.title;
    if (props.id) el.id = props.id;
    if (props.role) el.setAttribute("role", props.role);
    if (props.href) el.setAttribute("href", props.href);
    if (props.for) el.setAttribute("for", props.for);
    if (props.type) el.setAttribute("type", props.type);
    if (props.placeholder) el.setAttribute("placeholder", props.placeholder);
    if (props.rows) el.setAttribute("rows", String(props.rows));
    if (props.value !== undefined) (el as unknown as HTMLInputElement).value = props.value;
    if (props.disabled) (el as unknown as HTMLButtonElement).disabled = true;
    if (props.checked) (el as unknown as HTMLInputElement).checked = true;
    for (const [k, v] of Object.entries(props.attrs ?? {})) el.setAttribute(k, v);
    for (const [k, fn] of Object.entries(props.on ?? {})) el.addEventListener(k, fn as EventListener);
  }
  append(el, children);
  return el;
}

function append(el: Node, children: Child[]) {
  for (const c of children) {
    if (c === null || c === undefined || c === false) continue;
    if (Array.isArray(c)) append(el, c);
    else if (c instanceof Node) el.appendChild(c);
    else el.appendChild(document.createTextNode(String(c)));
  }
}

export function button(label: string, onClick: () => void, cls = "btn", disabled = false): HTMLButtonElement {
  return h("button", { class: cls, type: "button", text: label, disabled, on: { click: () => onClick() } });
}

/** A switch row: label, optional hint, and a checkbox styled as a toggle. */
export function toggle(label: string, checked: boolean, onChange: (v: boolean) => void, hint?: string): HTMLElement {
  const id = "t-" + label.toLowerCase().replace(/[^a-z0-9]+/g, "-");
  return h(
    "label",
    { class: "switch-row", for: id },
    h("span", { class: "switch-text" }, h("span", { text: label }), hint ? h("small", { text: hint }) : null),
    h("input", {
      id,
      type: "checkbox",
      class: "switch",
      checked,
      attrs: { role: "switch" },
      on: { change: (e) => onChange((e.target as HTMLInputElement).checked) },
    }),
  );
}
