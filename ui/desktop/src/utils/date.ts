export const formatToLocalDateWithTimezone = (dateString?: string | null): string => {
  if (!dateString) {
    return 'N/A';
  }
  try {
    const date = new Date(dateString);
    if (isNaN(date.getTime())) {
      return 'Invalid Date';
    }
    // Format: Jan 1, 2023, 10:00:00 AM PST (example)
    return date.toLocaleString(undefined, {
      year: 'numeric',
      month: 'short',
      day: 'numeric',
      hour: 'numeric',
      minute: '2-digit',
      second: '2-digit',
      timeZoneName: 'short',
    });
  } catch (e) {
    console.error('Error formatting date with timezone:', e);
    return 'Invalid Date';
  }
};
