// SPDX-License-Identifier: GPL-3.0-or-later
//! Decimales y redondeo.
//!
//! Los ámbitos replican los que define el registro `~K` de FIEBDC-3 para que
//! la ida y vuelta BC3 no pierda información. La correspondencia exacta campo
//! a campo con `~K` se cierra en la tarea P-004 (`docs/bc3/matriz-registros.md`).

use rust_decimal::{Decimal, RoundingStrategy};
use serde::{Deserialize, Serialize};

/// Redondeo comercial: mitad alejándose de cero (2,675 → 2,68; −0,125 → −0,13).
pub fn redondear(valor: Decimal, decimales: u32) -> Decimal {
    valor.round_dp_with_strategy(decimales, RoundingStrategy::MidpointAwayFromZero)
}

/// Número de decimales por ámbito de cálculo.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Decimales {
    /// Dimensiones de las líneas de medición (longitud, anchura, altura).
    pub dimensiones: u32,
    /// Parciales y totales de medición (cantidad de una partida en su capítulo).
    pub medicion: u32,
    /// Rendimientos de las líneas de descomposición.
    pub rendimiento: u32,
    /// Importe de cada línea de un descompuesto (rendimiento × precio).
    pub importe_linea: u32,
    /// Precio unitario de partidas y precios auxiliares.
    pub precio: u32,
    /// Importes de partidas en capítulo y totales de capítulo.
    pub importe: u32,
}

impl Default for Decimales {
    /// Valores habituales en presupuestos españoles de edificación e
    /// instalaciones (configurables por presupuesto).
    fn default() -> Self {
        Self {
            dimensiones: 2,
            medicion: 2,
            rendimiento: 3,
            importe_linea: 2,
            precio: 2,
            importe: 2,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn redondeo_comercial_mitad_alejandose_de_cero() {
        assert_eq!(redondear(dec!(2.675), 2), dec!(2.68));
        assert_eq!(redondear(dec!(0.125), 2), dec!(0.13));
        assert_eq!(redondear(dec!(1.004), 2), dec!(1.00));
        assert_eq!(redondear(dec!(-0.125), 2), dec!(-0.13));
        assert_eq!(redondear(dec!(6.125), 2), dec!(6.13));
        assert_eq!(redondear(dec!(2.67449), 3), dec!(2.674));
    }

    #[test]
    fn redondeo_no_es_bancario() {
        // El redondeo bancario daría 0,12; en presupuestación se exige 0,13.
        assert_ne!(redondear(dec!(0.125), 2), dec!(0.12));
    }
}
