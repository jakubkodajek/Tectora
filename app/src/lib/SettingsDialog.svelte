<script lang="ts">
  import { getVersion } from "@tauri-apps/api/app";
  import { open } from "@tauri-apps/plugin-dialog";
  import { onMount } from "svelte";
  import Icon from "./Icon.svelte";
  import Modal from "./Modal.svelte";
  import { api } from "./api";
  import { mayLeave } from "./leave";
  import { LOCALES, i18n, t } from "./i18n.svelte";
  import { app, loadSettings, presets, prefs, updateSettings } from "./settings.svelte";
  import { ui, type Theme } from "./theme.svelte";

  let { onclose, onlibrarychanged }: { onclose: () => void; onlibrarychanged: () => void } = $props();

  type Field = "projects_dir" | "drawings_dir" | "locale" | "theme" | "roof_defaults" | "follow_ai";

  let version = $state("");
  /** Error code per field from the last failed save. */
  let errors = $state<Partial<Record<Field, string>>>({});
  let busy = $state<Field | null>(null);

  const view = $derived(app.view);
  const eff = $derived(view?.effective);

  onMount(() => {
    getVersion().then((v) => (version = v)).catch(() => {});
    if (!app.view) loadSettings().catch(() => {});
  });

  async function save(field: Field, value: unknown) {
    // The open job is saved into the folder it came from before the folder
    // changes; edits that cannot be saved are left only when the user says so.
    if (field === "projects_dir" && !(await mayLeave())) return;
    busy = field;
    const shown = errors[field];
    try {
      const before = app.view?.effective.projects_dir;
      const next = await updateSettings({ [field]: value });
      // Keep an error that came up for another value while this one was saving.
      if (errors[field] === shown) delete errors[field];
      if (next.effective.projects_dir !== before) onlibrarychanged();
    } catch (e) {
      errors[field] = String(e);
    } finally {
      busy = null;
    }
  }

  async function pick(field: "projects_dir" | "drawings_dir") {
    const current = field === "projects_dir" ? eff?.projects_dir : eff?.drawings_dir;
    const dir = await open({ directory: true, multiple: false, defaultPath: current });
    if (typeof dir === "string") await save(field, dir);
  }

  function saveDefault(key: keyof typeof presets, raw: string, min: number, max: number) {
    const v = Number(raw.replace(",", "."));
    if (!Number.isFinite(v) || v < min || v > max) {
      errors.roof_defaults = "invalid_value";
      return;
    }
    save("roof_defaults", { [key]: v });
  }

  const errText = (code: string) => t(`settings.err.${code}`) === `settings.err.${code}` ? t("settings.err.io") : t(`settings.err.${code}`);

  const THEMES: Theme[] = ["system", "light", "dark"];
</script>

