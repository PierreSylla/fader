const FR: Record<string, string> = {
  Brightness: "Luminosité",
  backlight: "rétroéclairage",
  "no method": "aucune méthode",
  sending: "envoi",
  applying: "application en cours",
  "Looking for screens": "Recherche des écrans",
  "No screen detected.": "Aucun écran détecté.",
  "Run:": "Lancez :",
  "I²C access denied": "Accès I²C refusé",
  "I²C access denied: DDC screens cannot be adjusted.":
    "Accès I²C refusé : les écrans DDC ne peuvent pas être réglés.",
  "Drag vertically on a screen · wheel · ↑↓ ±1 · PgUp/PgDn ±10":
    "Glisser verticalement sur un écran · molette · ↑↓ ±1 · PgUp/PgDn ±10",
  "Drag on the bar · wheel · ↑↓ ±1 · PgUp/PgDn ±10":
    "Glisser sur la barre · molette · ↑↓ ±1 · PgUp/PgDn ±10",
};

const french = navigator.language.toLowerCase().startsWith("fr");

export const t = (text: string): string => (french && FR[text]) || text;
