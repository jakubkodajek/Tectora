<script lang="ts">
  // Export of a finished roof: the plan sheet (dimensions, areas, pitches)
  // as PDF, PNG or image on the clipboard, and the text list that the price
  // sheet in Excel reads, with a copy button. Both come from
  // tectora_core::export, so the MCP tools export_plan and export_list give
  // the same output.
  import { save } from "@tauri-apps/plugin-dialog";
  import Modal from "./Modal.svelte";
  import Icon from "./Icon.svelte";
  import { api, type ListSummary } from "./api";
  import { exportPreview, stampNow } from "./exportlist";
  import { numberLocale, t } from "./i18n.svelte";
  import { lineName, lineNames } from "./linecodes";
  import { materialName } from "./materials";
  import { notify, notifyError } from "./notify.svelte";
  import type { Project } from "./types";

  let { project, title, onclose }: { project: Project; title: string; onclose: () => void } = $props();

  const stamp = stampNow();
  let list = $state("");
  let svg = $state("");
  /** The list as numbers, shown translated; `list` stays the price sheet's text (UX-003). */
  let summary = $state<ListSummary | null>(null);
  let failed = $state(false);
  let copied = $state<"list" | "image" | null>(null);
  let copiedTimer: ReturnType<typeof setTimeout> | undefined;

  $effect(() => {
    exportPreview($state.snapshot(project) as Project, title, stamp)
      .then((r) => {
        list = r.list;
        svg = r.svg;
        summary = r.summary ?? null;
      })
      .catch((e) => {
        failed = true;
        notifyError(e, "export");
      });
    return () => clearTimeout(copiedTimer);
  });

  /** A length or area with the list's decimals, in the UI language. */
  const fix = (v: number) =>
    new Intl.NumberFormat(numberLocale(), { minimumFractionDigits: summary?.decimals ?? 3, maximumFractionDigits: summary?.decimals ?? 3 }).format(
      Math.abs(v) < 5e-7 ? 0 : v,
    );
  const deg = (v: number) => `${new Intl.NumberFormat(numberLocale(), { minimumFractionDigits: 2, maximumFractionDigits: 2 }).format(v)}°`;

  const svgUrl = $derived(svg ? `data:image/svg+xml;charset=utf-8,${encodeURIComponent(svg)}` : "");
  const baseName = $derived((project.name || title || "roof").replace(/[\\/:*?"<>|]+/g, "_"));

  function flash(what: "list" | "image") {
    copied = what;
    clearTimeout(copiedTimer);
    copiedTimer = setTimeout(() => (copied = null), 1800);
  }

  async function copyList() {
    try {
      await navigator.clipboard.writeText(list);
      flash("list");
    } catch (e) {
      notifyError(e, "export");
    }
  }

  /** The sheet as PNG at 2× its A4 size in points (about 1684 × 1190 px). */
  async function png(): Promise<Blob> {
    const img = new Image();
    img.src = svgUrl;
    await img.decode();
    const canvas = document.createElement("canvas");
    canvas.width = Math.round(img.naturalWidth * 2);
    canvas.height = Math.round(img.naturalHeight * 2);
    const ctx = canvas.getContext("2d");
    if (!ctx) throw new Error("canvas 2d context unavailable");
    ctx.drawImage(img, 0, 0, canvas.width, canvas.height);
    return new Promise((ok, err) => canvas.toBlob((b) => (b ? ok(b) : err(new Error("PNG encoding failed"))), "image/png"));
  }

  async function copyImage() {
    try {
      // The ClipboardItem gets the promise so Safari (WKWebView) keeps the
      // user gesture while the PNG is rendered.
      await navigator.clipboard.write([new ClipboardItem({ "image/png": png() })]);
      flash("image");
    } catch (e) {
      notifyError(e, "export");
    }
  }

  async function savePng() {
    const path = await save({ defaultPath: `${baseName}.png`, filters: [{ name: "PNG", extensions: ["png"] }] });
    if (!path) return;
    try {
      const bytes = new Uint8Array(await (await png()).arrayBuffer());
      await api.savePng(path, Array.from(bytes));
      notify({ kind: "info", title: t("export.saved"), message: path });
    } catch (e) {
      notifyError(e, "export");
    }
  }

  async function savePdf() {
    const path = await save({ defaultPath: `${baseName}.pdf`, filters: [{ name: "PDF", extensions: ["pdf"] }] });
    if (!path) return;
    try {
      await api.exportPlanFile($state.snapshot(project) as Project, stamp, path, lineNames());
      notify({ kind: "info", title: t("export.saved"), message: path });
    } catch (e) {
      notifyError(e, "export");
    }
  }
</script>

<Modal title={t("export.title")} width={980} {onclose}>
  <!-- The text for the Excel price sheet is the main use: first, with the primary button (UX-004). -->
  <section class="list">
    <div class="head">
      <h3>{t("export.list")}</h3>
      <button class="btn sm primary" disabled={!list} onclick={copyList} title={t("export.copyListHint")}>
        <Icon name={copied === "list" ? "check" : "copy"} size={14} />{copied === "list" ? t("export.copied") : t("export.copyList")}
      </button>
    </div>
    <p class="muted">{t("export.listHint")}</p>
    {#if summary}
      <!-- What the copied text says, in the UI language; the text itself
           keeps the price sheet's words (UX-003). -->
      <div class="sum" data-testid="export-summary">
        <div>
          <table>
            <caption>{t("export.sumFaces")}</caption>
            <thead>
              <tr><th>{t("export.colFace")}</th><th class="v">{t("export.colPitch")}</th><th class="v">{t("export.colArea")}</th><th class="v">{t("export.colDeducted")}</th><th class="v">{t("export.colRafter")}</th></tr>
            </thead>
            <tbody>
              {#each summary.faces as f}
                <tr>
                  <td class="mono">{f.name}</td>
                  <td class="v">{deg(f.pitch)}</td>
                  <td class="v">{fix(f.area)} m²</td>
                  <td class="v">{f.deducted > 5e-7 ? `−${fix(f.deducted)} m²` : ""}</td>
                  <td class="v">{f.rafter != null ? `${fix(f.rafter)} m` : ""}</td>
                </tr>
              {/each}
            </tbody>
          </table>
          {#if summary.windows.length}
            <table>
              <caption>{t("export.sumWindows")}</caption>
              <tbody>
                {#each summary.windows as w}
                  <tr><td class="mono">{w.face}</td><td class="v">{fix(w.width)} × {fix(w.height)} m</td></tr>
                {/each}
              </tbody>
            </table>
          {/if}
        </div>
        <div>
          {#if summary.wall_area != null || summary.materials.length}
            <table>
              <caption>{t("export.sumAreas")}</caption>
              <tbody>
                {#if summary.wall_area != null}<tr><td>{t("export.wallArea")}</td><td class="v">{fix(summary.wall_area)} m²</td></tr>{/if}
                {#each summary.materials as mat}
                  <tr><td>{materialName(project, mat.material)}</td><td class="v">{fix(mat.area)} m²</td></tr>
                {/each}
              </tbody>
            </table>
          {/if}
          <table>
            <caption>{t("export.sumLines")}</caption>
            <tbody>
              {#each summary.lines as l}
                <tr><td class="mono code">{l.code}</td><td>{lineName(l.code)}</td><td class="v">{fix(l.length)} m</td></tr>
              {/each}
            </tbody>
          </table>
        </div>
      </div>
    {/if}
    <details class="raw" open={!summary}>
      <summary>{t("export.rawText")}</summary>
      <p class="muted">{t("export.rawNote")}</p>
      <pre data-testid="export-list">{list}</pre>
    </details>
  </section>

  <section class="sheet">
    <div class="head">
      <h3>{t("export.plan")}</h3>
      <div class="actions">
        <button class="btn sm" disabled={!svg} onclick={copyImage} title={t("export.copyImageHint")}>
          <Icon name={copied === "image" ? "check" : "copy"} size={14} />{copied === "image" ? t("export.copied") : t("export.copyImage")}
        </button>
        <button class="btn sm" disabled={!svg} onclick={savePng}><Icon name="download" size={14} />PNG</button>
        <button class="btn sm" disabled={!svg} onclick={savePdf}><Icon name="download" size={14} />PDF</button>
      </div>
    </div>
    <div class="paper">
      {#if svgUrl}
        <img src={svgUrl} alt={t("export.plan")} />
      {:else}
        <span class="muted">{failed ? t("export.failed") : "…"}</span>
      {/if}
    </div>
  </section>

  {#snippet footer()}
    <button class="btn" onclick={onclose}>{t("common.close")}</button>
  {/snippet}
</Modal>

<style>
  section + section {
    margin-top: 18px;
  }
  .head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    margin-bottom: 8px;
  }
  h3 {
    margin: 0;
    font-size: 13px;
    font-weight: 600;
    color: var(--text);
  }
  .actions {
    display: flex;
    gap: 6px;
  }
  .btn :global(svg) {
    margin-right: 4px;
  }
  .paper {
    border: 1px solid var(--border);
    border-radius: 8px;
    background: var(--surface-2);
    padding: 8px;
    display: grid;
    place-items: center;
    min-height: 200px;
  }
  .paper img {
    width: 100%;
    height: auto;
    display: block;
    box-shadow: var(--shadow);
  }
  .muted {
    color: var(--text-2);
    font-size: 12px;
    margin: 0 0 6px;
  }
  .sum {
    display: grid;
    grid-template-columns: minmax(0, 3fr) minmax(0, 2fr);
    gap: 8px 20px;
    max-height: 260px;
    overflow: auto;
    padding: 4px 2px;
    border-top: 1px solid var(--border);
    border-bottom: 1px solid var(--border);
  }
  @media (max-width: 760px) {
    .sum {
      grid-template-columns: minmax(0, 1fr);
    }
  }
  caption {
    text-align: left;
    padding: 8px 0 4px;
    font-size: 11.5px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: var(--text-2);
  }
  table {
    width: 100%;
    border-collapse: collapse;
    font-size: 12.5px;
  }
  th {
    font-weight: 500;
    font-size: 11.5px;
    color: var(--text-2);
    text-align: left;
    padding: 2px 6px 4px 0;
    white-space: nowrap;
  }
  td {
    padding: 3px 6px 3px 0;
    border-bottom: 1px solid var(--border);
  }
  .v {
    text-align: right;
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
  }
  .mono {
    font-family: ui-monospace, "SF Mono", Menlo, Consolas, monospace;
    font-size: 11.5px;
  }
  .code {
    width: 36px;
    font-weight: 700;
  }
  .raw {
    margin-top: 10px;
  }
  .raw summary {
    cursor: pointer;
    font-size: 12px;
    color: var(--text-2);
    margin-bottom: 6px;
  }
  .raw summary:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 2px;
  }
  pre {
    margin: 0;
    max-height: 260px;
    overflow: auto;
    padding: 10px 12px;
    border: 1px solid var(--border);
    border-radius: 8px;
    background: var(--surface-2);
    color: var(--text);
    font-family: ui-monospace, "SF Mono", Menlo, Consolas, monospace;
    font-size: 11.5px;
    line-height: 1.45;
    white-space: pre;
  }
</style>
