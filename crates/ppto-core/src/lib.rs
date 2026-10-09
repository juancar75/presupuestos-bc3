// SPDX-License-Identifier: GPL-3.0-or-later
//! # ppto-core
//!
//! Motor económico del gestor de presupuestos: modelo de conceptos alineado
//! con FIEBDC-3 (BC3), precios descompuestos jerárquicos, mediciones,
//! redondeo comercial, explosión de recursos, costes/venta y escenarios de
//! sustitución de mano de obra propia por subcontratación.
//!
//! Reglas de diseño (ver `docs/reglas-calculo.md`):
//! - Toda la aritmética económica usa [`rust_decimal::Decimal`]; los `f64`
//!   están prohibidos por lint (`clippy::float_arithmetic`).
//! - Redondeo comercial: mitad alejándose de cero (0,125 → 0,13).
//! - Criterio «lo que se ve es lo que se multiplica»: cada importe se calcula
//!   con los operandos ya redondeados a sus decimales de presentación, como
//!   hacen Presto y la mayoría de programas españoles.
//! - El presupuesto es inmutable para los cálculos de escenarios: un escenario
//!   nunca modifica el presupuesto de origen.

pub mod concepto;
pub mod decimales;
pub mod ejemplos;
pub mod error;
pub mod explosion;
pub mod formato;
pub mod medicion;
pub mod presupuesto;
pub mod subcontrata;
pub mod venta;

pub use concepto::{Concepto, LineaDescomposicion, Naturaleza};
pub use decimales::{Decimales, redondear};
pub use error::ErrorMotor;
pub use medicion::{LineaMedicion, Medicion, TipoLinea};
pub use presupuesto::Presupuesto;
pub use rust_decimal::Decimal;
