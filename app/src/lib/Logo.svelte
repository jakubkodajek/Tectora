<script lang="ts">
  // Tectora mark: T-shaped hip roof plan (same drawing as the app icon).
  let { size = 28 }: { size?: number } = $props();
</script>

<svg viewBox="100 100 824 824" width={size} height={size} aria-hidden="true">
  <rect x="102" y="102" width="820" height="820" rx="186" fill="var(--logo-bg)" stroke="var(--border)" stroke-width="6" />
  <g fill="none" stroke-linejoin="round" stroke-linecap="round" stroke-width="40">
    <path
      d="M254.4,263.6 L355.6,364.8 L254.4,466.0 M355.6,364.8 L668.4,364.8 L769.6,263.6 M668.4,364.8 L769.6,466.0 M410.8,466.0 L512.0,364.8 L613.2,466.0 M512.0,364.8 L512.0,659.2 L613.2,760.4 M512.0,659.2 L410.8,760.4"
      stroke="#DB8839"
    />
    <polygon points="254.4,263.6 769.6,263.6 769.6,466.0 613.2,466.0 613.2,760.4 410.8,760.4 410.8,466.0 254.4,466.0" stroke="#A8362C" />
  </g>
</svg>
