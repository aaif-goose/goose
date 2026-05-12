import { useEffect, useState } from "react";
import { listSkills } from "@/features/skills/api/skills";
import type { OnboardingStep } from "../types";

export function useOnboardingReadyStep(step: OnboardingStep) {
  const [availableSkillCount, setAvailableSkillCount] = useState<number | null>(
    null,
  );

  useEffect(() => {
    if (step !== "tour") {
      return;
    }

    let cancelled = false;
    listSkills()
      .then((skills) => {
        if (!cancelled) {
          setAvailableSkillCount(skills.length);
        }
      })
      .catch(() => {
        if (!cancelled) {
          setAvailableSkillCount(null);
        }
      });

    return () => {
      cancelled = true;
    };
  }, [step]);

  return { availableSkillCount };
}
