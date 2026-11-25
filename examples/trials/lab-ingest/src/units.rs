//! Conversion of vendor units to the study's reporting units.

/// A measured value with its unit as the vendor sent it.
#[derive(Debug, Clone, PartialEq)]
pub struct Quantity {
    pub value: f64,
    pub unit: String,
}

/// Converts `q` to `target`; unknown pairs are an error so a value is never stored in the wrong unit.
pub fn normalise(analyte: &str, q: &Quantity, target: &str) -> Result<Quantity, String> {
    if q.unit.eq_ignore_ascii_case(target) {
        return Ok(Quantity { value: q.value, unit: target.to_string() });
    }
    let factor = match (analyte, q.unit.as_str(), target) {
        ("GLUC", "mg/dL", "mmol/L") => 1.0 / 18.016,
        ("CREAT", "mg/dL", "umol/L") => 88.42,
        ("BILI", "mg/dL", "umol/L") => 17.104,
        ("HGB", "g/dL", "g/L") => 10.0,
        ("ALT", "U/L", "ukat/L") => 1.0 / 60.0,
        _ => return Err(format!("no conversion for {analyte} from {} to {target}", q.unit)),
    };
    Ok(Quantity { value: q.value * factor, unit: target.to_string() })
}
