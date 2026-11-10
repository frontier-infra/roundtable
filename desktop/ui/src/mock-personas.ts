// The persona packs for the browser preview and the tests. Loaded only in dev and test
// builds, so the app bundle never carries them (the engine is the app's source of packs).

import type { Pack, PersonaLibrary } from "./api";

export async function builtinPacks(): Promise<Pack[]> {
  if (!import.meta.env.DEV) return [];
  const mods = await Promise.all([
    import("../../../personas/frontier-labs.json"),
    import("../../../personas/business-roles.json"),
    import("../../../personas/expert-panels.json"),
  ]);
  return mods.map((m) => {
    const p = m.default as unknown as Partial<Pack>;
    return { ...p, description: p.description ?? "", presets: p.presets ?? [], builtin: true } as Pack;
  });
}

export function libraryOf(packs: Pack[], showLabs: boolean, built = true): PersonaLibrary {
  const hidden = !built || !showLabs;
  const visible = hidden
    ? packs.filter((p) => !p.labs).map((p) => ({ ...p, personas: p.personas.filter((x) => !x.lab) }))
    : packs;
  return { packs: visible, lab_pack_built: built, labs_hidden: hidden, custom_pack: "custom" };
}
