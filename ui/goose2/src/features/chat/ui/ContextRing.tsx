import { useTranslation } from "react-i18next";
import { useLocaleFormatting } from "@/shared/i18n";

// ---------------------------------------------------------------------------
// ContextRing — SVG circular indicator for context token usage
// ---------------------------------------------------------------------------

export function ContextRing({
  tokens,
  limit,
  size = 20,
}: {
  tokens: number;
  limit: number;
  size?: number;
}) {
  const { t } = useTranslation("chat");
  const { formatNumber } = useLocaleFormatting();
  const radius = (size - 3) / 2;
  const circumference = 2 * Math.PI * radius;
  const progress = limit > 0 ? Math.min(tokens / limit, 1) : 0;
  const offset = circumference - progress * circumference;
  const percent = formatNumber(Math.round(progress * 100));

  // Color based on usage
  const strokeColor =
    progress > 0.9
      ? "var(--text-danger)"
      : progress > 0.7
        ? "var(--text-warning)"
        : "var(--text-muted)";

  return (
    <svg
      width={size}
      height={size}
      viewBox={`0 0 ${size} ${size}`}
      className="shrink-0"
      aria-label={t("context.ringAria", { percent })}
    >
      {/* Background track */}
      <circle
        cx={size / 2}
        cy={size / 2}
        r={radius}
        fill="none"
        stroke="currentColor"
        strokeWidth={2}
        className="text-muted-foreground/30"
      />
      {/* Progress arc */}
      <circle
        cx={size / 2}
        cy={size / 2}
        r={radius}
        fill="none"
        stroke={strokeColor}
        strokeWidth={2}
        strokeLinecap="round"
        strokeDasharray={circumference}
        strokeDashoffset={offset}
        transform={`rotate(-90 ${size / 2} ${size / 2})`}
        className="transition-all duration-300"
      />
    </svg>
  );
}
