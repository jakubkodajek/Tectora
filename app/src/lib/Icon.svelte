<script lang="ts">
  const paths: Record<string, string> = {
    plus: "M12 5v14M5 12h14",
    chimney: "M3 19l9-8 2 1.78M17 15.44L21 19M14 12.78V6h3v9.44M13 6h5",
    window: "M5 5h14v14H5zM12 5v14M5 12h14",
    hole: "M3 18l9-12 9 12zM9.5 13.5h5v3h-5z",
    search: "M11 4a7 7 0 1 0 0 14 7 7 0 0 0 0-14zM20 20l-4-4",
    more: "M5 12h.01M12 12h.01M19 12h.01",
    eraser: "M7 21h13M5.5 15.5l9-9.5 5 5-9 9.5H8.5zM10 11l5 5",
    trash: "M4 7h16M10 11v6M14 11v6M6 7l1 12a2 2 0 0 0 2 2h6a2 2 0 0 0 2-2l1-12M9 7V4h6v3",
    copy: "M9 9h10v10H9zM5 15V5h10",
    move: "M12 3v18M3 12h18M9 6l3-3 3 3M9 18l3 3 3-3M6 9l-3 3 3 3M18 9l3 3-3 3",
    undo: "M9 14L4 9l5-5M4 9h10a6 6 0 0 1 0 12h-3",
    redo: "M15 14l5-5-5-5M20 9H10a6 6 0 0 0 0 12h3",
    back: "M15 18l-6-6 6-6",
    upload: "M12 16V4M7 9l5-5 5 5M5 20h14",
    download: "M12 4v12M7 11l5 5 5-5M5 20h14",
    open: "M5 12h14M13 6l6 6-6 6",
    gear: "M12 9a3 3 0 1 0 0 6 3 3 0 0 0 0-6zM19.4 15a1.7 1.7 0 0 0 .3 1.8l.1.1a2 2 0 1 1-2.8 2.8l-.1-.1a1.7 1.7 0 0 0-1.8-.3 1.7 1.7 0 0 0-1 1.5V21a2 2 0 1 1-4 0v-.1a1.7 1.7 0 0 0-1.1-1.5 1.7 1.7 0 0 0-1.8.3l-.1.1a2 2 0 1 1-2.8-2.8l.1-.1a1.7 1.7 0 0 0 .3-1.8 1.7 1.7 0 0 0-1.5-1H3a2 2 0 1 1 0-4h.1a1.7 1.7 0 0 0 1.5-1.1 1.7 1.7 0 0 0-.3-1.8l-.1-.1a2 2 0 1 1 2.8-2.8l.1.1a1.7 1.7 0 0 0 1.8.3H9a1.7 1.7 0 0 0 1-1.5V3a2 2 0 1 1 4 0v.1a1.7 1.7 0 0 0 1 1.5 1.7 1.7 0 0 0 1.8-.3l.1-.1a2 2 0 1 1 2.8 2.8l-.1.1a1.7 1.7 0 0 0-.3 1.8V9a1.7 1.7 0 0 0 1.5 1H21a2 2 0 1 1 0 4h-.1a1.7 1.7 0 0 0-1.5 1z",
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
    sparkle: "M10 6l1.8 5.2L17 13l-5.2 1.8L10 20l-1.8-5.2L3 13l5.2-1.8zM19 3v4M17 5h4",
    link: "M10 14a4 4 0 0 0 5.7 0l3-3a4 4 0 0 0-5.7-5.7l-1 1M14 10a4 4 0 0 0-5.7 0l-3 3a4 4 0 0 0 5.7 5.7l1-1",
    list: "M8 6h12M8 12h12M8 18h12M4 6h.01M4 12h.01M4 18h.01",
    grid: "M4 4h7v7H4zM13 4h7v7h-7zM4 13h7v7H4zM13 13h7v7h-7z",
    sliders: "M4 6h10M18 6h2M4 12h4M12 12h8M4 18h12M20 18h0M14 4v4M8 10v4M16 16v4",
    history: "M3 12a9 9 0 1 0 3-6.7M3 4v5h5M12 7v5l3 2",
    fit: "M4 9V4h5M20 9V4h-5M4 15v5h5M20 15v5h-5",
    dims: "M4 7v10M20 7v10M4 12h16M7 9l-3 3 3 3M17 9l3 3-3 3",
    eye: "M2 12s3.6-7 10-7 10 7 10 7-3.6 7-10 7S2 12 2 12zM12 9a3 3 0 1 0 0 6 3 3 0 0 0 0-6z",
    left: "M15 18l-6-6 6-6",
    right: "M9 6l6 6-6 6",
    briefcase: "M4 8h16v11H4zM9 8V5h6v3M4 13h16",
    chart: "M5 20V10M12 20V4M19 20v-7",
    sigma: "M18 5H6l7 7-7 7h12",
    cursor: "M5 3l6 17 2.5-7.5L21 10z",
    warning: "M12 3l10 18H2zM12 10v5M12 18h.01",
    line: "M5 19L19 5M3 21l4-4M17 7l4-4",
    face: "M4 18l5-12 11 3-4 10z",
    flat: "M3 7h18M5 7v10h14V7",
  };
  let { name, size = 18 }: { name: string; size?: number } = $props();
</script>

<svg
  class="icon"
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
