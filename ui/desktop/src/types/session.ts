import type { Recipe } from '../recipe';
import type { Message } from './message';

export type ExtensionData = Record<string, unknown>;

export type GooseMode = 'auto' | 'approve' | 'smart_approve';

export type ModelConfig = {
  context_limit?: number | null;
  max_tokens?: number | null;
  model_name: string;
  reasoning?: boolean | null;
  request_params?: Record<string, unknown> | null;
  temperature?: number | null;
  toolshim: boolean;
  toolshim_model?: string | null;
};

export type Usage = {
  cache_read_input_tokens?: number | null;
  cache_write_input_tokens?: number | null;
  input_tokens?: number | null;
  output_tokens?: number | null;
  total_tokens?: number | null;
};

export type SessionType =
  | 'user'
  | 'scheduled'
  | 'sub_agent'
  | 'hidden'
  | 'terminal'
  | 'gateway'
  | 'acp';

export type Session = {
  accumulated_cost?: number | null;
  accumulated_usage?: Usage;
  archived_at?: string | null;
  conversation?: Message[] | null;
  created_at: string;
  extension_data: ExtensionData;
  goose_mode?: GooseMode;
  id: string;
  last_message_at?: string | null;
  last_message_snippet?: string | null;
  message_count: number;
  model_config?: ModelConfig | null;
  name: string;
  project_id?: string | null;
  provider_name?: string | null;
  recipe?: Recipe | null;
  schedule_id?: string | null;
  session_type?: SessionType;
  updated_at: string;
  usage?: Usage;
  user_recipe_values?: Record<string, string> | null;
  user_set_name?: boolean;
  working_dir: string;
};
