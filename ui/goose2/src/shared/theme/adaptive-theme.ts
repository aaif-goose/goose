interface RGB {
  r: number;
  g: number;
  b: number;
}

function hexToRgb(hex: string): RGB {
  const long = /^#?([a-f\d]{2})([a-f\d]{2})([a-f\d]{2})([a-f\d]{2})?$/i.exec(
    hex,
  );
  if (long) {
    return {
      r: Number.parseInt(long[1], 16),
      g: Number.parseInt(long[2], 16),
      b: Number.parseInt(long[3], 16),
    };
  }

  const short = /^#?([a-f\d])([a-f\d])([a-f\d])([a-f\d])?$/i.exec(hex);
  if (short) {
    return {
      r: Number.parseInt(short[1] + short[1], 16),
      g: Number.parseInt(short[2] + short[2], 16),
      b: Number.parseInt(short[3] + short[3], 16),
    };
  }

  return { r: 128, g: 128, b: 128 };
}

function rgbToHex({ r, g, b }: RGB): string {
  const clamp = (value: number) =>
    Math.max(0, Math.min(255, Math.round(value)));
  return `#${[r, g, b]
    .map((channel) => clamp(channel).toString(16).padStart(2, "0"))
    .join("")}`;
}

export function luminance(hex: string): number {
  const { r, g, b } = hexToRgb(hex);
  const [rs, gs, bs] = [r, g, b].map((channel) => {
    const scaled = channel / 255;
    return scaled <= 0.03928
      ? scaled / 12.92
      : ((scaled + 0.055) / 1.055) ** 2.4;
  });

  return 0.2126 * rs + 0.7152 * gs + 0.0722 * bs;
}

function getContrastColor(hexColor: string): string {
  return luminance(hexColor) > 0.5 ? "#000000" : "#ffffff";
}

function mix(hex1: string, hex2: string, factor: number): string {
  const color1 = hexToRgb(hex1);
  const color2 = hexToRgb(hex2);

  return rgbToHex({
    r: color1.r + (color2.r - color1.r) * factor,
    g: color1.g + (color2.g - color1.g) * factor,
    b: color1.b + (color2.b - color1.b) * factor,
  });
}

function adjust(hex: string, amount: number): string {
  const target = amount > 0 ? "#ffffff" : "#000000";
  return mix(hex, target, Math.abs(amount));
}

function overlay(hex: string, alpha: number): string {
  const { r, g, b } = hexToRgb(hex);
  return `rgba(${r}, ${g}, ${b}, ${alpha})`;
}

const CONTRAST_VALUE = 0.035;
const CONTRAST_OFFSET = 0.0135;

function calculateLumDiff(backgroundLuminance: number): number {
  return (
    CONTRAST_VALUE * Math.log(1 + (backgroundLuminance + CONTRAST_OFFSET) * 10)
  );
}

function findColorWithLuminance(
  baseColor: string,
  targetLuminance: number,
): string {
  const baseLuminance = luminance(baseColor);
  if (Math.abs(baseLuminance - targetLuminance) < 0.001) {
    return baseColor;
  }

  const target = targetLuminance < baseLuminance ? "#000000" : "#ffffff";
  let low = 0;
  let high = 1;

  for (let attempt = 0; attempt < 20; attempt += 1) {
    const mid = (low + high) / 2;
    const testLuminance = luminance(mix(baseColor, target, mid));
    const difference = testLuminance - targetLuminance;

    if (Math.abs(difference) < 0.001) {
      break;
    }

    if (target === "#000000") {
      if (testLuminance > targetLuminance) {
        low = mid;
      } else {
        high = mid;
      }
    } else if (testLuminance < targetLuminance) {
      low = mid;
    } else {
      high = mid;
    }
  }

  return mix(baseColor, target, (low + high) / 2);
}

function calculateChromeColors(syntaxBackground: string): {
  chrome: string;
  primary: string;
} {
  const backgroundLuminance = luminance(syntaxBackground);
  const luminanceDifference = calculateLumDiff(backgroundLuminance);
  const targetChromeLuminance = backgroundLuminance - luminanceDifference;

  if (targetChromeLuminance >= 0) {
    return {
      chrome: findColorWithLuminance(syntaxBackground, targetChromeLuminance),
      primary: syntaxBackground,
    };
  }

  return {
    chrome: findColorWithLuminance(syntaxBackground, 0),
    primary: findColorWithLuminance(syntaxBackground, luminanceDifference),
  };
}

