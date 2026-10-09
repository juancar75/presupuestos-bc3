// SPDX-License-Identifier: GPL-3.0-or-later
//! Hojas de medición (equivalentes al registro `~M`).
//!
//! Reglas (ver `docs/reglas-calculo.md` §3):
//! - Línea normal: parcial = producto de los campos rellenos (uds × long × anch × alt).
//!   Un campo vacío no interviene; si todos están vacíos el parcial es 0.
//! - Las dimensiones se redondean a `Decimales::dimensiones` antes de multiplicar
//!   y cada parcial a `Decimales::medicion`.
//! - Línea de fórmula: la expresión usa `a`, `b`, `c`, `d` = uds, long, anch, alt
//!   (vacío = 0) y `p` = π. Correspondencia a confirmar con FIEBDC en P-004.
//! - Las líneas de subtotal no suman al total; solo informan.
//! - Total = suma de parciales ya redondeados.

use crate::decimales::{Decimales, redondear};
use crate::error::ErrorMotor;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum TipoLinea {
    #[default]
    Normal,
    /// Suma de las líneas desde el subtotal parcial anterior.
    SubtotalParcial,
    /// Suma de todas las líneas anteriores.
    SubtotalAcumulado,
    Formula,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct LineaMedicion {
    pub tipo: TipoLinea,
    pub comentario: String,
    pub unidades: Option<Decimal>,
    pub longitud: Option<Decimal>,
    pub anchura: Option<Decimal>,
    pub altura: Option<Decimal>,
    pub formula: Option<String>,
}

impl LineaMedicion {
    /// Línea normal. Pasar `None` en los campos vacíos.
    pub fn normal(
        comentario: &str,
        unidades: Option<Decimal>,
        longitud: Option<Decimal>,
        anchura: Option<Decimal>,
        altura: Option<Decimal>,
    ) -> Self {
        Self {
            tipo: TipoLinea::Normal,
            comentario: comentario.to_owned(),
            unidades,
            longitud,
            anchura,
            altura,
            formula: None,
        }
    }

    pub fn subtotal(comentario: &str) -> Self {
        Self {
            tipo: TipoLinea::SubtotalParcial,
            comentario: comentario.to_owned(),
            ..Default::default()
        }
    }

    fn dims(&self) -> [Option<Decimal>; 4] {
        [self.unidades, self.longitud, self.anchura, self.altura]
    }
}

/// Medición de una partida dentro de un capítulo.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Medicion {
    pub lineas: Vec<LineaMedicion>,
}

/// Resultado de calcular una medición: parciales línea a línea, total y avisos.
#[derive(Debug, Clone, PartialEq)]
pub struct ResultadoMedicion {
    pub parciales: Vec<Decimal>,
    pub total: Decimal,
    pub avisos: Vec<String>,
}

impl Medicion {
    pub fn new(lineas: Vec<LineaMedicion>) -> Self {
        Self { lineas }
    }

    /// Calcula parciales y total. `unidad` se usa para validar la coherencia
    /// dimensional (p. ej. una partida en m² con tres dimensiones rellenas).
    pub fn calcular(&self, dec: &Decimales, unidad: &str) -> Result<ResultadoMedicion, ErrorMotor> {
        let mut parciales = Vec::with_capacity(self.lineas.len());
        let mut avisos = Vec::new();
        let mut total = Decimal::ZERO;
        let mut desde_subtotal = Decimal::ZERO;

        for (i, l) in self.lineas.iter().enumerate() {
            let parcial = match l.tipo {
                TipoLinea::Normal => {
                    let rellenos: Vec<Decimal> = l
                        .dims()
                        .into_iter()
                        .enumerate()
                        .filter_map(|(k, v)| v.map(|x| if k == 0 { x } else { redondear(x, dec.dimensiones) }))
                        .collect();
                    if let Some(aviso) = validar_dimensiones(l, unidad) {
                        avisos.push(format!("línea {}: {aviso}", i + 1));
                    }
                    if rellenos.is_empty() {
                        Decimal::ZERO
                    } else {
                        redondear(rellenos.iter().product(), dec.medicion)
                    }
                }
                TipoLinea::Formula => {
                    let expr = l.formula.as_deref().unwrap_or("");
                    let [a, b, c, d] = l.dims().map(|v| v.unwrap_or(Decimal::ZERO));
                    redondear(evaluar_formula(expr, [a, b, c, d])?, dec.medicion)
                }
                TipoLinea::SubtotalParcial => {
                    let s = desde_subtotal;
                    desde_subtotal = Decimal::ZERO;
                    parciales.push(s);
                    continue;
                }
                TipoLinea::SubtotalAcumulado => {
                    parciales.push(total);
                    continue;
                }
            };
            parciales.push(parcial);
            total += parcial;
            desde_subtotal += parcial;
        }
        Ok(ResultadoMedicion {
            parciales,
            total: redondear(total, dec.medicion),
            avisos,
        })
    }
}

