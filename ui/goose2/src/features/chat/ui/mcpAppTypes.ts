export type McpAppMessageHandler = (
  text: string,
) => boolean | undefined | Promise<boolean | undefined>;
