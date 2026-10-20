import { useEffect, useRef } from "react";

type Props = {
  /** Gravando: carretéis mais rápidos e luz REC piscando. */
  recording?: boolean;
  /**
   * Quanto som está chegando agora (0 a 1). Quando informado, os carretéis
   * aceleram enquanto alguém fala e desaceleram no silêncio.
   */
  activity?: number;
  /** Pausada: os carretéis desaceleram até parar. */
  paused?: boolean;
  /** Texto escrito à mão na etiqueta. */
  label?: string;
  className?: string;
};

function Reel({
  cx,
  wound,
  reelRef,
}: {
  cx: number;
  wound: number;
  reelRef?: (el: SVGGElement | null) => void;
}) {
  return (
    <g>
      {/* Fita enrolada no carretel */}
      <circle cx={cx} cy={74} r={wound} fill="var(--tape)" />
      <g className="cassette-reel" ref={reelRef}>
        <circle cx={cx} cy={74} r={11} fill="var(--ink)" />
        <circle cx={cx} cy={74} r={4} fill="var(--mark-window)" />
        {[0, 60, 120].map((deg) => (
          <rect
            key={deg}
            x={cx - 1.6}
            y={63}
            width={3.2}
            height={22}
            rx={1.2}
            fill="var(--mark-window)"
            transform={`rotate(${deg} ${cx} 74)`}
          />
        ))}
      </g>
    </g>
  );
}

const LABEL_MAX = 22;

/** Fita cassete com carretéis girando, no estilo da marca. */
export function CassetteAnimation({
  recording = false,
  activity,
  paused = false,
  label = "Lado A",
  className = "",
}: Props) {
  const reels = useRef<(SVGGElement | null)[]>([]);
  const activityRef = useRef(activity ?? 0);
  activityRef.current = activity ?? 0;
  const pausedRef = useRef(paused);
  pausedRef.current = paused;
  const driven = activity !== undefined;

  // Gira os carretéis por script para a velocidade variar sem saltos.
  useEffect(() => {
    if (!driven) return;
    if (window.matchMedia?.("(prefers-reduced-motion: reduce)").matches) return;
    let frame = 0;
    let angle = 0;
    let speed = 0;
    let last = performance.now();
    const loop = (now: number) => {
      const dt = Math.min(0.1, (now - last) / 1000);
      last = now;
      // Graus por segundo: devagar no silêncio, rápido com fala.
      const target = pausedRef.current
        ? 0
        : 60 + 520 * Math.min(1, Math.sqrt(activityRef.current) * 1.6);
      speed += (target - speed) * Math.min(1, dt * 6);
      angle = (angle - speed * dt) % 360;
      // Via CSS (com transform-box: fill-box) e não pelo atributo do SVG:
      // o WebKit soma o atributo ao transform-origin e o carretel sai do lugar.
      reels.current.forEach((el) => {
        if (el) el.style.transform = `rotate(${angle}deg)`;
      });
      frame = requestAnimationFrame(loop);
    };
    frame = requestAnimationFrame(loop);
    return () => cancelAnimationFrame(frame);
  }, [driven]);

  return (
    <svg
      className={`cassette ${recording ? "is-recording" : ""} ${driven ? "is-driven" : ""} ${className}`
        .replace(/\s+/g, " ")
        .trim()}
      viewBox="0 0 240 150"
      role="img"
      aria-label={recording ? "Fita cassete gravando" : "Fita cassete pronta para gravar"}
    >
      <rect x="3" y="3" width="234" height="144" rx="14" fill="var(--mark-bg)" stroke="var(--line)" />
      <rect x="16" y="12" width="208" height="96" rx="6" fill="var(--ink)" />
      <rect x="16" y="20" width="208" height="5" fill="var(--accent)" />
      <rect x="16" y="25" width="208" height="5" fill="var(--warn)" />
      <rect x="16" y="30" width="208" height="5" fill="var(--sky)" />
      <text x="28" y="52" className="cassette-label">
        {label.length > LABEL_MAX ? `${label.slice(0, LABEL_MAX - 1).trimEnd()}…` : label}
      </text>
      <circle className="cassette-rec" cx="206" cy="47" r="5" fill="var(--accent)" />

      <rect x="56" y="56" width="128" height="36" rx="18" fill="var(--mark-window)" />
      {/* Fita passando entre os carretéis */}
      <path d="M84 92 L156 92" stroke="var(--tape)" strokeWidth="3" />
      <Reel
        cx={84}
        wound={17}
        reelRef={(el) => {
          reels.current[0] = el;
        }}
      />
      <Reel
        cx={156}
        wound={13}
        reelRef={(el) => {
          reels.current[1] = el;
        }}
      />

      <path d="M56 147 L66 118 H174 L184 147 Z" fill="var(--bg0)" />
      <circle cx="84" cy="134" r="4" fill="var(--mark-bg)" />
      <circle cx="156" cy="134" r="4" fill="var(--mark-bg)" />
    </svg>
  );
}
