// SPDX-License-Identifier: GPL-3.0-or-later
//! Formato y lectura de números en notación española (1.234,56).

use crate::decimales::redondear;
use rust_decimal::Decimal;
use std::str::FromStr;

/// Número con `d` decimales, separador de miles «.» y decimal «,».
pub fn num(v: Decimal, d: u32) -> String {
    let s = format!("{:.*}", d as usize, redondear(v, d));
    let (ent, dec) = s.split_once('.').map_or((s.as_str(), None), |(e, f)| (e, Some(f)));
    let (signo, dig) = ent.strip_prefix('-').map_or(("", ent), |x| ("-", x));
    let mut out = String::from(signo);
    for (i, c) in dig.chars().enumerate() {
        if i > 0 && (dig.len() - i) % 3 == 0 {
            out.push('.');
        }
        out.push(c);
    }
    if let Some(f) = dec {
        out.push(',');
        out.push_str(f);
    }
    out
}

/// Importe en euros con dos decimales (sin símbolo).
pub fn eur(v: Decimal) -> String {
    num(v, 2)
}

/// Interpreta lo que escribe un usuario español.
///
/// - Con coma: la coma es decimal y los puntos son miles («1.470,5» → 1470,5).
/// - Sin coma: el punto es decimal («24.50» → 24,50), salvo que tenga forma de
///   miles exactos («1.470» → 1470).
/// - Vacío → `None`.
pub fn leer(texto: &str) -> Option<Decimal> {
    let t: String = texto
        .trim()
        .chars()
        .filter(|c| !c.is_whitespace() && *c != '€')
        .collect();
    if t.is_empty() {
        return None;
    }
    let normal = if t.contains(',') {
        t.replace('.', "").replace(',', ".")
    } else if es_miles(&t) {
        t.replace('.', "")
    } else {
        t
    };
    Decimal::from_str(&normal).ok()
}

fn es_miles(t: &str) -> bool {
    let t = t.strip_prefix('-').unwrap_or(t);
    let partes: Vec<&str> = t.split('.').collect();
    partes.len() >= 2
        && (1..=3).contains(&partes[0].len())
        && partes[1..].iter().all(|p| p.len() == 3)
        && partes.iter().all(|p| p.chars().all(|c| c.is_ascii_digit()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn formato_espanol() {
        assert_eq!(eur(dec!(7005.15)), "7.005,15");
        assert_eq!(eur(dec!(10086.72)), "10.086,72");
        assert_eq!(eur(dec!(-2.1)), "-2,10");
        assert_eq!(eur(dec!(123)), "123,00");
        assert_eq!(eur(dec!(1234567.891)), "1.234.567,89");
        assert_eq!(num(dec!(1470), 3), "1.470,000");
        assert_eq!(num(dec!(0.125), 2), "0,13");
    }

    #[test]
    fn lectura_espanola() {
        assert_eq!(leer("24,50"), Some(dec!(24.50)));
        assert_eq!(leer("24.50"), Some(dec!(24.50)));
        assert_eq!(leer("1.470"), Some(dec!(1470)));
        assert_eq!(leer("1.470,5"), Some(dec!(1470.5)));
        assert_eq!(leer(" 7 005,15 € "), Some(dec!(7005.15)));
        assert_eq!(leer("-6"), Some(dec!(-6)));
        assert_eq!(leer("0.25"), Some(dec!(0.25)));
        assert_eq!(leer(""), None);
        assert_eq!(leer("abc"), None);
    }
}
