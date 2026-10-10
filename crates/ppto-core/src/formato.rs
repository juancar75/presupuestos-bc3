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

/// Importe en letra, como se escribe en los presupuestos:
/// 10.086,72 → «diez mil ochenta y seis euros con setenta y dos céntimos».
pub fn en_letra(importe: Decimal) -> String {
    let v = redondear(importe.abs(), 2);
    let euros = v.trunc();
    let cent = ((v - euros) * Decimal::ONE_HUNDRED).trunc();
    let e = u64::try_from(euros).unwrap_or(u64::MAX);
    let c = u64::try_from(cent).unwrap_or(0);
    let mut s = String::new();
    if importe < Decimal::ZERO {
        s.push_str("menos ");
    }
    s.push_str(&entero_en_letra(e, true));
    // «un millón de euros», pero «un millón cien euros»
    if e >= 1_000_000 && e.is_multiple_of(1_000_000) {
        s.push_str(" de");
    }
    s.push_str(if e == 1 { " euro" } else { " euros" });
    if c > 0 {
        s.push_str(" con ");
        s.push_str(&entero_en_letra(c, true));
        s.push_str(if c == 1 { " céntimo" } else { " céntimos" });
    }
    s
}

/// Número entero en letra. `apocope`: «un»/«veintiún» delante de sustantivo.
fn entero_en_letra(n: u64, apocope: bool) -> String {
    if n == 0 {
        return "cero".into();
    }
    let millones = n / 1_000_000;
    let miles = (n / 1000) % 1000;
    let resto = n % 1000;
    let mut partes = Vec::new();
    if millones > 0 {
        partes.push(if millones == 1 {
            "un millón".to_owned()
        } else {
            format!("{} millones", entero_en_letra(millones, true))
        });
    }
    if miles > 0 {
        partes.push(if miles == 1 {
            "mil".to_owned()
        } else {
            format!("{} mil", centenas(miles, true))
        });
    }
    if resto > 0 {
        partes.push(centenas(resto, apocope));
    }
    partes.join(" ")
}

fn centenas(n: u64, apocope: bool) -> String {
    const C: [&str; 10] = [
        "",
        "ciento",
        "doscientos",
        "trescientos",
        "cuatrocientos",
        "quinientos",
        "seiscientos",
        "setecientos",
        "ochocientos",
        "novecientos",
    ];
    if n == 100 {
        return "cien".into();
    }
    let (c, r) = (n / 100, n % 100);
    let dec = decenas(r, apocope);
    match (c, r) {
        (0, _) => dec,
        (_, 0) => C[c as usize].into(),
        _ => format!("{} {dec}", C[c as usize]),
    }
}

fn decenas(n: u64, apocope: bool) -> String {
    const U: [&str; 30] = [
        "",
        "uno",
        "dos",
        "tres",
        "cuatro",
        "cinco",
        "seis",
        "siete",
        "ocho",
        "nueve",
        "diez",
        "once",
        "doce",
        "trece",
        "catorce",
        "quince",
        "dieciséis",
        "diecisiete",
        "dieciocho",
        "diecinueve",
        "veinte",
        "veintiuno",
        "veintidós",
        "veintitrés",
        "veinticuatro",
        "veinticinco",
        "veintiséis",
        "veintisiete",
        "veintiocho",
        "veintinueve",
    ];
    const D: [&str; 10] = [
        "",
        "",
        "",
        "treinta",
        "cuarenta",
        "cincuenta",
        "sesenta",
        "setenta",
        "ochenta",
        "noventa",
    ];
    let t = if n < 30 {
        U[n as usize].to_owned()
    } else if n.is_multiple_of(10) {
        D[(n / 10) as usize].to_owned()
    } else {
        format!("{} y {}", D[(n / 10) as usize], U[(n % 10) as usize])
    };
    if apocope && n % 10 == 1 && n != 11 {
        // uno → un, veintiuno → veintiún, treinta y uno → treinta y un
        if n == 21 {
            return "veintiún".into();
        }
        return t.strip_suffix("uno").map(|x| format!("{x}un")).unwrap_or(t);
    }
    t
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn importes_en_letra() {
        let casos = [
            (
                dec!(10086.72),
                "diez mil ochenta y seis euros con setenta y dos céntimos",
            ),
            (dec!(1), "un euro"),
            (dec!(0.01), "cero euros con un céntimo"),
            (dec!(21), "veintiún euros"),
            (dec!(31.21), "treinta y un euros con veintiún céntimos"),
            (dec!(100), "cien euros"),
            (dec!(101), "ciento un euros"),
            (dec!(115), "ciento quince euros"),
            (dec!(1000), "mil euros"),
            (dec!(21000), "veintiún mil euros"),
            (
                dec!(157310.88),
                "ciento cincuenta y siete mil trescientos diez euros con ochenta y ocho céntimos",
            ),
            (dec!(500700), "quinientos mil setecientos euros"),
            (dec!(1000000), "un millón de euros"),
            (dec!(3000000.50), "tres millones de euros con cincuenta céntimos"),
            (dec!(1000100), "un millón cien euros"),
            (
                dec!(2345678.90),
                "dos millones trescientos cuarenta y cinco mil seiscientos setenta y ocho euros con noventa céntimos",
            ),
            (dec!(-5), "menos cinco euros"),
            (dec!(0), "cero euros"),
        ];
        for (v, t) in casos {
            assert_eq!(en_letra(v), t, "{v}");
        }
    }

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
