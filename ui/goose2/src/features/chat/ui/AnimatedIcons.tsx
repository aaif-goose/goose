import { useState, useEffect } from "react";
import {
  CodeXml,
  Cog,
  Fuel,
  GalleryHorizontalEnd,
  Gavel,
  GlassWater,
  Grape,
  Watch0,
  Watch1,
  Watch2,
  Watch3,
  Watch4,
  Watch5,
  Watch6,
} from "@/shared/ui/icons/thinking";

interface AnimatedIconsProps {
  className?: string;
  cycleInterval?: number;
  variant?: "thinking" | "waiting";
}

const thinkingIcons = [
  CodeXml,
  Cog,
  Fuel,
  GalleryHorizontalEnd,
  Gavel,
  GlassWater,
  Grape,
];
const waitingIcons = [Watch0, Watch1, Watch2, Watch3, Watch4, Watch5, Watch6];

export function AnimatedIcons({
  className = "",
  cycleInterval = 500,
  variant = "thinking",
}: AnimatedIconsProps) {
  const [currentIconIndex, setCurrentIconIndex] = useState(0);
  const icons = variant === "thinking" ? thinkingIcons : waitingIcons;

  useEffect(() => {
    const interval = setInterval(() => {
      setCurrentIconIndex((prevIndex) => (prevIndex + 1) % icons.length);
    }, cycleInterval);

    return () => clearInterval(interval);
  }, [cycleInterval, icons.length]);

  const CurrentIcon = icons[currentIconIndex];

  return (
    <div
      className={`transition-opacity duration-200 w-4 h-4 ${className}`}
      aria-hidden="true"
    >
      <CurrentIcon className="w-full h-full" />
    </div>
  );
}
