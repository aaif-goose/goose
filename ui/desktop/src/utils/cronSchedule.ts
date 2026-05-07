import cronstrue from 'cronstrue';

export type Period = 'minute' | 'hour' | 'day' | 'week' | 'month' | 'quarter' | 'year' | 'custom';

export const quarterMonthsByStartMonth: Record<string, string> = {
  '1': '1,4,7,10',
  '2': '2,5,8,11',
  '3': '3,6,9,12',
};

export const quarterDayLimitByStartMonth: Record<string, number> = {
  '1': 30,
  '2': 28,
  '3': 30,
};

export type ParsedCron = {
  period: Period;
  second: string;
  minute: string;
  hour: string;
  dayOfMonth: string;
  month: string;
  dayOfWeek: string;
};

export type CronParts = {
  period: Period;
  second: string;
  minute: string;
  hour24: number;
  dayOfWeek: string;
  dayOfMonth: string | null;
  month: string;
  quarterStartMonth: string;
  customCron: string;
};

export const defaultParsedCron: ParsedCron = {
  period: 'day',
  second: '0',
  minute: '0',
  hour: '14',
  dayOfMonth: '*',
  month: '*',
  dayOfWeek: '*',
};

export const getQuarterStartMonth = (month: string): string | null => {
  const entry = Object.entries(quarterMonthsByStartMonth).find(
    ([, quarterMonths]) => quarterMonths === month
  );
  return entry?.[0] ?? null;
};

const normalizeCronParts = (cron: string): string[] | null => {
  const parts = cron.trim().split(/\s+/);
  if (parts.length === 5) {
    return ['0', ...parts];
  }
  if (parts.length === 6) {
    return parts;
  }
  return null;
};

export const isSingleNumericValue = (value: string): boolean => /^\d+$/.test(value);

export const getValidDayOfMonth = (value: string, max: number): string | null => {
  if (!isSingleNumericValue(value)) {
    return null;
  }
  const parsedDay = parseInt(value, 10);
  if (parsedDay < 1 || parsedDay > max) {
    return null;
  }
  return parsedDay.toString();
};

const asCustomCron = (parts: string[]): ParsedCron => {
  const [second, minute, hour, dayOfMonth, month, dayOfWeek] = parts;
  return { period: 'custom', second, minute, hour, dayOfMonth, month, dayOfWeek };
};

export const describeCron = (cron: string): string => {
  const parts = cron.trim().split(/\s+/);
  if (parts.length === 5 || parts.length === 6) {
    return cronstrue.toString(parts.join(' '));
  }
  throw new Error('Expected 5 or 6 fields');
};

export const parseCron = (cron: string): ParsedCron => {
  if (!cron.trim()) {
    return defaultParsedCron;
  }

  const parts = normalizeCronParts(cron);
  if (!parts) {
    return { ...defaultParsedCron, period: 'custom' };
  }

  const [second, minute, hour, dayOfMonth, month, dayOfWeek] = parts;

  if (!isSingleNumericValue(second)) {
    return asCustomCron(parts);
  }

  if (dayOfMonth !== '*') {
    const quarterStartMonth = getQuarterStartMonth(month);
    const dayOfMonthNumber = parseInt(dayOfMonth, 10);
    if (
      quarterStartMonth &&
      isSingleNumericValue(dayOfMonth) &&
      dayOfMonthNumber <= quarterDayLimitByStartMonth[quarterStartMonth]
    ) {
      return { period: 'quarter', second, minute, hour, dayOfMonth, month, dayOfWeek };
    }
  }
  if (month !== '*' && dayOfMonth !== '*') {
    if (!isSingleNumericValue(month) || !isSingleNumericValue(dayOfMonth)) {
      return asCustomCron(parts);
    }
    return { period: 'year', second, minute, hour, dayOfMonth, month, dayOfWeek };
  }
  if (dayOfMonth !== '*') {
    if (!isSingleNumericValue(dayOfMonth)) {
      return asCustomCron(parts);
    }
    return { period: 'month', second, minute, hour, dayOfMonth, month, dayOfWeek };
  }
  if (dayOfWeek !== '*') {
    if (!isSingleNumericValue(dayOfWeek)) {
      return asCustomCron(parts);
    }
    return { period: 'week', second, minute, hour, dayOfMonth, month, dayOfWeek };
  }
  if (hour !== '*') {
    if (!isSingleNumericValue(hour)) {
      return asCustomCron(parts);
    }
    return { period: 'day', second, minute, hour, dayOfMonth, month, dayOfWeek };
  }
  if (minute !== '*') {
    if (!isSingleNumericValue(minute)) {
      return asCustomCron(parts);
    }
    return { period: 'hour', second, minute, hour, dayOfMonth, month, dayOfWeek };
  }
  return { period: 'minute', second, minute, hour, dayOfMonth, month, dayOfWeek };
};

export const buildCronForPeriod = ({
  period,
  second,
  minute,
  hour24,
  dayOfWeek,
  dayOfMonth,
  month,
  quarterStartMonth,
  customCron,
}: CronParts): string => {
  switch (period) {
    case 'custom':
      return customCron;
    case 'minute':
      return `${second} * * * * *`;
    case 'hour':
      return `${second} ${minute} * * * *`;
    case 'day':
      return `${second} ${minute} ${hour24} * * *`;
    case 'week':
      return `${second} ${minute} ${hour24} * * ${dayOfWeek}`;
    case 'month':
      return `${second} ${minute} ${hour24} ${dayOfMonth ?? '0'} * *`;
    case 'quarter':
      return `${second} ${minute} ${hour24} ${dayOfMonth ?? '0'} ${quarterMonthsByStartMonth[quarterStartMonth]} *`;
    case 'year':
      return `${second} ${minute} ${hour24} ${dayOfMonth ?? '0'} ${month} *`;
    default:
      return '0 0 0 * * *';
  }
};
