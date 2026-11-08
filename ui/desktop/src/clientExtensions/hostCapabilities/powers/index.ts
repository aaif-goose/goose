import { commandsPower } from './commands';
import { netPower } from './net';
import { platformPower } from './platform';
import { providersPower } from './providers';
import { recipesPower } from './recipes';
import { schedulesPower } from './schedules';
import { sessionsPower } from './sessions';
import { storagePower } from './storage';
import { toolsPower } from './tools';

export const COMMON_HOST_POWERS = [
  platformPower,
  providersPower,
  sessionsPower,
  recipesPower,
  commandsPower,
  storagePower,
  schedulesPower,
  toolsPower,
  netPower,
] as const;
