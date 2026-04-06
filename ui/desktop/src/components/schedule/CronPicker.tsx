import React, { useState, useEffect } from 'react';
import cronstrue from 'cronstrue';
import { ScheduledJob } from '../../schedule';
import { errorMessage } from '../../utils/conversionUtils';
import { defineMessages, useIntl } from '../../i18n';

const i18n = defineMessages({
  every: { id: 'cronPicker.every', defaultMessage: 'Every' },
  minute: { id: 'cronPicker.minute', defaultMessage: 'Minute' },
  hour: { id: 'cronPicker.hour', defaultMessage: 'Hour' },
  day: { id: 'cronPicker.day', defaultMessage: 'Day' },
  week: { id: 'cronPicker.week', defaultMessage: 'Week' },
  month: { id: 'cronPicker.month', defaultMessage: 'Month' },
  year: { id: 'cronPicker.year', defaultMessage: 'Year' },
  inMonth: { id: 'cronPicker.inMonth', defaultMessage: 'in' },
  january: { id: 'cronPicker.january', defaultMessage: 'January' },
  february: { id: 'cronPicker.february', defaultMessage: 'February' },
  march: { id: 'cronPicker.march', defaultMessage: 'March' },
  april: { id: 'cronPicker.april', defaultMessage: 'April' },
  may: { id: 'cronPicker.may', defaultMessage: 'May' },
  june: { id: 'cronPicker.june', defaultMessage: 'June' },
  july: { id: 'cronPicker.july', defaultMessage: 'July' },
  august: { id: 'cronPicker.august', defaultMessage: 'August' },
  september: { id: 'cronPicker.september', defaultMessage: 'September' },
  october: { id: 'cronPicker.october', defaultMessage: 'October' },
  november: { id: 'cronPicker.november', defaultMessage: 'November' },
  december: { id: 'cronPicker.december', defaultMessage: 'December' },
  onDay: { id: 'cronPicker.onDay', defaultMessage: 'on day' },
  on: { id: 'cronPicker.on', defaultMessage: 'on' },
  sunday: { id: 'cronPicker.sunday', defaultMessage: 'Sunday' },
  monday: { id: 'cronPicker.monday', defaultMessage: 'Monday' },
  tuesday: { id: 'cronPicker.tuesday', defaultMessage: 'Tuesday' },
  wednesday: { id: 'cronPicker.wednesday', defaultMessage: 'Wednesday' },
  thursday: { id: 'cronPicker.thursday', defaultMessage: 'Thursday' },
  friday: { id: 'cronPicker.friday', defaultMessage: 'Friday' },
  saturday: { id: 'cronPicker.saturday', defaultMessage: 'Saturday' },
  at: { id: 'cronPicker.at', defaultMessage: 'at' },
  atMinute: { id: 'cronPicker.atMinute', defaultMessage: 'at minute' },
  atSecond: { id: 'cronPicker.atSecond', defaultMessage: 'at second' },
});

type Period = 'minute' | 'hour' | 'day' | 'week' | 'month' | 'year';

type ParsedCron = {
  period: Period;
  second: string;
  minute: string;
  hour: string;
  dayOfMonth: string;
  month: string;
  dayOfWeek: string;
};

interface CronPickerProps {
  schedule: ScheduledJob | null;
  onChange: (cron: string) => void;
  isValid: (valid: boolean) => void;
}

const parseCron = (cron: string): ParsedCron => {
  const parts = cron.split(' ');
  if (parts.length === 5) {
    parts.unshift('0');
  }
  if (parts.length !== 6) {
    return {
      period: 'day',
      second: '0',
      minute: '0',
      hour: '14',
      dayOfMonth: '*',
      month: '*',
      dayOfWeek: '*',
    };
  }

  const [second, minute, hour, dayOfMonth, month, dayOfWeek] = parts;

  if (month !== '*' && dayOfMonth !== '*') {
    return { period: 'year', second, minute, hour, dayOfMonth, month, dayOfWeek };
  }
  if (dayOfMonth !== '*') {
    return { period: 'month', second, minute, hour, dayOfMonth, month, dayOfWeek };
  }
  if (dayOfWeek !== '*') {
    return { period: 'week', second, minute, hour, dayOfMonth, month, dayOfWeek };
  }
  if (hour !== '*') {
    return { period: 'day', second, minute, hour, dayOfMonth, month, dayOfWeek };
  }
  if (minute !== '*') {
    return { period: 'hour', second, minute, hour, dayOfMonth, month, dayOfWeek };
  }
  return { period: 'minute', second, minute, hour, dayOfMonth, month, dayOfWeek };
};

