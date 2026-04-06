export const Delegate = ({ className }: { className?: string }) => (
  <svg
    width="11"
    height="11"
    viewBox="0 0 11 11"
    fill="none"
    xmlns="http://www.w3.org/2000/svg"
    className={className}
  >
    <rect width="11" height="11" rx="2" fill="#6366F1" />
    <path
      d="M3 4L6 5.5L3 7"
      stroke="white"
      strokeWidth="1.5"
      strokeLinecap="round"
      strokeLinejoin="round"
    />
    <circle cx="8" cy="5.5" r="1" fill="white" />
  </svg>
);
