/** Formato fixo das barras: mais altas no meio, como uma onda de voz. */
const SHAPE = [0.35, 0.55, 0.8, 0.65, 1, 0.75, 0.9, 0.6, 0.8, 0.5, 0.65, 0.35];

/** Abaixo disso consideramos silêncio (ruído de fundo). */
export const SPEECH_THRESHOLD = 0.04;

type Props = {
  label: string;
  /** Nível suavizado, de 0 a 1. */
  level: number;
  /** Muda a cada atualização para as barras não ficarem estáticas. */
  tick: number;
};

/** Barras que pulsam com a voz de uma das faixas (você ou a chamada). */
export function SpeechMeter({ label, level, tick }: Props) {
  const speaking = level >= SPEECH_THRESHOLD;
  // Curva para que falas baixas também apareçam bem.
  const intensity = Math.min(1, Math.sqrt(level) * 1.6);

  return (
    <div className={`speech-meter ${speaking ? "is-speaking" : ""}`}>
      <div className="speech-bars" aria-hidden>
        {SHAPE.map((weight, i) => {
          const wobble = speaking ? 0.7 + 0.3 * Math.abs(Math.sin(tick * 1.7 + i * 1.3)) : 1;
          const height = speaking ? 10 + 90 * intensity * weight * wobble : 8;
          return <i key={i} style={{ height: `${height}%` }} />;
        })}
      </div>
      <span className="speech-label">
        {label}
        <span className="speech-state">{speaking ? "falando" : "em silêncio"}</span>
      </span>
    </div>
  );
}
