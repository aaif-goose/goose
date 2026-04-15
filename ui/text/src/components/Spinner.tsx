import React from "react";
import { Text } from "ink";
import { CRANBERRY } from "../colors.js";

const SPINNER_FRAMES = ["◐", "◓", "◑", "◒"];

interface SpinnerProps {
  idx: number;
}

export const Spinner = React.memo(function Spinner({ idx }: SpinnerProps) {
  return (
    <Text color={CRANBERRY}>
      {SPINNER_FRAMES[idx % SPINNER_FRAMES.length]}
    </Text>
  );
});

export { SPINNER_FRAMES };
