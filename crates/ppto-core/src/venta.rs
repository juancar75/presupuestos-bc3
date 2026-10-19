// SPDX-License-Identifier: GPL-3.0-or-later
//! Costes indirectos, precio de venta, márgenes y resumen PEM → PEC.
//!
//! Distinción clave (P-008):
//! - **Recargo sobre coste** (*markup*): venta = coste × (1 + r).
//! - **Margen sobre venta**: venta = coste / (1 − m). Con m = 30 %, el margen
//!   es el 30 % del precio de venta, lo que equivale a un recargo del 42,86 %.
//!
//! Todos los porcentajes se pasan en puntos (30 = 30 %).

use crate::decimales::redondear;
use crate::error::ErrorMotor;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;

fn pct(p: Decimal) -> Decimal {
    p / Decimal::ONE_HUNDRED
}

/// Precio con costes indirectos: CD × (1 + K/100).
pub fn con_costes_indirectos(coste_directo: Decimal, k_pct: Decimal, decimales: u32) -> Decimal {
    redondear(coste_directo * (Decimal::ONE + pct(k_pct)), decimales)
}

/// Precio neto tras un descuento sobre tarifa.
pub fn aplicar_descuento(tarifa: Decimal, descuento_pct: Decimal, decimales: u32) -> Result<Decimal, ErrorMotor> {
    if descuento_pct < Decimal::ZERO || descuento_pct > Decimal::ONE_HUNDRED {
        return Err(ErrorMotor::ValorInvalido(format!(
            "descuento {descuento_pct} % fuera de 0–100"
        )));
    }
    Ok(redondear(tarifa * (Decimal::ONE - pct(descuento_pct)), decimales))
}

pub fn venta_por_recargo(coste: Decimal, recargo_pct: Decimal, decimales: u32) -> Decimal {
    redondear(coste * (Decimal::ONE + pct(recargo_pct)), decimales)
}

pub fn venta_por_margen(coste: Decimal, margen_pct: Decimal, decimales: u32) -> Result<Decimal, ErrorMotor> {
    if margen_pct >= Decimal::ONE_HUNDRED {
        return Err(ErrorMotor::ValorInvalido(format!(
            "un margen sobre venta del {margen_pct} % no es alcanzable (debe ser < 100 %)"
        )));
    }
    Ok(redondear(coste / (Decimal::ONE - pct(margen_pct)), decimales))
}

/// Margen sobre venta realmente obtenido, en %.
pub fn margen_obtenido(coste: Decimal, venta: Decimal) -> Result<Decimal, ErrorMotor> {
    if venta.is_zero() {
        return Err(ErrorMotor::ValorInvalido("precio de venta nulo".into()));
    }
    Ok((venta - coste) / venta * Decimal::ONE_HUNDRED)
}

/// Recargo sobre coste equivalente a un margen sobre venta: r = m / (1 − m).
pub fn recargo_equivalente(margen_pct: Decimal) -> Result<Decimal, ErrorMotor> {
    if margen_pct >= Decimal::ONE_HUNDRED {
        return Err(ErrorMotor::ValorInvalido("margen ≥ 100 %".into()));
    }
    Ok(pct(margen_pct) / (Decimal::ONE - pct(margen_pct)) * Decimal::ONE_HUNDRED)
}

/// Resumen de presupuesto: PEM, gastos generales, beneficio industrial, IVA.
#[derive(Debug, Clone, PartialEq)]
pub struct ResumenPresupuesto {
    pub pem: Decimal,
    pub gastos_generales: Decimal,
    pub beneficio_industrial: Decimal,
    /// PEM + GG + BI (presupuesto base de licitación sin IVA).
    pub base: Decimal,
    pub iva: Decimal,
    pub total: Decimal,
}

/// Cada componente se redondea a `decimales` y el total es la suma de los
/// componentes redondeados (lo que se imprime cuadra).
pub fn resumen(pem: Decimal, gg_pct: Decimal, bi_pct: Decimal, iva_pct: Decimal, decimales: u32) -> ResumenPresupuesto {
    let gg = redondear(pem * pct(gg_pct), decimales);
    let bi = redondear(pem * pct(bi_pct), decimales);
    let base = pem + gg + bi;
    let iva = redondear(base * pct(iva_pct), decimales);
    ResumenPresupuesto {
        pem,
        gastos_generales: gg,
        beneficio_industrial: bi,
        base,
        iva,
        total: base + iva,
    }
}

/// Precio de venta de una partida (sin IVA) desde su precio de ejecución
/// material: `precio + GG + BI`, con GG y BI redondeados como en el resumen.
/// Es el desglose que justifica la venta partida a partida.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VentaPartida {
    /// Precio de ejecución material (coste directo + costes indirectos).
    pub pem: Decimal,
    /// Margen propio de la partida (% sobre el PEM) en lugar de GG + BI.
    pub margen_propio: Option<Decimal>,
    /// Importe del margen propio (cero si usa GG + BI).
    pub margen: Decimal,
    pub gastos_generales: Decimal,
    pub beneficio_industrial: Decimal,
    /// PEM + GG + BI.
    pub venta: Decimal,
}

