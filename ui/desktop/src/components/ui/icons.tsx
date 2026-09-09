export const GPSIcon = ({ size = 16 }: { size: number }) => {
  return (
    <svg
      height={size}
      strokeLinejoin="round"
      viewBox="0 0 16 16"
      width={size}
      style={{ color: 'currentcolor' }}
    >
      <path
        d="M1 6L15 1L10 15L7.65955 8.91482C7.55797 8.65073 7.34927 8.44203 7.08518 8.34045L1 6Z"
        stroke="currentColor"
        strokeWidth="1.5"
        strokeLinecap="round"
        strokeLinejoin="bevel"
        fill="transparent"
      ></path>
    </svg>
  );
};