export function hexToHsl(hex: string): string {
  const { r, g, b } = hexToRgb(hex);
  const red = r / 255;
  const green = g / 255;
  const blue = b / 255;

  const max = Math.max(red, green, blue);
  const min = Math.min(red, green, blue);
  const lightness = (max + min) / 2;

  if (max === min) {
    return `0 0% ${(lightness * 100).toFixed(1)}%`;
  }

  const difference = max - min;
  const saturation =
    lightness > 0.5 ? difference / (2 - max - min) : difference / (max + min);

  let hue: number;
  if (max === red) {
    hue = ((green - blue) / difference + (green < blue ? 6 : 0)) / 6;
  } else if (max === green) {
    hue = ((blue - red) / difference + 2) / 6;
  } else {
    hue = ((red - green) / difference + 4) / 6;
  }

  return `${(hue * 360).toFixed(1)} ${(saturation * 100).toFixed(2)}% ${(lightness * 100).toFixed(1)}%`;
}

export interface ThemeGitColors {
  added: string | null;
  deleted: string | null;
  modified: string | null;
}

export interface ThemeResult {
  isDark: boolean;
  vars: Record<string, string>;
}

export function createThemeVars(
  syntaxBackground: string,
  syntaxForeground: string,
  syntaxComment: string,
  gitColors?: ThemeGitColors,
): ThemeResult {
  const isDark = luminance(syntaxBackground) < 0.5;

  const { chrome: chromeColor, primary: primaryBackground } =
    calculateChromeColors(syntaxBackground);

  const direction = isDark ? 1 : -1;
  const elevate = (amount: number) =>
    adjust(primaryBackground, direction * amount);

  const fallbackGreen = isDark ? "#3fb950" : "#1a7f37";
  const fallbackRed = isDark ? "#f85149" : "#cf222e";
  const fallbackOrange = isDark ? "#d29922" : "#9a6700";

  const accentGreen = gitColors?.added ?? fallbackGreen;
  const accentRed = gitColors?.deleted ?? fallbackRed;
  const accentOrange = gitColors?.modified ?? fallbackOrange;

  const borderColor = mix(
    primaryBackground,
    syntaxForeground,
    isDark ? 0.15 : 0.12,
  );
  const hoverBackground = elevate(0.06);
  const textForeground = hexToHsl(syntaxForeground);
  const destructiveForeground = hexToHsl(getContrastColor(accentRed));

  return {
    isDark,
    vars: {
      "--background": hexToHsl(primaryBackground),
      "--card": hexToHsl(primaryBackground),
      "--popover": hexToHsl(elevate(0.08)),
      "--muted": hexToHsl(hoverBackground),
      "--accent": hexToHsl(hoverBackground),
      "--secondary": hexToHsl(hoverBackground),
      "--foreground": textForeground,
      "--card-foreground": textForeground,
      "--popover-foreground": textForeground,
      "--muted-foreground": hexToHsl(syntaxComment),
      "--accent-foreground": textForeground,
      "--secondary-foreground": textForeground,
      "--destructive": hexToHsl(accentRed),
      "--destructive-foreground": destructiveForeground,
      "--border": hexToHsl(borderColor),
      "--input": hexToHsl(borderColor),
      "--ring": textForeground,
      "--sidebar-background": hexToHsl(chromeColor),
      "--sidebar-foreground": textForeground,
      "--sidebar-accent": hexToHsl(primaryBackground),
      "--sidebar-accent-foreground": textForeground,
      "--sidebar-border": hexToHsl(borderColor),
      "--sidebar-ring": hexToHsl(borderColor),
      "--status-added": accentGreen,
      "--status-deleted": accentRed,
      "--status-modified": accentOrange,
      "--ui-warning": accentOrange,
      "--ui-warning-bg": overlay(accentOrange, isDark ? 0.1 : 0.08),
    },
  };
}
