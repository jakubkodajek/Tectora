<script lang="ts">
  import { info, notify, notifyError } from "./notify.svelte";
  import { open, save } from "@tauri-apps/plugin-dialog";
  import { untrack } from "svelte";
  import Icon from "./Icon.svelte";
  import { api } from "./api";
  import { safeFileName } from "./filename";
  import { t } from "./i18n.svelte";
  import type { Attachment, JobStatus, Project, ResultSnapshot } from "./types";
  import { JOB_STATUSES } from "./types";
  import { DACAD_MATERIALS, defaultMaterialName } from "./materials";
  import { textfield } from "./textfield";
  import { fileSize, m2 } from "./format";
  import { hasBuilding, snapshotDifferences } from "./offer";
  import { figureName, formatFigure } from "./changes";
  import { exclusive } from "./inflight";
  import type { JobEdits, RoofHold } from "./jobedits";
  import type { FigureDifference } from "./types";

  /** Opens an attached file; files that are not drawings or documents are only shown in their folder. */
  async function openFile(name: string) {
    try {
      if (!(await api.openAttachment(id, name))) info(t("files.shownInFolder", { name }));
    } catch (e) {
      notifyError(e, "files");
    }
  }

  let {
    id,
    project,
    onchange,
    onsave,
    track = (p) => p,
    blocked = null,
    current,
    edits,
    recalculated = false,
    onresulttoast,
  }: {
    id: string;
    project: Project;
    onchange: () => void;
    /** Writes pending edits to disk (before an export); false when that failed. */
    onsave: () => Promise<boolean>;
    /** Registers work that changes the job once the core answers; leaving the job waits for it. */
    track?: (p: Promise<unknown>) => Promise<unknown>;
    /** Why the roof cannot be offered or ordered now (the core gave no roof), or null. */
    blocked?: string | null;
    /**
     * The stored result is the engine's recalculation of the offer's: a
     * bundle carries it without the offer's own figures, which stay here
     * (owner decision 2026-11-23, UX-1003); the export says so.
     */
    recalculated?: boolean;
    /** The project the editor holds now. The panel may be gone (the AI tab took the dock)
     *  when an awaited answer comes back, so its own prop can be the replaced project (ERR-102). */
    current?: () => Project | null;
    /** The editor's edit rule (jobedits.ts): a status change holds the roof it stores the result of. */
    edits: JobEdits;
    /**
     * A notice whose text quotes the stored result was shown: the editor
     * closes it once the stored result changes (the panel may be gone by
     * then), so it never stands with old figures (UI-1202).
     */
    onresulttoast?: (toast: number) => void;
  } = $props();

  /** Whether status `s` cannot be chosen now: any status past the draft while the core gives
   *  no roof for a building or leaves input out (ERR-003, UX-005). Done too, also with a stored
   *  result: the job now has a building the offer's figures do not contain (UX-103). */
  const refused = (s: JobStatus) => !!why && s !== status && s !== "draft";
  /** Why offer statuses are off: the core gave no roof, or there is no building at all (UX-102). */
  const why = $derived(blocked ?? (hasBuilding(project) ? null : t("res.empty")));

  /** The job as the editor holds it now: a status change writes through it, never through the
   *  unbound prop (UI-010, ERRU-004), as `setText` does for the texts. */
  const latest = (): Project | null => (current ? current() : project);

  // Leaving the draft status stores the result the offer is made with.
  let status = $state<JobStatus>(untrack(() => project.job.status));
  $effect(() => {
    // An AI save or an undo may change the status under the panel.
    const now = project.job.status;
    untrack(() => (status = now));
  });
  /** A status change is storing its result: the select is off and a second choice waits (jobedits.ts, one-shot). */
  let changing = $state(false);
  const changeStatus = exclusive(statusChangedNow, (b) => (changing = b));
  /** The status chosen last; while it is stored, the select shows it and takes no other. */
  let chosen: JobStatus = untrack(() => status);
  const statusChanged = (e: Event) => {
    const sel = e.currentTarget as HTMLSelectElement;
    // Busy, not disabled: a disabled select drops the keyboard focus (UI-402).
    if (changing) {
      sel.value = chosen;
      return;
    }
    chosen = sel.value as JobStatus;
    return track(changeStatus(sel.value as JobStatus, sel));
  };
  /** While a status is stored, keys and presses that would choose another do nothing (UI-402). */
  const holdWhileChanging = (e: Event) => {
    if (!changing) return;
    if (e instanceof KeyboardEvent && (e.key === "Tab" || e.key === "Escape")) return;
    e.preventDefault();
  };
  async function statusChangedNow(next: JobStatus, sel: HTMLSelectElement) {
    const was = status;
    /** The status stays as it is: the select shows it again. */
    const back = () => {
      const s = latest()?.job.status ?? was;
      sel.value = s;
      status = s;
    };
    if (refused(next)) {
      sel.value = was;
      return;
    }
    let snap: ResultSnapshot | null = null;
    let kept: ResultSnapshot | null = null;
    let fresh: ResultSnapshot | null = null;
    let roof: RoofHold | null = null;
    if (was === "draft" && next !== "draft") {
      // A value typed just before is part of the roof: wait for it.
      await edits.idle();
      const p = latest();
      if (!p) return;
      // A result stored before (the job went back to draft since) stays the
      // offer's; the user is told when the roof now gives other figures (UX-206).
      kept = p.result_snapshot ?? null;
      roof = edits.holdRoof();
      try {
        fresh = await api.takeSnapshot(JSON.parse(JSON.stringify(p)));
      } catch (e) {
        // No figures of the roof as it is now: the job stays a draft (DR-002),
        // also when it keeps an earlier result, which is not of this roof
        // (ERR-303). Not a failed save: the status was refused (UX-005).
        back();
        notifyError(e, "status");
        return;
      }
      // The roof changed while the core calculated, by an AI save (ERR-102)
      // or by the user's own edit (ERR-201): the figures are of a roof the
      // job no longer has, so the status is refused with a note. An AI save
      // of the notes or the job data leaves the roof as it was (ERR-301).
      if (!latest()) return;
      if (!edits.roofSame(roof)) {
        back();
        info(t("job.statusStale"));
        return;
      }
      if (!kept) snap = fresh;
    }
    const now = latest();
    if (!now) return;
    if (snap && !now.result_snapshot) now.result_snapshot = snap;
    now.job.status = next;
    status = next;
    onchange();
    if (kept && fresh && roof) {
      const differences = snapshotDifferences(kept, fresh);
      if (differences.length) offerKept(differences, fresh, roof);
    }
  }

  /** What differs, in words: the two areas, or the first other figure with its name (ERR-304). */
  function keptText(differences: FigureDifference[]): string {
    const d = differences[0];
    if (d.key === "roof_area") return t("job.offerKept", { a: m2(d.stored), b: m2(d.current) });
    return t("job.offerKeptOther", { what: figureName(d.key, latest()), a: formatFigure(d.key, d.stored), b: formatFigure(d.key, d.current) });
  }

  /** Says that the job keeps its earlier result, with the figure that differs and the action that stores the new one (UX-206). */
  function offerKept(differences: FigureDifference[], fresh: ResultSnapshot, roof: RoofHold) {
    const toast = notify({
      kind: "warning",
      title: t("job.offerKeptTitle"),
      message: keptText(differences),
      action: {
        label: t("snap.store"),
        run: () => {
          const now = latest();
          if (!now) return;
          // Only figures of the roof the job still has (as the Editor's store, ERR-102).
          if (!edits.roofSame(roof)) {
            info(t("snap.stale"));
            return;
          }
          now.result_snapshot = fresh;
          onchange();
        },
      },
    });
    onresulttoast?.(toast);
  }

  // Files attached to the job (drawings, photos).
  let files = $state<Attachment[]>([]);
  async function loadFiles() {
    try {
      files = await api.listAttachments(id);
    } catch (e) {
      notifyError(e, "files");
    }
  }
  $effect(() => {
    void id;
    untrack(loadFiles);
  });

  async function addFiles() {
    const sel = await open({ multiple: true });
    const paths = Array.isArray(sel) ? sel : sel ? [sel] : [];
    if (!paths.length) return;
    try {
      await api.attachFiles(id, paths);
    } catch (e) {
      notifyError(e, "files");
    }
    loadFiles();
  }

  /** Files with a removal under way: a double click asks once and removes once (UI-009). */
  let removing = $state<Record<string, boolean>>({});

  async function removeFile(name: string) {
    if (removing[name]) return;
    removing[name] = true;
    try {
      // Tauri's dialog plugin makes window.confirm asynchronous (a Promise is
      // always truthy), so the answer must be awaited.
      if (!(await confirm(t("files.confirmRemove", { name })))) return;
      try {
        await api.removeAttachment(id, name);
      } catch (e) {
        notifyError(e, "files");
      }
      await loadFiles();
    } finally {
      delete removing[name];
    }
  }

  async function exportJob() {
    const path = await save({
      defaultPath: `${safeFileName(project.name, "job")}.tectorajob`,
      filters: [{ name: "Tectora", extensions: ["tectorajob"] }],
    });
    if (!path) return;
    try {
      // A failed save keeps the edits only in memory: the bundle would hold
      // the older file (ERR-003). The save has shown its own error.
      if (!(await onsave())) {
        notifyError({ code: "not_saved", id }, "export");
        return;
      }
      await api.exportJob(id, path);
      // A warning that stays long enough to be read, naming the job and the
      // way to send the offer's own figures (UX-1101).
      if (recalculated) {
        const name = (current?.()?.name ?? project.name).trim();
        notify({
          kind: "warning",
          title: name ? t("snap.titleOf", { name }) : t("snap.title"),
          message: t("snap.exportRecalculated", { tab: t("dock.results"), button: t("snap.revertRefresh") }),
        });
      }
    } catch (e) {
      notifyError(e, "export");
    }
  }


  const ids = Array.from({ length: DACAD_MATERIALS }, (_, i) => i + 1);

  /** Stores a typed text into the job as it is now (an AI save may have replaced it while typing). */
  function setText(apply: (p: Project, v: string) => void) {
    return (v: string) => {
      apply(project, v);
      onchange();
    };
  }

  function setName(m: number, v: string) {
    const names = [...(project.material_names ?? [])];
    while (names.length < m) names.push("");
    names[m - 1] = v;
    while (names.length && !names[names.length - 1]) names.pop();
    project.material_names = names;
    onchange();
  }
