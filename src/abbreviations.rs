//! Word-level abbreviation table for Star Citizen action labels.
//!
//! Splits the input on whitespace, uppercases each word, and replaces
//! known words with shorter forms. Designed to make labels fit on
//! 144×144 Stream Deck keys.

/// Abbreviate a label for display on a Stream Deck key.
///
/// Uppercases the entire string and replaces known words with shorter forms.
pub fn abbreviate(text: &str) -> String {
    text.split_whitespace()
        .map(|word| {
            let upper = word.to_uppercase();
            match upper.as_str() {
                // ── Flight & Movement ────────────────────────────────────
                "ACCELERATION" => "ACCEL",
                "AFTERBURNER" => "ABRN",
                "AUTOLAND" => "ALND",
                "BACKWARD" | "BACKWARDS" => "BWD",
                "BOOST" => "BST",
                "COUPLED" => "CPLD",
                "CRUISE" => "CRS",
                "DECOUPLED" => "DCPLD",
                "DECREASE" => "DECR",
                "DECELERATE" => "DECEL",
                "FLIGHT" => "FLGHT",
                "FORWARD" => "FWD",
                "INCREASE" => "INCR",
                "LANDING" => "LNDG",
                "MATCH" => "MTCH",
                "MOVEMENT" => "MVMT",
                "QUANTUM" => "QTM",
                "SPEED" => "SPD",
                "THROTTLE" => "THRTL",
                "VELOCITY" => "VEL",
                "VTOL" => "VTOL",

                // ── Power & Systems ──────────────────────────────────────
                "COOLER" => "COOL",
                "DESTRUCT" => "DESTR",
                "DESTRUCTION" => "DESTR",
                "ENGINE" => "ENGIN",
                "ENGINES" => "ENGINS",
                "GENERATOR" => "GEN",
                "POWER" => "PWR",
                "SELF-DESTRUCT" => "SELF-DESTR",
                "SHIELD" => "SHELD",
                "SHIELDS" => "SHELDS",
                "SYSTEM" | "SYSTEMS" => "SYS",

                // ── Weapons & Combat ─────────────────────────────────────
                "COUNTERMEASURE" => "CM",
                "COUNTERMEASURES" => "CMS",
                "GIMBAL" => "GMBL",
                "MISSILE" => "MSSL",
                "MISSILES" => "MSSLS",
                "TARGETING" => "TGT",
                "TORPEDO" => "TORP",
                "TORPEDOES" => "TORPS",
                "WEAPON" => "WEAPN",
                "WEAPONS" => "WEAPNS",

                // ── Vehicle & Cockpit ────────────────────────────────────
                "COCKPIT" => "CKPT",
                "DISPLAY" | "DISPLAYS" => "DISP",
                "DOORS" => "DRS",
                "EJECT" => "EJCT",
                "EMERGENCY" => "EMERG",
                "OPERATOR" => "OPER",
                "REMOTE" => "RMTE",
                "SEAT" => "SEAT",
                "TURRET" => "TURT",
                "VEHICLE" => "VHCL",
                "VEHICLES" => "VHCLS",

                // ── Modes & Actions ──────────────────────────────────────
                "ACTIVATE" => "ACTV",
                "CAMERA" => "CAM",
                "CYCLE" => "CYC",
                "DISABLE" | "DISABLED" => "DSBL",
                "ENABLE" | "ENABLED" => "ENBL",
                "FREELOOK" => "FLOOK",
                "LOCK" => "LCK",
                "MINING" => "MINE",
                "MODE" => "MODE",
                "SALVAGE" => "SALV",
                "SCANNING" => "SCAN",
                "TARGET" => "TGT",
                "TOGGLE" => "TOGL",
                "UNLOCK" => "ULCK",
                "ZOOM" => "ZM",

                // ── General ──────────────────────────────────────────────
                "CONFIGURATION" => "CFG",
                "GENERAL" => "GEN",
                "INTERACTION" => "INTRC",
                "MANAGEMENT" => "MGMT",
                "NAVIGATION" => "NAV",
                "PERSONAL" => "PRSNL",
                "SPACESHIP" => "SHIP",

                // No abbreviation — return the uppercased word as-is
                _ => &upper,
            }
            .to_string()
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic_abbreviation() {
        assert_eq!(abbreviate("Toggle Power"), "TOGL PWR");
    }

    #[test]
    fn multi_word() {
        assert_eq!(abbreviate("Self Destruct"), "SELF DESTR");
        assert_eq!(abbreviate("Quantum Throttle Increase"), "QTM THRTL INCR");
    }

    #[test]
    fn unknown_words_pass_through_uppercased() {
        assert_eq!(abbreviate("Open MobiGlas"), "OPEN MOBIGLAS");
    }

    #[test]
    fn already_uppercase() {
        assert_eq!(abbreviate("WEAPON"), "WEAPN");
    }

    #[test]
    fn empty_string() {
        assert_eq!(abbreviate(""), "");
    }
}
