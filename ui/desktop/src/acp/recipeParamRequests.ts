import type {
  RecipeParameterDto,
  RecipeParamsResponse_unstable,
  RequestRecipeParams_unstable,
} from '@aaif/goose-acp-client';
import { v7 as uuidv7 } from 'uuid';

export interface AcpRecipeParamRequest {
  id: string;
  sessionId: string;
  parameters: RecipeParameterDto[];
  initialValues?: Record<string, string>;
}

interface PendingRecipeParamRequest {
  request: AcpRecipeParamRequest;
  resolve: (response: RecipeParamsResponse_unstable) => void;
  scopeId?: string;
}

interface ConfiguredParameterValues {
  values: Record<string, string>;
  sessionId?: string;
}

export interface ConfiguredRecipeParameterScope {
  id: string;
  finish(): void;
}

const pendingRequests = new Map<string, PendingRecipeParamRequest>();
const configuredParameterScopes = new Map<string, ConfiguredParameterValues>();
const listeners = new Set<() => void>();
let snapshot: AcpRecipeParamRequest[] = [];
let startupParametersRead = false;

function emit(): void {
  snapshot = Array.from(pendingRequests.values(), (pending) => pending.request);
  for (const listener of listeners) {
    listener();
  }
}

export function subscribeAcpRecipeParamRequests(listener: () => void): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

export function getAcpRecipeParamRequestsSnapshot(): AcpRecipeParamRequest[] {
  return snapshot;
}

function consumeConfiguredParameters(scopeId: string): boolean {
  if (!configuredParameterScopes.delete(scopeId)) {
    return false;
  }

  let scrubbedPendingRequest = false;
  for (const pending of pendingRequests.values()) {
    if (pending.scopeId === scopeId) {
      pending.request.initialValues = {};
      scrubbedPendingRequest = true;
    }
  }
  return scrubbedPendingRequest;
}

export function beginConfiguredRecipeParameterScope(
  values?: Record<string, string>
): ConfiguredRecipeParameterScope | undefined {
  if (values === undefined) {
    if (startupParametersRead) return undefined;
    startupParametersRead = true;
    values = window.appConfig?.get('recipeParameters') as Record<string, string> | undefined;
  }
  if (!values || Object.keys(values).length === 0) {
    return undefined;
  }

  const scopeId = `configured_recipe_parameters_${uuidv7()}`;
  configuredParameterScopes.set(scopeId, { values: { ...values } });
  return {
    id: scopeId,
    finish: () => {
      if (consumeConfiguredParameters(scopeId)) {
        emit();
      }
    },
  };
}

function configuredParameterValues(request: RequestRecipeParams_unstable): {
  values: Record<string, string>;
  scopeId?: string;
} {
  const scopeId = request.parameterScopeId ?? undefined;
  const scope = scopeId ? configuredParameterScopes.get(scopeId) : undefined;
  if (!scope || (scope.sessionId !== undefined && scope.sessionId !== request.sessionId)) {
    return { values: {} };
  }
  scope.sessionId = request.sessionId;
  const fileParameterKeys = new Set(
    request.parameters
      .filter((parameter) => parameter.input_type === 'file')
      .map((parameter) => parameter.key)
  );
  return {
    values: Object.fromEntries(
      Object.entries(scope.values).filter(([key]) => !fileParameterKeys.has(key))
    ),
    scopeId,
  };
}

export async function requestAcpRecipeParams(
  request: RequestRecipeParams_unstable
): Promise<RecipeParamsResponse_unstable> {
  const { values: initialValues, scopeId } = configuredParameterValues(request);
  const paramRequest: AcpRecipeParamRequest = {
    id: `acp_recipe_params_${uuidv7()}`,
    sessionId: request.sessionId,
    parameters: request.parameters,
    initialValues,
  };

  return new Promise<RecipeParamsResponse_unstable>((resolve) => {
    pendingRequests.set(paramRequest.id, {
      request: paramRequest,
      resolve,
      scopeId,
    });
    emit();
  });
}

export function resolveAcpRecipeParamRequest(id: string, values: Record<string, string>): boolean {
  const pending = pendingRequests.get(id);
  if (!pending) {
    return false;
  }
  pendingRequests.delete(id);
  if (pending.scopeId) {
    consumeConfiguredParameters(pending.scopeId);
  }
  emit();
  pending.resolve({ action: 'submit', values });
  return true;
}

export function cancelAcpRecipeParamRequest(id: string): void {
  const pending = pendingRequests.get(id);
  if (!pending) {
    return;
  }
  pendingRequests.delete(id);
  if (pending.scopeId) {
    consumeConfiguredParameters(pending.scopeId);
  }
  emit();
  pending.resolve({ action: 'cancel' });
}
