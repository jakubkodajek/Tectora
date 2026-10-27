<script lang="ts">
  const paths: Record<string, string> = {
    plus: "M12 5v14M5 12h14",
    chimney: "M3 20l9-8 9 8M14 14V6h4v11.5M13 6h6",
    window: "M5 5h14v14H5zM12 5v14M5 12h14",
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
    draw: "M4 20h4L19 9l-4-4L4 16zM13 7l4 4",
    house: "M3 11l9-7 9 7M5 9.5V20h14V9.5M10 20v-5h4v5",
    sparkle: "M12 3l1.8 5.2L19 10l-5.2 1.8L12 17l-1.8-5.2L5 10l5.2-1.8zM19 15l.8 2.2L22 18l-2.2.8L19 21l-.8-2.2L16 18l2.2-.8z",
    link: "M10 14a4 4 0 0 0 5.7 0l3-3a4 4 0 0 0-5.7-5.7l-1 1M14 10a4 4 0 0 0-5.7 0l-3 3a4 4 0 0 0 5.7 5.7l1-1",
    list: "M8 6h12M8 12h12M8 18h12M4 6h.01M4 12h.01M4 18h.01",
    grid: "M4 4h7v7H4zM13 4h7v7h-7zM4 13h7v7H4zM13 13h7v7h-7z",
    sliders: "M4 6h10M18 6h2M4 12h4M12 12h8M4 18h12M20 18h0M14 4v4M8 10v4M16 16v4",
    history: "M3 12a9 9 0 1 0 3-6.7M3 4v5h5M12 7v5l3 2",
    fit: "M4 9V4h5M20 9V4h-5M4 15v5h5M20 15v5h-5",
    eye: "M2 12s3.6-7 10-7 10 7 10 7-3.6 7-10 7S2 12 2 12zM12 9a3 3 0 1 0 0 6 3 3 0 0 0 0-6z",
    left: "M15 18l-6-6 6-6",
    right: "M9 6l6 6-6 6",
    briefcase: "M4 8h16v11H4zM9 8V5h6v3M4 13h16",
    chart: "M5 20V10M12 20V4M19 20v-7",
    close: "M5 12l7-7 7 7",
    cursor: "M5 3l6 17 2.5-7.5L21 10z",
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