pub fn venta_partida(pem: Decimal, gg_pct: Decimal, bi_pct: Decimal, decimales: u32) -> VentaPartida {
    let r = resumen(pem, gg_pct, bi_pct, Decimal::ZERO, decimales);
    VentaPartida {
        pem,
        margen_propio: None,
        margen: Decimal::ZERO,
        gastos_generales: r.gastos_generales,
        beneficio_industrial: r.beneficio_industrial,
        venta: r.base,
    }
}

/// Como [`venta_partida`], pero con el margen propio de la partida si lo
/// tiene: venta = PEM + PEM × margen (sin GG ni BI).
pub fn venta_con_margen(
    pem: Decimal,
    margen_propio: Option<Decimal>,
    gg_pct: Decimal,
    bi_pct: Decimal,
    decimales: u32,
) -> VentaPartida {
    match margen_propio {
        None => venta_partida(pem, gg_pct, bi_pct, decimales),
        Some(m) => {
            let margen = redondear(pem * pct(m), decimales);
            VentaPartida {
                pem,
                margen_propio: Some(m),
                margen,
                gastos_generales: Decimal::ZERO,
                beneficio_industrial: Decimal::ZERO,
                venta: pem + margen,
            }
        }
    }
}

/// Valores habituales en obra pública (art. 131 RGLCAP): GG 13 %, BI 6 %.
pub const GG_HABITUAL: Decimal = dec!(13);
pub const BI_HABITUAL: Decimal = dec!(6);
pub const IVA_GENERAL: Decimal = dec!(21);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn venta_de_una_partida() {
        let v = venta_partida(dec!(599.05), dec!(13), dec!(6), 2);
        assert_eq!(v.gastos_generales, dec!(77.88));
        assert_eq!(v.beneficio_industrial, dec!(35.94));
        assert_eq!(v.venta, dec!(712.87));
        // Margen propio del 30 %: 599,05 + 179,72 = 778,77
        let m = venta_con_margen(dec!(599.05), Some(dec!(30)), dec!(13), dec!(6), 2);
        assert_eq!(
            (m.margen, m.venta, m.gastos_generales),
            (dec!(179.72), dec!(778.77), dec!(0))
        );
        assert_eq!(venta_con_margen(dec!(599.05), None, dec!(13), dec!(6), 2), v);
    }

    #[test]
    fn margen_30_sobre_venta_frente_a_recargo_30_sobre_coste() {
        let coste = dec!(100.00);
        let por_margen = venta_por_margen(coste, dec!(30), 2).unwrap();
        let por_recargo = venta_por_recargo(coste, dec!(30), 2);
        assert_eq!(por_margen, dec!(142.86));
        assert_eq!(por_recargo, dec!(130.00));
        // Comprobación inversa: el margen obtenido es 30 % (± redondeo de 2 dec.)
        let m = margen_obtenido(coste, por_margen).unwrap();
        assert_eq!(redondear(m, 2), dec!(30.00));
        // Un recargo del 30 % solo da un margen del 23,08 %
        assert_eq!(redondear(margen_obtenido(coste, por_recargo).unwrap(), 2), dec!(23.08));
        // Margen 30 % ≡ recargo 42,86 %
        assert_eq!(redondear(recargo_equivalente(dec!(30)).unwrap(), 2), dec!(42.86));
    }

    #[test]
    fn margen_imposible() {
        assert!(venta_por_margen(dec!(100), dec!(100), 2).is_err());
        assert!(venta_por_margen(dec!(100), dec!(120), 2).is_err());
    }

    #[test]
    fn descuento_sobre_tarifa_e_indirectos() {
        // Tarifa 1.250,00 € con 45 % de descuento → 687,50 €
        assert_eq!(aplicar_descuento(dec!(1250.00), dec!(45), 2).unwrap(), dec!(687.50));
        assert!(aplicar_descuento(dec!(1), dec!(101), 2).is_err());
        // CD 29,89 € con K = 3 % → 30,79 € (30,7867)
        assert_eq!(con_costes_indirectos(dec!(29.89), dec!(3), 2), dec!(30.79));
    }

    #[test]
    fn resumen_pem_a_pec() {
        let r = resumen(dec!(7005.15), GG_HABITUAL, BI_HABITUAL, IVA_GENERAL, 2);
        assert_eq!(r.gastos_generales, dec!(910.67)); // 910,6695
        assert_eq!(r.beneficio_industrial, dec!(420.31)); // 420,309
        assert_eq!(r.base, dec!(8336.13));
        assert_eq!(r.iva, dec!(1750.59)); // 1750,5873
        assert_eq!(r.total, dec!(10086.72));
    }
}
