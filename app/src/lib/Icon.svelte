<script lang="ts">
  const paths: Record<string, string> = {
    plus: "M12 5v14M5 12h14",
    search: "M11 4a7 7 0 1 0 0 14 7 7 0 0 0 0-14zM20 20l-4-4",
    more: "M5 12h.01M12 12h.01M19 12h.01",
    trash: "M4 7h16M10 11v6M14 11v6M6 7l1 12a2 2 0 0 0 2 2h6a2 2 0 0 0 2-2l1-12M9 7V4h6v3",
    copy: "M9 9h10v10H9zM5 15V5h10",
    undo: "M9 14L4 9l5-5M4 9h10a6 6 0 0 1 0 12h-3",
    redo: "M15 14l5-5-5-5M20 9H10a6 6 0 0 0 0 12h3",
    back: "M15 18l-6-6 6-6",
    upload: "M12 16V4M7 9l5-5 5 5M5 20h14",
    open: "M5 12h14M13 6l6 6-6 6",
    folder: "M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z",
    split: "M4 5h16v14H4zM12 5v14",
    plan: "M4 4h16v16H4zM4 12h16M12 4v16",
    cube: "M12 3l8 4.5v9L12 21l-8-4.5v-9zM12 12l8-4.5M12 12v9M12 12L4 7.5",
    check: "M5 12l5 5L20 7",
    x: "M6 6l12 12M18 6L6 18",
    refresh: "M20 11a8 8 0 1 0-2.3 5.7M20 5v6h-6",
    roof: "M3 12l9-7 9 7M6 10v9h12v-9",
  };
  let { name, size = 18 }: { name: string; size?: number } = $props();
</script>

<svg
  width={size}
  height={size}
  viewBox="0 0 24 24"
  fill="none"
  stroke="currentColor"
  stroke-width="1.8"
  stroke-linecap="round"
  stroke-linejoin="round"
  aria-hidden="true"
>
  <path d={paths[name] ?? ""} />
</svg>
