const FR: Record<string, string> = {
  Brightness: "Luminosité",
};

const french = navigator.language.toLowerCase().startsWith("fr");

export const t = (text: string): string => (french && FR[text]) || text;
