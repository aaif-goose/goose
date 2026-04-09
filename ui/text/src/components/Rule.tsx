import React from "react";
import { Text } from "ink";
import { RULE_COLOR } from "../colors.js";

interface RuleProps {
  width: number;
}

export const Rule = React.memo(function Rule({ width }: RuleProps) {
  const ruleWidth = Math.max(width, 1);
  return <Text color={RULE_COLOR}>{"─".repeat(ruleWidth)}</Text>;
});
