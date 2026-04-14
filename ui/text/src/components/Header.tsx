import React from "react";
import { Box, Text } from "ink";
import { Spinner } from "./Spinner.js";
import { Rule } from "./Rule.js";
import { TEAL, CRANBERRY, TEXT_PRIMARY, TEXT_DIM, RULE_COLOR } from "../colors.js";
import { isErrorStatus } from "../utils.js";

interface HeaderProps {
  width: number;
  status: string;
  loading: boolean;
  spinIdx: number;
  hasPendingPermission: boolean;
  turnInfo?: { current: number; total: number };
}

export const Header = React.memo(function Header({
  width,
  status,
  loading,
  spinIdx,
  hasPendingPermission,
  turnInfo,
}: HeaderProps) {
  const statusColor =
    status === "ready" ? TEAL : isErrorStatus(status) ? CRANBERRY : TEXT_DIM;

  const constrainedWidth = Math.max(width, 20);
  const leftSideWidth = Math.min(Math.floor(constrainedWidth * 0.7), constrainedWidth - 15);
  const rightSideWidth = constrainedWidth - leftSideWidth;

  return (
    <Box flexDirection="column" width={constrainedWidth} flexShrink={0}>
      <Box justifyContent="space-between" width={constrainedWidth}>
        <Box width={leftSideWidth}>
          <Text color={TEXT_PRIMARY} bold>goose</Text>
          <Text color={RULE_COLOR}> · </Text>
          <Box width={Math.max(leftSideWidth - 10, 5)}>
            <Text color={statusColor} wrap="truncate-end">{status}</Text>
          </Box>
          {loading && !hasPendingPermission && (
            <Text> <Spinner idx={spinIdx} /></Text>
          )}
        </Box>
        <Box width={rightSideWidth} justifyContent="flex-end">
          {turnInfo && turnInfo.total > 1 && (
            <Text color={TEXT_DIM}>
              {turnInfo.current}/{turnInfo.total}{"  "}
            </Text>
          )}
          <Text color={TEXT_DIM}>^C exit</Text>
        </Box>
      </Box>
      <Rule width={constrainedWidth} />
    </Box>
  );
});