/// Número máximo de dimensiones geométricas coherente con la unidad, o `None`
/// si la unidad no es geométrica (h, kg, t, PA…).
fn dimensiones_esperadas(unidad: &str) -> Option<usize> {
    match unidad
        .trim()
        .to_lowercase()
        .replace('²', "2")
        .replace('³', "3")
        .as_str()
    {
        "u" | "ud" | "uds" | "un" => Some(0),
        "m" | "ml" => Some(1),
        "m2" => Some(2),
        "m3" => Some(3),
        _ => None,
    }
}

fn validar_dimensiones(l: &LineaMedicion, unidad: &str) -> Option<String> {
    let esperadas = dimensiones_esperadas(unidad)?;
    let usadas = [l.longitud, l.anchura, l.altura].iter().filter(|v| v.is_some()).count();
    (usadas > esperadas).then(|| {
        format!("{usadas} dimensiones rellenas para una unidad «{unidad}» (se esperan como máximo {esperadas})")
    })
}

// ---------------------------------------------------------------------------
// Evaluador de fórmulas de medición en aritmética decimal exacta.
// Gramática: expr := term (('+'|'-') term)* ; term := unario (('*'|'/') unario)* ;
//            unario := '-' unario | pot ; pot := prim ('^' unario)? ;
//            (así −a^2 = −(a²), como en notación matemática)
//            prim := número | a | b | c | d | p | '(' expr ')'
// ---------------------------------------------------------------------------

pub fn evaluar_formula(expr: &str, vars: [Decimal; 4]) -> Result<Decimal, ErrorMotor> {
    let mut p = Parser {
        s: expr.as_bytes(),
        i: 0,
        expr,
        vars,
    };
    let v = p.expr()?;
    p.ws();
    if p.i != p.s.len() {
        return Err(p.err("sobran caracteres al final"));
    }
    Ok(v)
}

struct Parser<'a> {
    s: &'a [u8],
    i: usize,
    expr: &'a str,
    vars: [Decimal; 4],
}

