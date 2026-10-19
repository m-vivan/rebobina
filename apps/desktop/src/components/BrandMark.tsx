type Props = {
  size?: number;
  className?: string;
};

/** Marca do Gravador: ponto de gravação + barras de forma de onda. */
export function BrandMark({ size = 36, className = "" }: Props) {
  return (
    <svg
      className={`brand-mark ${className}`.trim()}
      width={size}
      height={size}
      viewBox="0 0 64 64"
      fill="none"
      xmlns="http://www.w3.org/2000/svg"
      aria-hidden
    >
      <rect width="64" height="64" rx="16" fill="var(--mark-bg)" />
      <circle cx="18" cy="32" r="6" fill="var(--mark-rec)" />
      <path
        d="M30 22v20M38 18v28M46 24v16"
        stroke="var(--mark-wave)"
        strokeWidth="4"
        strokeLinecap="round"
      />
    </svg>
  );
}
