type Props = {
  size?: number;
  className?: string;
};

/** Marca do Rebobina: fita cassete com o carretel esquerdo em vermelho REC. */
export function BrandMark({ size = 36, className = "" }: Props) {
  return (
    <svg
      className={`brand-mark ${className}`.trim()}
      width={size}
      height={(size * 80) / 120}
      viewBox="0 0 120 80"
      fill="none"
      xmlns="http://www.w3.org/2000/svg"
      aria-hidden
    >
      <rect x="2" y="2" width="116" height="76" rx="10" fill="var(--mark-bg)" />
      <rect x="10" y="8" width="100" height="44" rx="4" fill="var(--mark-label)" />
      <rect x="10" y="12" width="100" height="3.5" fill="var(--accent)" />
      <rect x="10" y="15.5" width="100" height="3.5" fill="var(--warn)" />
      <rect x="10" y="19" width="100" height="3.5" fill="var(--sky)" />
      <rect x="30" y="27" width="60" height="18" rx="9" fill="var(--mark-window)" />
      <circle cx="44" cy="36" r="6.5" fill="var(--mark-rec)" />
      <circle cx="76" cy="36" r="6" stroke="var(--mark-label)" strokeWidth="2.5" />
      <circle cx="76" cy="36" r="2" fill="var(--mark-label)" />
      <path d="M28 78 34 60h52l6 18z" fill="var(--bg0)" />
      <circle cx="44" cy="70" r="2.5" fill="var(--mark-bg)" />
      <circle cx="76" cy="70" r="2.5" fill="var(--mark-bg)" />
    </svg>
  );
}