const to24Hour = (hour12: number, isPM: boolean): number => {
  if (hour12 === 12) {
    return isPM ? 12 : 0;
  }
  return isPM ? hour12 + 12 : hour12;
};

const to12Hour = (hour24: number): { hour: number; isPM: boolean } => {
  if (hour24 === 0) {
    return { hour: 12, isPM: false };
  }
  if (hour24 === 12) {
    return { hour: 12, isPM: true };
  }
  if (hour24 > 12) {
    return { hour: hour24 - 12, isPM: true };
  }
  return { hour: hour24, isPM: false };
};

export const CronPicker: React.FC<CronPickerProps> = ({ schedule, onChange, isValid }) => {
  const intl = useIntl();
  const [period, setPeriod] = useState<Period>('day');
  const [second, setSecond] = useState('0');
  const [minute, setMinute] = useState('0');
  const [hour12, setHour12] = useState(2);
  const [isPM, setIsPM] = useState(true);
  const [dayOfWeek, setDayOfWeek] = useState('1');
  const [dayOfMonth, setDayOfMonth] = useState('1');
  const [month, setMonth] = useState('1');
  const [readableCron, setReadableCron] = useState('');

  useEffect(() => {
    const parsed = parseCron(schedule?.cron || '');
    setPeriod(parsed.period);
    setSecond(parsed.second === '*' ? '0' : parsed.second);
    setMinute(parsed.minute === '*' ? '0' : parsed.minute);
    const hour24 = parsed.hour === '*' ? 14 : parseInt(parsed.hour, 10);
    const { hour, isPM: pm } = to12Hour(hour24);
    setHour12(hour);
    setIsPM(pm);
    setDayOfWeek(parsed.dayOfWeek === '*' ? '1' : parsed.dayOfWeek);
    setDayOfMonth(parsed.dayOfMonth === '*' ? '1' : parsed.dayOfMonth);
    setMonth(parsed.month === '*' ? '1' : parsed.month);
  }, [schedule]);

  useEffect(() => {
    const hour24 = to24Hour(hour12, isPM);
    let cron: string;

    switch (period) {
      case 'minute':
        cron = `${second} * * * * *`;
        break;
      case 'hour':
        cron = `${second} ${minute} * * * *`;
        break;
      case 'day':
        cron = `${second} ${minute} ${hour24} * * *`;
        break;
      case 'week':
        cron = `${second} ${minute} ${hour24} * * ${dayOfWeek}`;
        break;
      case 'month':
        cron = `${second} ${minute} ${hour24} ${dayOfMonth} * *`;
        break;
      case 'year':
        cron = `${second} ${minute} ${hour24} ${dayOfMonth} ${month} *`;
        break;
      default:
        cron = '0 0 0 * * *';
    }
    onChange(cron);
    if (cron) {
      const cronWithoutSeconds = cron.split(' ').slice(1).join(' ');
      try {
        setReadableCron(cronstrue.toString(cronWithoutSeconds));
        isValid(true);
      } catch (e) {
        isValid(false);
        setReadableCron('error: ' + errorMessage(e));
      }
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [period, second, minute, hour12, isPM, dayOfWeek, dayOfMonth, month]);

  const selectClassName = 'px-2 py-1 border rounded bg-white dark:bg-gray-800 dark:border-gray-600';

  return (
    <div className="space-y-4">
      <div className="flex items-center gap-2">
        <span className="text-sm font-medium">{intl.formatMessage(i18n.every)}</span>
        <select
          value={period}
          onChange={(e) => setPeriod(e.target.value as Period)}
          className={selectClassName}
        >
          <option value="minute">{intl.formatMessage(i18n.minute)}</option>
          <option value="hour">{intl.formatMessage(i18n.hour)}</option>
          <option value="day">{intl.formatMessage(i18n.day)}</option>
          <option value="week">{intl.formatMessage(i18n.week)}</option>
          <option value="month">{intl.formatMessage(i18n.month)}</option>
          <option value="year">{intl.formatMessage(i18n.year)}</option>
        </select>
      </div>

      <div className="space-y-3">
        {period === 'year' && (
          <div className="flex items-center gap-2">
            <span className="text-sm">{intl.formatMessage(i18n.inMonth)}</span>
            <select
              value={month}
              onChange={(e) => setMonth(e.target.value)}
              className={selectClassName}
            >
              <option value="1">{intl.formatMessage(i18n.january)}</option>
              <option value="2">{intl.formatMessage(i18n.february)}</option>
              <option value="3">{intl.formatMessage(i18n.march)}</option>
              <option value="4">{intl.formatMessage(i18n.april)}</option>
              <option value="5">{intl.formatMessage(i18n.may)}</option>
              <option value="6">{intl.formatMessage(i18n.june)}</option>
              <option value="7">{intl.formatMessage(i18n.july)}</option>
              <option value="8">{intl.formatMessage(i18n.august)}</option>
              <option value="9">{intl.formatMessage(i18n.september)}</option>
              <option value="10">{intl.formatMessage(i18n.october)}</option>
              <option value="11">{intl.formatMessage(i18n.november)}</option>
              <option value="12">{intl.formatMessage(i18n.december)}</option>
            </select>
          </div>
        )}

        {(period === 'month' || period === 'year') && (
          <div className="flex items-center gap-2">
            <span className="text-sm">{intl.formatMessage(i18n.onDay)}</span>
            <input
              type="number"
              min="1"
              max="31"
              value={dayOfMonth}
              onChange={(e) => setDayOfMonth(e.target.value)}
              className="w-16 px-2 py-1 border rounded"
            />
          </div>
        )}

        {period === 'week' && (
          <div className="flex items-center gap-2">
            <span className="text-sm">{intl.formatMessage(i18n.on)}</span>
            <select
              value={dayOfWeek}
              onChange={(e) => setDayOfWeek(e.target.value)}
              className={selectClassName}
            >
              <option value="0">{intl.formatMessage(i18n.sunday)}</option>
              <option value="1">{intl.formatMessage(i18n.monday)}</option>
              <option value="2">{intl.formatMessage(i18n.tuesday)}</option>
              <option value="3">{intl.formatMessage(i18n.wednesday)}</option>
              <option value="4">{intl.formatMessage(i18n.thursday)}</option>
              <option value="5">{intl.formatMessage(i18n.friday)}</option>
              <option value="6">{intl.formatMessage(i18n.saturday)}</option>
            </select>
          </div>
        )}

        {(period === 'day' || period === 'week' || period === 'month' || period === 'year') && (
          <div className="flex items-center gap-2">
            <span className="text-sm">{intl.formatMessage(i18n.at)}</span>
            <input
              type="number"
              min="1"
              max="12"
              value={hour12}
              onChange={(e) => setHour12(parseInt(e.target.value) || 1)}
              className="w-16 px-2 py-1 border rounded"
            />
            <span className="text-sm">:</span>
            <input
              type="number"
              min="0"
              max="59"
              value={minute}
              onChange={(e) => setMinute(e.target.value.padStart(2, '0'))}
              className="w-16 px-2 py-1 border rounded"
            />
            <select
              value={isPM ? 'PM' : 'AM'}
              onChange={(e) => setIsPM(e.target.value === 'PM')}
              className={selectClassName}
            >
              <option value="AM">AM</option>
              <option value="PM">PM</option>
            </select>
          </div>
        )}

        {period === 'hour' && (
          <div className="flex items-center gap-2">
            <span className="text-sm">{intl.formatMessage(i18n.atMinute)}</span>
            <input
              type="number"
              min="0"
              max="59"
              value={minute}
              onChange={(e) => setMinute(e.target.value)}
              className="w-16 px-2 py-1 border rounded"
            />
          </div>
        )}

        {period === 'minute' && (
          <div className="flex items-center gap-2">
            <span className="text-sm">{intl.formatMessage(i18n.atSecond)}</span>
            <input
              type="number"
              min="0"
              max="59"
              value={second}
              onChange={(e) => setSecond(e.target.value)}
              className="w-16 px-2 py-1 border rounded"
            />
          </div>
        )}
      </div>

      <div className="text-xs text-gray-500 mt-2">{readableCron}</div>
    </div>
  );
};
