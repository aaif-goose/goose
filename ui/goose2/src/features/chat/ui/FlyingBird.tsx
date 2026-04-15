import { useState, useEffect } from "react";
import {
  Bird1,
  Bird2,
  Bird3,
  Bird4,
  Bird5,
  Bird6,
} from "@/shared/ui/icons/birds";

interface FlyingBirdProps {
  className?: string;
  cycleInterval?: number;
}

const birdFrames = [Bird1, Bird2, Bird3, Bird4, Bird5, Bird6];

export function FlyingBird({
  className = "",
  cycleInterval = 150,
}: FlyingBirdProps) {
  const [currentFrameIndex, setCurrentFrameIndex] = useState(0);

  useEffect(() => {
    const interval = setInterval(() => {
      setCurrentFrameIndex((prevIndex) => (prevIndex + 1) % birdFrames.length);
    }, cycleInterval);

    return () => clearInterval(interval);
  }, [cycleInterval]);

  const CurrentFrame = birdFrames[currentFrameIndex];

  return (
    <div
      className={`transition-opacity duration-75 ${className}`}
      aria-hidden="true"
    >
      <CurrentFrame className="w-4 h-4" />
    </div>
  );
}
