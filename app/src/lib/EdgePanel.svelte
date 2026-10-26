<script lang="ts">
  import type { EdgeDef, RoofPart } from "./types";
  import { GABLE_PITCH, isGable } from "./types";

  let {
    part,
    index,
    onchange,
    onapplyall,
    onlength,
    ondelete,
  }: {
    part: RoofPart;
    index: number;
    onchange: (patch: Partial<EdgeDef>) => void;
    onapplyall: (patch: Partial<EdgeDef>) => void;
    onlength: (len: number) => void;
    ondelete: () => void;
  } = $props();

  const edge = $derived(part.edges[index]);
  const next = $derived(part.edges[(index + 1) % part.edges.length]);
  const length = $derived(Math.hypot(next.start.x - edge.start.x, next.start.y - edge.start.y));
  const gable = $derived(isGable(edge));

  // Remembered pitch so toggling gable on and off restores it.
  let lastPitch = 40;

  function num(e: Event): number | null {
    const v = parseFloat((e.target as HTMLInputElement).value.replace(",", "."));
    return Number.isFinite(v) ? v : null;
  }

  function setType(toGable: boolean) {
    if (toGable === gable) return;
    if (toGable) {
      lastPitch = edge.pitch_deg;
      onchange({ pitch_deg: GABLE_PITCH });
    } else {
      onchange({ pitch_deg: lastPitch, half_hip: null });
    }
  }
</script>

<div class="panel">
  <div class="head">
    <h3>Hrana {index + 1}</h3>
    <span class="muted">{part.name}</span>
  </div>

  <div class="seg" role="radiogroup" aria-label="Typ hrany">
    <button class:on={!gable} role="radio" aria-checked={!gable} onclick={() => setType(false)}>Okap</button>
    <button class:on={gable} role="radio" aria-checked={gable} onclick={() => setType(true)}>Štít</button>
  </div>

  <div class="fields">
    <label>
      <span>Délka</span>
      <div class="unit"><input type="number" step="0.05" min="0.05" value={length.toFixed(2)}
        onchange={(e) => { const v = num(e); if (v && v > 0) onlength(v); }} /><em>m</em></div>
    </label>
    {#if !gable}
      <label>
        <span>Sklon</span>
        <div class="unit"><input type="number" step="1" min="1" max="89" value={edge.pitch_deg}
          onchange={(e) => { const v = num(e); if (v && v > 0 && v < 90) onchange({ pitch_deg: v }); }} /><em>°</em></div>
      </label>
    {/if}
    <label>
      <span>Výška okapu</span>
      <div class="unit"><input type="number" step="0.05" value={edge.eave_height}
        onchange={(e) => { const v = num(e); if (v !== null) onchange({ eave_height: v }); }} /><em>m</em></div>
    </label>
    <label>
      <span>Přesah</span>
      <div class="unit"><input type="number" step="0.05" min="0" value={edge.overhang}
        onchange={(e) => { const v = num(e); if (v !== null && v >= 0) onchange({ overhang: v }); }} /><em>m</em></div>
    </label>
  </div>

  {#if gable}
    <label class="check">
      <input type="checkbox" checked={!!edge.half_hip}
        onchange={(e) => onchange({ half_hip: (e.target as HTMLInputElement).checked ? { height: edge.eave_height + 2.5, pitch_deg: 50 } : null })} />
      Polovalba (Krüppelwalm)
    </label>
    {#if edge.half_hip}
      <div class="fields">
        <label>
          <span>Výška polovalby</span>
          <div class="unit"><input type="number" step="0.05" value={edge.half_hip.height}
            onchange={(e) => { const v = num(e); if (v !== null && edge.half_hip) onchange({ half_hip: { ...edge.half_hip, height: v } }); }} /><em>m</em></div>
        </label>
        <label>
          <span>Sklon polovalby</span>
          <div class="unit"><input type="number" step="1" min="1" max="89" value={edge.half_hip.pitch_deg}
            onchange={(e) => { const v = num(e); if (v && edge.half_hip) onchange({ half_hip: { ...edge.half_hip, pitch_deg: v } }); }} /><em>°</em></div>
        </label>
      </div>
      <p class="note">Polovalba se zatím ve výpočtu neprojevuje, doplníme ji podle algoritmu DACADu.</p>
    {/if}
  {/if}

  <div class="actions">
    {#if !gable}
      <button class="btn" onclick={() => onapplyall({ pitch_deg: edge.pitch_deg })}>Sklon na všechny okapy</button>
    {/if}
    <button class="btn" onclick={() => onapplyall({ eave_height: edge.eave_height, overhang: edge.overhang })}>Výšku a přesah na vše</button>
    <button class="btn ghost danger" disabled={part.edges.length <= 3} onclick={ondelete}>Smazat bod na začátku hrany</button>
  </div>
</div>

<style>
  .panel {
    display: flex;
    flex-direction: column;
    gap: 14px;
  }
  .head {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
  }
  h3 {
    margin: 0;
    font-size: 15px;
  }
  .seg {
    display: grid;
    grid-template-columns: 1fr 1fr;
    padding: 3px;
    border-radius: 9px;
    background: var(--surface-2);
  }
  .seg button {
    height: 30px;
    border: none;
    border-radius: 7px;
    background: transparent;
    font-weight: 500;
    color: var(--text-2);
  }
  .seg button.on {
    background: var(--surface);
    color: var(--text);
    box-shadow: var(--shadow);
  }
  .fields {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 10px;
  }
  label span {
    display: block;
    margin-bottom: 4px;
    font-size: 12px;
    color: var(--text-2);
  }
  .unit {
    position: relative;
  }
  .unit input {
    width: 100%;
    padding-right: 26px;
  }
  .unit em {
    position: absolute;
    right: 10px;
    top: 8px;
    font-style: normal;
    color: var(--text-3);
    font-size: 13px;
  }
  .check {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .note {
    margin: 0;
    font-size: 12px;
    color: var(--text-3);
  }
  .actions {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .actions .btn {
    justify-content: center;
  }
  .danger {
    color: var(--danger);
  }
</style>
