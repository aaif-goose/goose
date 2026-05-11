import { invoke } from "@tauri-apps/api/core";
import type {
  ReadTextFileRequest,
  ReadTextFileResponse,
  WriteTextFileRequest,
  WriteTextFileResponse,
} from "@agentclientprotocol/sdk";

const ACP_READ_TEXT_FILE = "acp_read_text_file";
const ACP_WRITE_TEXT_FILE = "acp_write_text_file";

export const acpFsCallbacks = {
  readTextFile: async (
    args: ReadTextFileRequest,
  ): Promise<ReadTextFileResponse> => {
    const content = await invoke<string>(ACP_READ_TEXT_FILE, {
      path: args.path,
      line: args.line ?? null,
      limit: args.limit ?? null,
    });
    return { content };
  },

  writeTextFile: async (
    args: WriteTextFileRequest,
  ): Promise<WriteTextFileResponse> => {
    await invoke(ACP_WRITE_TEXT_FILE, {
      path: args.path,
      content: args.content,
    });
    return {};
  },
};