impl Parser<'_> {
    fn err(&self, motivo: &str) -> ErrorMotor {
        ErrorMotor::FormulaInvalida {
            formula: self.expr.to_owned(),
            motivo: format!("{motivo} (posición {})", self.i + 1),
        }
    }
    fn ws(&mut self) {
        while self.i < self.s.len() && self.s[self.i].is_ascii_whitespace() {
            self.i += 1;
        }
    }
    fn peek(&mut self) -> Option<u8> {
        self.ws();
        self.s.get(self.i).copied()
    }
    fn expr(&mut self) -> Result<Decimal, ErrorMotor> {
        let mut v = self.term()?;
        while let Some(c @ (b'+' | b'-')) = self.peek() {
            self.i += 1;
            let r = self.term()?;
            v = if c == b'+' { v + r } else { v - r };
        }
        Ok(v)
    }
    fn term(&mut self) -> Result<Decimal, ErrorMotor> {
        let mut v = self.unario()?;
        while let Some(c @ (b'*' | b'/')) = self.peek() {
            self.i += 1;
            let r = self.unario()?;
            if c == b'*' {
                v *= r;
            } else {
                if r.is_zero() {
                    return Err(ErrorMotor::DivisionPorCero(self.expr.to_owned()));
                }
                v /= r;
            }
        }
        Ok(v)
    }
    fn pot(&mut self) -> Result<Decimal, ErrorMotor> {
        let base = self.prim()?;
        if self.peek() == Some(b'^') {
            self.i += 1;
            let e = self.unario()?;
            if !e.fract().is_zero() || e < Decimal::ZERO || e > Decimal::from(10) {
                return Err(self.err("el exponente debe ser un entero entre 0 y 10"));
            }
            let n = rust_decimal::prelude::ToPrimitive::to_u32(&e).unwrap_or(0);
            let mut r = Decimal::ONE;
            for _ in 0..n {
                r *= base;
            }
            return Ok(r);
        }
        Ok(base)
    }
    fn unario(&mut self) -> Result<Decimal, ErrorMotor> {
        if self.peek() == Some(b'-') {
            self.i += 1;
            return Ok(-self.unario()?);
        }
        self.pot()
    }
    fn prim(&mut self) -> Result<Decimal, ErrorMotor> {
        match self.peek() {
            Some(b'(') => {
                self.i += 1;
                let v = self.expr()?;
                if self.peek() != Some(b')') {
                    return Err(self.err("falta ')'"));
                }
                self.i += 1;
                Ok(v)
            }
            Some(c) if c.is_ascii_digit() || c == b'.' => {
                let ini = self.i;
                while self.i < self.s.len() && (self.s[self.i].is_ascii_digit() || self.s[self.i] == b'.') {
                    self.i += 1;
                }
                let txt = std::str::from_utf8(&self.s[ini..self.i]).unwrap_or("");
                txt.parse::<Decimal>().map_err(|_| self.err("número mal escrito"))
            }
            Some(c) if c.is_ascii_alphabetic() => {
                self.i += 1;
                if self.s.get(self.i).is_some_and(|n| n.is_ascii_alphanumeric()) {
                    return Err(self.err("variable desconocida (use a, b, c, d o p)"));
                }
                match c.to_ascii_lowercase() {
                    b'a' => Ok(self.vars[0]),
                    b'b' => Ok(self.vars[1]),
                    b'c' => Ok(self.vars[2]),
                    b'd' => Ok(self.vars[3]),
                    b'p' => Ok(Decimal::PI),
                    _ => Err(self.err("variable desconocida (use a, b, c, d o p)")),
                }
            }
            Some(_) => Err(self.err("carácter no válido")),
            None => Err(self.err("expresión incompleta")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    fn d() -> Decimales {
        Decimales::default()
    }

    #[test]
    fn producto_de_campos_rellenos_y_negativos() {
        let m = Medicion::new(vec![
            LineaMedicion::normal("Planta baja", Some(dec!(1)), Some(dec!(12.50)), Some(dec!(8.00)), None),
            LineaMedicion::normal("Baños", Some(dec!(2)), Some(dec!(3.20)), Some(dec!(2.50)), None),
            LineaMedicion::normal(
                "Hueco escalera",
                Some(dec!(-1)),
                Some(dec!(2.00)),
                Some(dec!(3.00)),
                None,
            ),
            LineaMedicion::normal("Vacía", None, None, None, None),
        ]);
        let r = m.calcular(&d(), "m2").unwrap();
        assert_eq!(r.parciales, vec![dec!(100.00), dec!(16.00), dec!(-6.00), dec!(0)]);
        assert_eq!(r.total, dec!(110.00));
        assert!(r.avisos.is_empty());
    }

    #[test]
    fn dimensiones_se_redondean_antes_de_multiplicar() {
        // 12,345 se ve como 12,35 → 12,35 × 3 = 37,05 (no 37,035 → 37,04)
        let m = Medicion::new(vec![LineaMedicion::normal(
            "x",
            Some(dec!(3)),
            Some(dec!(12.345)),
            None,
            None,
        )]);
        assert_eq!(m.calcular(&d(), "m").unwrap().total, dec!(37.05));
    }

    #[test]
    fn subtotales_no_suman() {
        let m = Medicion::new(vec![
            LineaMedicion::normal("a", Some(dec!(2)), None, None, None),
            LineaMedicion::normal("b", Some(dec!(3)), None, None, None),
            LineaMedicion::subtotal("Planta 1"),
            LineaMedicion::normal("c", Some(dec!(4)), None, None, None),
            LineaMedicion::subtotal("Planta 2"),
            LineaMedicion {
                tipo: TipoLinea::SubtotalAcumulado,
                ..Default::default()
            },
        ]);
        let r = m.calcular(&d(), "ud").unwrap();
        assert_eq!(r.parciales, vec![dec!(2), dec!(3), dec!(5), dec!(4), dec!(4), dec!(9)]);
        assert_eq!(r.total, dec!(9));
    }

    #[test]
    fn aviso_por_incoherencia_dimensional() {
        let m = Medicion::new(vec![LineaMedicion::normal(
            "losa",
            None,
            Some(dec!(5)),
            Some(dec!(4)),
            Some(dec!(0.3)),
        )]);
        let r = m.calcular(&d(), "m2").unwrap();
        assert_eq!(r.avisos.len(), 1);
        assert!(r.avisos[0].contains("3 dimensiones"));
    }

    #[test]
    fn formulas() {
        let v = [dec!(2), dec!(3), dec!(0.5), dec!(0)];
        assert_eq!(evaluar_formula("a*b*c", v).unwrap(), dec!(3.0));
        assert_eq!(evaluar_formula("(a+b)*2 - c", v).unwrap(), dec!(9.5));
        assert_eq!(evaluar_formula("-a^2", v).unwrap(), dec!(-4));
        // Superficie de un círculo de diámetro 0,5 m: π·d²/4 = 0,19635
        let circ = evaluar_formula("p*c^2/4", v).unwrap();
        assert_eq!(redondear(circ, 4), dec!(0.1963));
        assert!(matches!(evaluar_formula("a/d", v), Err(ErrorMotor::DivisionPorCero(_))));
        assert!(matches!(
            evaluar_formula("a*x", v),
            Err(ErrorMotor::FormulaInvalida { .. })
        ));
        assert!(matches!(
            evaluar_formula("(a+b", v),
            Err(ErrorMotor::FormulaInvalida { .. })
        ));
        assert!(matches!(
            evaluar_formula("a^0.5", v),
            Err(ErrorMotor::FormulaInvalida { .. })
        ));
    }

    #[test]
    fn linea_de_formula_en_medicion() {
        let m = Medicion::new(vec![LineaMedicion {
            tipo: TipoLinea::Formula,
            comentario: "Pilares circulares".into(),
            unidades: Some(dec!(4)),
            longitud: Some(dec!(0.40)),
            altura: Some(dec!(3.00)),
            formula: Some("a*p*b^2/4*d".into()),
            ..Default::default()
        }]);
        // 4 · π · 0,16 / 4 · 3 = 1,50796 → 1,51 m³
        assert_eq!(m.calcular(&d(), "m3").unwrap().total, dec!(1.51));
    }
}
