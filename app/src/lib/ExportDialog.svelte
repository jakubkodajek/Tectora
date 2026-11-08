<script lang="ts">
  // Export of a finished roof: the plan sheet (dimensions, areas, pitches)
  // as PDF, PNG or image on the clipboard, and the text list that the price
  // sheet in Excel reads, with a copy button. Both come from
  // tectora_core::export, so the MCP tools export_plan and export_list give
  // the same output.
  import { save } from "@tauri-apps/plugin-dialog";
  import Modal from "./Modal.svelte";
  import Icon from "./Icon.svelte";
  import { api } from "./api";
  import { t } from "./i18n.svelte";
  import { notify, notifyError } from "./notify.svelte";
  import type { Project } from "./types";

  let { project, title, onclose }: { project: Project; title: string; onclose: () => void } = $props();

  /** Local time as `dd.mm.yyyy hh:mm`, the stamp the list and sheet carry. */
  function stampNow(): string {
    const d = new Date();
    const p = (n: number) => String(n).padStart(2, "0");
    return `${p(d.getDate())}.${p(d.getMonth() + 1)}.${d.getFullYear()} ${p(d.getHours())}:${p(d.getMinutes())}`;
  }

  const stamp = stampNow();
  let list = $state("");
  let svg = $state("");
  let failed = $state(false);
  let copied = $state<"list" | "image" | null>(null);
  let copiedTimer: ReturnType<typeof setTimeout> | undefined;

  $effect(() => {
    api
      .exportPreview($state.snapshot(project) as Project, title, stamp)
      .then((r) => {
        list = r.list;
        svg = r.svg;
      })
      .catch((e) => {
        failed = true;
        notifyError(e, "export");
      });
    return () => clearTimeout(copiedTimer);
  });

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
      await api.exportPlanFile($state.snapshot(project) as Project, stamp, path);
      notify({ kind: "info", title: t("export.saved"), message: path });
    } catch (e) {
      notifyError(e, "export");
    }
  }
</script>

<Modal title={t("export.title")} width={980} {onclose}>
  <section class="sheet">
    <div class="head">
      <h3>{t("export.plan")}</h3>
      <div class="actions">
        <button class="btn sm" disabled={!svg} onclick={copyImage} title={t("export.copyImageHint")}>
          <Icon name={copied === "image" ? "check" : "copy"} size={14} />{copied === "image" ? t("export.copied") : t("export.copyImage")}
        </button>
        <button class="btn sm" disabled={!svg} onclick={savePng}><Icon name="download" size={14} />PNG</button>
        <button class="btn sm primary" disabled={!svg} onclick={savePdf}><Icon name="download" size={14} />PDF</button>
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

  <section class="list">
    <div class="head">
      <h3>{t("export.list")}</h3>
      <button class="btn sm" disabled={!list} onclick={copyList} title={t("export.copyListHint")}>
        <Icon name={copied === "list" ? "check" : "copy"} size={14} />{copied === "list" ? t("export.copied") : t("export.copyList")}
      </button>
    </div>
    <p class="muted">{t("export.listHint")}</p>
    <pre>{list}</pre>
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
