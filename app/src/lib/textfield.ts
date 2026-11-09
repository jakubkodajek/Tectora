// One behaviour for every text field that edits the job (Job tab, the job
// title), matching the number fields (numeric.svelte.ts):
//
// - commits on Enter (Ctrl/Cmd+Enter in a multi-line field), on blur, and so
//   on a press anywhere outside (commitfocus.ts blurs the field first);
// - Escape restores the stored value and leaves the field, storing nothing;
// - while the user types, outside updates (autosave, an AI save, a reload)
//   never replace the typed text, and the commit goes to the job as it is
//   then, not to the object the field was showing when typing began;
// - `required` refuses an empty value (shows the stored one again).

export interface TextFieldOptions {
  /** The stored value. */
  value: string | null | undefined;
  /** Called with the trimmed text when it differs from `value`; return `false` to refuse it. */
  onchange: (v: string) => unknown;
  /** An empty value is refused. */
  required?: boolean;
  /** Keep leading and trailing spaces (default: trimmed). */
  raw?: boolean;
}

export function textfield(el: HTMLInputElement | HTMLTextAreaElement, initial: TextFieldOptions) {
  let opts = initial;
  let dirty = false;
  // Marks the field as one of ours, and whether it holds typed text not
  // stored yet (follow mode waits for that, leave.ts `isTyping`).
  el.dataset.field = "";
  const setDirty = (d: boolean) => {
    dirty = d;
    el.toggleAttribute("data-typing", d);
  };
  const stored = () => opts.value ?? "";

  function render() {
    if (dirty) return;
    if (el.value !== stored()) el.value = stored();
  }

  function commit() {
    if (!dirty) return;
    setDirty(false);
    const v = opts.raw ? el.value : el.value.trim();
    if (v === stored()) {
      render();
      return;
    }
    if ((opts.required && !v) || opts.onchange(v) === false) {
      el.value = stored();
      return;
    }
    el.value = v;
  }

  const onInput = () => setDirty(true);
  function onKeydown(e: Event) {
    const k = e as KeyboardEvent;
    if (k.key === "Escape") {
      if (dirty) {
        setDirty(false);
        render();
      }
      k.preventDefault();
      k.stopPropagation();
      el.blur();
    } else if (k.key === "Enter" && (el instanceof HTMLInputElement || k.ctrlKey || k.metaKey)) {
      k.preventDefault();
      commit();
    }
  }

  el.addEventListener("input", onInput);
  el.addEventListener("keydown", onKeydown);
  el.addEventListener("blur", commit);
  render();

  return {
    update(next: TextFieldOptions) {
      opts = next;
      render();
    },
    destroy() {
      // Leaving the panel (tab switch, Back) right after typing still stores it.
      commit();
      el.removeEventListener("input", onInput);
      el.removeEventListener("keydown", onKeydown);
      el.removeEventListener("blur", commit);
    },
  };
}
