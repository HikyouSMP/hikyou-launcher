interface MaterialSliderProps {
  min: number;
  max: number;
  step: number;
  value: number;
  width: number;
  onChange: (value: number) => void;
}

function clamp(value: number, min: number, max: number) {
  return Math.min(max, Math.max(min, value));
}

export function MaterialSlider({
  min,
  max,
  step,
  value,
  width,
  onChange,
}: MaterialSliderProps) {
  const pct = ((value - min) / (max - min)) * 100;
  const commitFromClientX = (element: HTMLElement, clientX: number) => {
    const rect = element.getBoundingClientRect();
    const ratio = clamp((clientX - rect.left) / rect.width, 0, 1);
    const raw = min + ratio * (max - min);
    const stepped = Math.round(raw / step) * step;
    onChange(clamp(stepped, min, max));
  };

  return (
    <span
      className="material-slider"
      style={{
        width,
        ["--slider-p" as string]: `${pct}%`,
      }}
      onPointerDown={(event) => {
        event.currentTarget.setPointerCapture(event.pointerId);
        commitFromClientX(event.currentTarget, event.clientX);
      }}
      onPointerMove={(event) => {
        if (!event.currentTarget.hasPointerCapture(event.pointerId)) return;
        commitFromClientX(event.currentTarget, event.clientX);
      }}
      onPointerUp={(event) => {
        if (event.currentTarget.hasPointerCapture(event.pointerId)) {
          event.currentTarget.releasePointerCapture(event.pointerId);
        }
      }}
      onPointerCancel={(event) => {
        if (event.currentTarget.hasPointerCapture(event.pointerId)) {
          event.currentTarget.releasePointerCapture(event.pointerId);
        }
      }}
    >
      <span className="material-slider-track inactive left" />
      <span className="material-slider-track active" />
      <span className="material-slider-track inactive right" />
      <span className="material-slider-thumb" />
      <input
        type="range"
        min={min}
        max={max}
        step={step}
        value={value}
        onChange={(event) => onChange(Number(event.target.value))}
        aria-label="Slider"
        tabIndex={0}
      />
    </span>
  );
}