</script>

<div class="job">
  <label><span>{t("job.name")}</span><input type="text" use:textfield={{ value: project.name, required: true, onchange: setText((p, v) => (p.name = v)) }} /></label>
  <div class="two">
    <label><span>{t("job.number")}</span><input type="text" use:textfield={{ value: project.job.number, onchange: setText((p, v) => (p.job.number = v)) }} /></label>
    <label>
      <span>{t("job.status")}</span>
      <select value={status} onchange={statusChanged} onkeydown={holdWhileChanging} onmousedown={holdWhileChanging} title={why ?? undefined} aria-disabled={changing} aria-busy={changing}>
        {#each JOB_STATUSES as s}<option value={s} disabled={refused(s)}>{t(`status.${s}`)}</option>{/each}
      </select>
    </label>
  </div>
  <label><span>{t("job.customer")}</span><input type="text" use:textfield={{ value: project.job.customer, onchange: setText((p, v) => (p.job.customer = v)) }} /></label>
  <label><span>{t("job.address")}</span><input type="text" use:textfield={{ value: project.job.address, onchange: setText((p, v) => (p.job.address = v)) }} /></label>
  <label><span>{t("job.notes")}</span><textarea rows="5" use:textfield={{ value: project.notes, raw: true, onchange: setText((p, v) => (p.notes = v)) }}></textarea></label>
  <h4>{t("files.title")}</h4>
  {#if files.length}
    <ul class="files">
      {#each files as f (f.name)}
        <li>
          <button class="link" onclick={() => openFile(f.name)} title={t("files.open")}>{f.name}</button>
          <span class="size">{fileSize(f.size)}</span>
          <button class="icon" disabled={!!removing[f.name]} onclick={() => removeFile(f.name)} title={t("files.remove")} aria-label={t("files.remove")}><Icon name="trash" size={14} /></button>
        </li>
      {/each}
    </ul>
  {:else}
    <p class="hint">{t("files.empty")}</p>
  {/if}
  <div class="row">
    <button class="btn sm" onclick={addFiles}><Icon name="plus" size={14} />{t("files.add")}</button>
    <button class="btn sm" onclick={exportJob} title={t("files.exportHint")}><Icon name="download" size={14} />{t("files.export")}</button>
  </div>
  <h4 title={t("job.materialsHint")}>{t("job.materials")}</h4>
  {#each ids as m}
    <label class="mat">
      <span>{m}</span>
      <input type="text" placeholder={defaultMaterialName(m)}
        use:textfield={{ value: project.material_names?.[m - 1] ?? "", onchange: (v) => setName(m, v) }} />
    </label>
  {/each}
</div>

<style>
  .job {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .two {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 10px;
  }
  label span {
    display: block;
    margin-bottom: 3px;
    font-size: 12px;
    color: var(--text-2);
  }
  input,
  select,
  textarea {
    width: 100%;
  }
  h4 {
    margin: 6px 0 0;
    font-size: 12px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: var(--text-2);
  }
  .files {
    margin: 0;
    padding: 0;
    list-style: none;
    font-size: 12.5px;
  }
  .files li {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 4px 0;
    border-bottom: 1px solid var(--border);
  }
  .link {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    text-align: left;
    border: 0;
    padding: 0;
    background: none;
    color: var(--text);
    cursor: pointer;
  }
  .link:hover {
    text-decoration: underline;
  }
  .size {
    color: var(--text-3);
    font-variant-numeric: tabular-nums;
  }
  .icon {
    border: 0;
    padding: 2px;
    background: none;
    color: var(--text-3);
    cursor: pointer;
  }
  .icon:hover {
    color: var(--danger);
  }
  .hint {
    margin: 0;
    font-size: 12px;
    color: var(--text-3);
  }
  .row {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }
  .row .btn {
    white-space: nowrap;
  }
  .mat {
    display: grid;
    grid-template-columns: 18px 1fr;
    align-items: center;
    gap: 6px;
  }
  .mat span {
    margin: 0;
    text-align: right;
  }
</style>
