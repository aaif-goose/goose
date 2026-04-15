import React from "react";
import { Box, Text, useInput, useStdout } from "ink";
import { CRANBERRY, TEXT_PRIMARY, TEXT_DIM } from "../colors.js";

interface ErrorScreenProps {
  errorMsg: string;
  onRetry: () => void;
}

export const ErrorScreen = React.memo(function ErrorScreen({ errorMsg, onRetry }: ErrorScreenProps) {
  const { stdout } = useStdout();
  const columns = stdout?.columns ?? 80;
  
  useInput((ch, key) => {
    if (key.return || key.escape) {
      onRetry();
    }
  });

  const maxWidth = Math.min(columns - 4, 80);

  return (
    <Box flexDirection="column" paddingX={2} width={maxWidth}>
      <Text color={CRANBERRY} bold>✗ Setup error</Text>
      {errorMsg && (
        <Box width={maxWidth - 4}>
          <Text color={TEXT_PRIMARY} wrap="wrap">{errorMsg}</Text>
        </Box>
      )}
      <Box marginTop={1}>
        <Text color={TEXT_DIM}>press enter to retry</Text>
      </Box>
    </Box>
  );
});
