//! CLI messages: English in the code, French in `FR`.

use std::fmt::Display;
use std::sync::OnceLock;

pub const USAGE: &str =
    "usage: fader <daemon|panel|list|doctor|get SCREEN|set SCREEN|focused|all 0-100|+n|-n>";

const FR: &[(&str, &str)] = &[
    (
        USAGE,
        "utilisation : fader <daemon|panel|list|doctor|get ÉCRAN|set ÉCRAN|focused|all 0-100|+n|-n>",
    ),
    (
        "XDG_RUNTIME_DIR is not set",
        "XDG_RUNTIME_DIR n'est pas défini",
    ),
    (
        "fader daemon is already running",
        "le daemon fader est déjà lancé",
    ),
    (
        "fader daemon is not running, start it with: fader daemon",
        "le daemon fader n'est pas lancé, démarrez-le avec : fader daemon",
    ),
    (
        "cannot connect to a Wayland display",
        "connexion à l'écran Wayland impossible",
    ),
    (
        "this compositor does not support the layer-shell protocol",
        "ce compositeur ne gère pas le protocole layer-shell",
    ),
    (
        "cannot reach Hyprland, is it running in this session?",
        "Hyprland est injoignable, tourne-t-il dans cette session ?",
    ),
    (
        "unexpected reply from Hyprland: {}",
        "réponse inattendue de Hyprland : {}",
    ),
    ("{} is not installed", "{} n'est pas installé"),
    ("{} did not answer in time", "{} n'a pas répondu à temps"),
    ("unknown screen: {}", "écran inconnu : {}"),
    ("invalid request: {}", "requête invalide : {}"),
    ("internal error", "erreur interne"),
    (
        "no brightness backend found",
        "aucun moyen de régler la luminosité",
    ),
    ("output is disabled", "sortie désactivée"),
    ("problems found", "problèmes détectés"),
    (
        "Run fader from a terminal inside your Hyprland session.",
        "Lancez fader depuis un terminal de votre session Hyprland.",
    ),
    ("i2c-dev module is loaded", "module i2c-dev chargé"),
    ("i2c-dev module is not loaded", "module i2c-dev non chargé"),
    (
        "Load it with: sudo modprobe i2c-dev",
        "Chargez-le avec : sudo modprobe i2c-dev",
    ),
    (
        "Install it with: sudo pacman -S ddcutil",
        "Installez-le avec : sudo pacman -S ddcutil",
    ),
    ("{} is accessible", "{} est accessible"),
    ("{}: permission denied", "{} : accès refusé"),
    (
        "Run: sudo usermod -aG i2c $USER, then log out and back in",
        "Lancez : sudo usermod -aG i2c $USER, puis reconnectez-vous",
    ),
    ("backlight device: {}", "rétroéclairage : {}"),
    (
        "no backlight device (normal on a desktop)",
        "pas de rétroéclairage (normal sur un PC de bureau)",
    ),
    (
        "Enable DDC/CI in the monitor's on-screen menu, and check the cable",
        "Activez DDC/CI dans le menu de l'écran et vérifiez le câble",
    ),
];

fn is_french() -> bool {
    static FRENCH: OnceLock<bool> = OnceLock::new();
    *FRENCH.get_or_init(|| {
        let var = |name| std::env::var(name).ok().filter(|v| !v.is_empty());
        locale_is_french(
            var("LC_ALL")
                .or_else(|| var("LC_MESSAGES"))
                .or_else(|| var("LANG"))
                .as_deref(),
        )
    })
}

fn locale_is_french(locale: Option<&str>) -> bool {
    locale.is_some_and(|l| l == "fr" || l.starts_with("fr_") || l.starts_with("fr."))
}

fn translate(french: bool, text: &'static str) -> &'static str {
    if french && let Some((_, translated)) = FR.iter().find(|(en, _)| *en == text) {
        return translated;
    }
    text
}

pub fn t(text: &'static str) -> &'static str {
    translate(is_french(), text)
}

pub fn tf(template: &'static str, value: impl Display) -> String {
    t(template).replacen("{}", &value.to_string(), 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_a_french_locale() {
        assert!(locale_is_french(Some("fr_FR.UTF-8")));
        assert!(locale_is_french(Some("fr")));
        assert!(!locale_is_french(Some("en_US.UTF-8")));
        assert!(!locale_is_french(Some("fry_NL")));
        assert!(!locale_is_french(None));
    }

    #[test]
    fn translates_known_messages_and_keeps_unknown_ones() {
        assert_eq!(translate(true, "internal error"), "erreur interne");
        assert_eq!(translate(false, "internal error"), "internal error");
        assert_eq!(translate(true, "not in the table"), "not in the table");
    }

    #[test]
    fn templates_keep_their_placeholder_in_both_languages() {
        for (en, fr) in FR {
            assert_eq!(en.matches("{}").count(), fr.matches("{}").count(), "{en}");
        }
    }
}
