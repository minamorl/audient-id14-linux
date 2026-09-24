//! Decibel conversion for UAC2 1/256 dB controls (volume, mixer, input gain).
//!
//! The raw value is a signed 16-bit count of 1/256 dB; `0x8000` means silence
//! (-inf dB) and is never produced from a decibel argument.

use crate::control_error::ControlError;

/// Raw code for silence (-inf dB).
pub const SILENCE: i16 = i16::MIN;

/// Convert decibels to the nearest raw 1/256 dB step.
pub fn db_to_raw(db: f64) -> Result<i16, ControlError> {
    if !db.is_finite() {
        return Err(ControlError::InvalidDecibels(db.to_string()));
    }
    let scaled = (db * 256.0).round();
    if scaled <= f64::from(SILENCE) || scaled > f64::from(i16::MAX) {
        return Err(ControlError::InvalidDecibels(db.to_string()));
    }
    // in range (SILENCE, i16::MAX] after the check above
    Ok(scaled as i16)
}

/// Parse a decibel argument such as `-20`, `-20.5` or `0`.
pub fn parse_db(input: &str) -> Result<i16, ControlError> {
    let trimmed = input.trim();
    let number = trimmed
        .strip_suffix("dB")
        .or_else(|| trimmed.strip_suffix("db"))
        .unwrap_or(trimmed)
        .trim();
    let db: f64 = number
        .parse()
        .map_err(|_| ControlError::InvalidDecibels(input.to_string()))?;
    db_to_raw(db).map_err(|_| ControlError::InvalidDecibels(input.to_string()))
}

/// Convert a raw 1/256 dB value to decibels.
pub fn raw_to_db(raw: i16) -> f64 {
    f64::from(raw) / 256.0
}

/// Format a raw 1/256 dB value (`-inf dB` for silence).
pub fn format_raw_db(raw: i16) -> String {
    if raw == SILENCE {
        "-inf dB".to_string()
    } else {
        format!("{:.2} dB", raw_to_db(raw))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn whole_and_fractional_decibels() {
        assert_eq!(parse_db("-20"), Ok(-20 * 256));
        assert_eq!(parse_db("-20.5"), Ok(-20 * 256 - 128));
        assert_eq!(parse_db("0"), Ok(0));
        assert_eq!(parse_db("-127 dB"), Ok(-127 * 256));
    }

    #[test]
    fn unrepresentable_decibels_are_rejected() {
        assert!(parse_db("-128").is_err());
        assert!(parse_db("128").is_err());
        assert!(parse_db("loud").is_err());
        assert!(parse_db("NaN").is_err());
        assert!(parse_db("inf").is_err());
    }

    #[test]
    fn formatting() {
        assert_eq!(format_raw_db(-20 * 256), "-20.00 dB");
        assert_eq!(format_raw_db(SILENCE), "-inf dB");
    }
}
