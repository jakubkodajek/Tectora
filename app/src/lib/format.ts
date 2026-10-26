const nf = new Intl.NumberFormat("cs-CZ", { minimumFractionDigits: 2, maximumFractionDigits: 2 });

export const m = (v: number) => `${nf.format(v)} m`;
export const m2 = (v: number) => `${nf.format(v)} m²`;
export const num = (v: number) => nf.format(v);

export function relativeTime(ms: number): string {
  const diff = Date.now() - ms;
  const min = Math.round(diff / 60000);
  if (min < 1) return "právě teď";
  if (min < 60) return `před ${min} min`;
  const h = Math.round(min / 60);
  if (h < 24) return `před ${h} h`;
  return new Date(ms).toLocaleDateString("cs-CZ", { day: "numeric", month: "short", year: "numeric" });
}