<Modal title={t("settings.title")} width={680} {onclose}>
  <div class="settings">
    <section>
      <h3>{t("settings.folders")}</h3>

      <div class="row">
        <div class="label">
          <span>{t("settings.projectsDir")}</span>
          <small>{t("settings.projectsDirHint")}</small>
        </div>
        <div class="control">
          <code class="path" title={eff?.projects_dir}>{eff?.projects_dir ?? "…"}</code>
          <div class="buttons">
            <button class="btn" onclick={() => pick("projects_dir")} disabled={!!eff?.projects_dir_from_env || busy !== null}>
              {t("settings.change")}
            </button>
            <button class="btn ghost" onclick={() => api.openFolder("projects")} title={t("settings.openFolder")}>
              <Icon name="folder" size={15} />{t("settings.open")}
            </button>
            {#if view?.settings.projects_dir && !eff?.projects_dir_from_env}
              <button class="btn ghost" onclick={() => save("projects_dir", null)} disabled={busy !== null}>{t("settings.reset")}</button>
            {/if}
          </div>
          {#if eff?.projects_dir_from_env}
            <small class="note">{t("settings.fromEnv", { name: "TECTORA_PROJECTS_DIR" })}</small>
          {/if}
          {#if errors.projects_dir}<small class="err" role="alert">{errText(errors.projects_dir)}</small>{/if}
        </div>
      </div>

      <div class="row">
        <div class="label">
          <span>{t("settings.drawingsDir")}</span>
          <small>{t("settings.drawingsDirHint")}</small>
        </div>
        <div class="control">
          <code class="path" title={eff?.drawings_dir}>{eff?.drawings_dir ?? "…"}</code>
          <div class="buttons">
            <button class="btn" onclick={() => pick("drawings_dir")} disabled={busy !== null}>{t("settings.change")}</button>
            <button class="btn ghost" onclick={() => api.openFolder("drawings")} title={t("settings.openFolder")}>
              <Icon name="folder" size={15} />{t("settings.open")}
            </button>
            {#if view?.settings.drawings_dir}
              <button class="btn ghost" onclick={() => save("drawings_dir", null)} disabled={busy !== null}>{t("settings.reset")}</button>
            {/if}
          </div>
          {#if errors.drawings_dir}<small class="err" role="alert">{errText(errors.drawings_dir)}</small>{/if}
        </div>
      </div>
    </section>

    <section>
      <h3>{t("settings.appearance")}</h3>
      <div class="row">
        <div class="label"><span>{t("lang")}</span></div>
        <div class="control">
          <select value={i18n.locale} onchange={(e) => save("locale", (e.target as HTMLSelectElement).value)} aria-label={t("lang")}>
            {#each LOCALES as l}<option value={l.id}>{l.label}</option>{/each}
          </select>
        </div>
      </div>
      <div class="row">
        <div class="label"><span>{t("settings.theme")}</span></div>
        <div class="control">
          <div class="seg" role="radiogroup" aria-label={t("settings.theme")}>
            {#each THEMES as th}
              <button role="radio" aria-checked={ui.theme === th} class:on={ui.theme === th} onclick={() => save("theme", th)}>
                {t(`settings.theme.${th}`)}
              </button>
            {/each}
          </div>
        </div>
      </div>
    </section>

    <section>
      <h3>{t("settings.newRoofs")}</h3>
      <p class="hint">{t("settings.newRoofsHint")}</p>
      <div class="grid3">
        <label>
          <span>{t("edge.pitch")}</span>
          <div class="unit">
            <input type="number" min="1" max="89" step="0.5" value={presets.pitch_deg} onchange={(e) => saveDefault("pitch_deg", (e.target as HTMLInputElement).value, 1, 89)} />
            <em>°</em>
          </div>
        </label>
        <label>
          <span>{t("edge.eaveHeight")}</span>
          <div class="unit">
            <input type="number" min="-100" max="1000" step="0.05" value={presets.eave_height} onchange={(e) => saveDefault("eave_height", (e.target as HTMLInputElement).value, -100, 1000)} />
            <em>m</em>
          </div>
        </label>
        <label>
          <span>{t("edge.overhang")}</span>
          <div class="unit">
            <input type="number" min="0" max="20" step="0.05" value={presets.overhang} onchange={(e) => saveDefault("overhang", (e.target as HTMLInputElement).value, 0, 20)} />
            <em>m</em>
          </div>
        </label>
      </div>
      {#if errors.roof_defaults}<small class="err" role="alert">{errText(errors.roof_defaults)}</small>{/if}
    </section>

    <section>
      <h3>{t("settings.ai")}</h3>
      <label class="check">
        <input type="checkbox" checked={prefs.followAi} onchange={(e) => save("follow_ai", (e.target as HTMLInputElement).checked)} />
        <span>
          {t("settings.followAi")}
          <small>{t("settings.followAiHint")}</small>
        </span>
      </label>
    </section>

    <section class="about">
      <h3>{t("settings.about")}</h3>
      <dl>
        <dt>{t("settings.version")}</dt>
        <dd><code>{version || "–"}</code></dd>
        <dt>{t("settings.file")}</dt>
        <dd>
          <code class="path" title={eff?.settings_file}>{eff?.settings_file ?? "…"}</code>
          <button class="icon-btn" onclick={() => api.openFolder("settings")} title={t("settings.openFolder")} aria-label={t("settings.openFolder")}>
            <Icon name="folder" size={15} />
          </button>
        </dd>
      </dl>
    </section>
  </div>
</Modal>

<style>
  .settings {
    display: flex;
    flex-direction: column;
    gap: 4px;
    user-select: text;
  }
  section {
    padding: 14px 0;
    border-top: 1px solid var(--border);
  }
  section:first-child {
    border-top: none;
    padding-top: 4px;
  }
  h3 {
    margin: 0 0 10px;
    font-size: 11px;
    font-weight: 650;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: var(--text-3);
  }
  .row {
    display: grid;
    grid-template-columns: minmax(150px, 1fr) 2fr;
    gap: 6px 20px;
    align-items: start;
    padding: 6px 0;
  }
  .label {
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding-top: 7px;
  }
  .label span {
    font-weight: 550;
  }
  small {
    font-size: 12px;
    color: var(--text-2);
    line-height: 1.35;
  }
  .control {
    display: flex;
    flex-direction: column;
    gap: 8px;
    min-width: 0;
  }
  .control select {
    align-self: flex-start;
    min-width: 180px;
  }
  .path {
    display: block;
    padding: 7px 10px;
    border-radius: 8px;
    background: var(--surface-2);
    font: 12px/1.4 ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
    color: var(--text);
    overflow-wrap: anywhere;
  }
  .buttons {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }
  .buttons .btn {
    height: 30px;
    padding: 0 12px;
    font-size: 13px;
  }
  .note {
    color: var(--text-3);
  }
  .err {
    font: 12px/1.4 ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
    color: var(--danger);
  }
  .seg {
    display: inline-flex;
    align-self: flex-start;
    padding: 2px;
    border: 1px solid var(--border);
    border-radius: 8px;
    background: var(--surface-2);
  }
  .seg button {
    height: 28px;
    padding: 0 14px;
    border: none;
    border-radius: 6px;
    background: transparent;
    color: var(--text-2);
    font-size: 13px;
  }
  .seg button.on {
    background: var(--surface);
    color: var(--text);
    box-shadow: var(--shadow);
    font-weight: 550;
  }
  .hint {
    margin: -4px 0 10px;
    font-size: 12px;
    color: var(--text-2);
  }
  .grid3 {
    display: grid;
    grid-template-columns: repeat(3, minmax(0, 1fr));
    gap: 12px;
  }
  .grid3 label {
    display: flex;
    flex-direction: column;
    gap: 4px;
    font-size: 12px;
    color: var(--text-2);
  }
  .unit {
    position: relative;
  }
  .unit input {
    width: 100%;
    padding-right: 28px;
  }
  .unit em {
    position: absolute;
    right: 10px;
    top: 50%;
    transform: translateY(-50%);
    font-style: normal;
    color: var(--text-3);
    pointer-events: none;
  }
  .check {
    display: flex;
    gap: 10px;
    align-items: flex-start;
    cursor: pointer;
  }
  .check input {
    margin-top: 3px;
  }
  .check span {
    display: flex;
    flex-direction: column;
    gap: 2px;
    font-weight: 550;
  }
  .check small {
    font-weight: 400;
  }
  .about dl {
    display: grid;
    grid-template-columns: minmax(150px, 1fr) 2fr;
    gap: 8px 20px;
    margin: 0;
    align-items: center;
  }
  .about dt {
    color: var(--text-2);
  }
  .about dd {
    display: flex;
    align-items: center;
    gap: 4px;
    margin: 0;
    min-width: 0;
  }
  .about dd .path {
    flex: 1;
    min-width: 0;
  }
  .about code:not(.path) {
    font: 12px ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
  }

  @media (max-width: 560px) {
    .row,
    .about dl {
      grid-template-columns: 1fr;
    }
    .label {
      padding-top: 0;
    }
    .grid3 {
      grid-template-columns: 1fr;
    }
  }
</style>
