import { GooseIcon, Rain } from "@/shared/ui/icons/GooseIcon";
import { cn } from "@/shared/lib/cn";

interface GooseLogoProps {
  className?: string;
  size?: "default" | "small";
  hover?: boolean;
}

export function GooseLogo({
  className = "",
  size = "default",
  hover = true,
}: GooseLogoProps) {
  const sizes = {
    default: {
      frame: "w-16 h-16",
      rain: "w-[275px] h-[275px]",
      goose: "w-16 h-16",
    },
    small: {
      frame: "w-8 h-8",
      rain: "w-[150px] h-[150px]",
      goose: "w-8 h-8",
    },
  } as const;

  const currentSize = sizes[size];

  return (
    <div
      className={cn(
        className,
        currentSize.frame,
        "relative overflow-hidden",
        hover && "group/with-hover",
      )}
    >
      <Rain
        className={cn(
          currentSize.rain,
          "absolute left-0 bottom-0 transition-all duration-300 z-[1]",
          hover && "opacity-0 group-hover/with-hover:opacity-100",
        )}
      />
      <GooseIcon
        className={cn(currentSize.goose, "absolute left-0 bottom-0 z-[2]")}
      />
    </div>
  );
}
