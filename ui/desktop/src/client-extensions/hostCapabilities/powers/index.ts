import { platformPower } from './platform';
import { providersPower } from './providers';
import { sessionsPower } from './sessions';

export const COMMON_HOST_POWERS = [platformPower, providersPower, sessionsPower] as const;
