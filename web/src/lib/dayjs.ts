import dayjs from 'dayjs';
import relativeTime from 'dayjs/plugin/relativeTime';
import updateLocale from 'dayjs/plugin/updateLocale';

dayjs.extend(relativeTime);
dayjs.extend(updateLocale);

dayjs.updateLocale('en', {
  relativeTime: {
    future: 'in %s',
    past: '%s ago',
    s: '<1 min',
    m: '1 min',
    mm: '%d min',
    h: '1 hr',
    hh: '%d hr',
    d: '1 day',
    dd: '%d days',
    M: '1 mo',
    MM: '%d mo',
    y: '1 yr',
    yy: '%d yr',
  },
});

export default dayjs;
